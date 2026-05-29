//! Xilem [`View`] composition for the data grid.
//!
//! - The body is `flex_col(header, virtual_scroll(body))` — pure xilem
//!   stock with a row builder closure that materializes one
//!   `flex_row` of fixed-width cells per loaded index. Each row is
//!   wrapped in [`super::row_click::clickable_row`] so primary clicks
//!   (with optional shift / ctrl-cmd modifiers) update the
//!   [`SelectionState`].
//! - The grid is wrapped in [`super::overflow_warn::OverflowWarn`]
//!   to log a one-shot `tracing::warn!` when the viewport is
//!   narrower than the sum of column widths, then in
//!   [`CopyOnShortcutView`], a private wrapper for the
//!   [`CopyOnShortcut`](super::CopyOnShortcut) masonry widget that
//!   pushes a fresh TSV projection of the current selection on every
//!   rebuild and dumps it to the platform clipboard on Ctrl/Cmd+C.
//! - Selected rows are styled with the theme's `surface_2` panel
//!   color.

use std::marker::PhantomData;
use std::sync::{Arc, Mutex, PoisonError};

use xilem::WidgetView;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    AnyFlexChild, CrossAxisAlignment, MainAxisAlignment, flex_col, flex_item, flex_row, label,
    sized_box, virtual_scroll,
};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use super::column::{CellAlign, CellRenderer, ColumnDef, RowComparator, TextProjector};
use super::copy_shortcut::CopyOnShortcut;
use super::header_click::clickable_header;
use super::overflow_warn::overflow_warn;
use super::row_click::clickable_row;
use super::selection::SelectionState;
use super::filter::FilterState;
use super::sort::{SortDirection, SortState, display_order};
use crate::Theme;

/// Boxed row-data accessor (`Fn(&State) -> &[R]`), shared via `Arc`
/// across the body and clipboard closures.
type RowsFn<State, R> = Arc<dyn for<'a> Fn(&'a State) -> &'a [R] + Send + Sync>;
/// Boxed selection lens (`Fn(&mut State) -> &mut SelectionState`).
type SelectionLens<State> =
    Arc<dyn for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync>;
/// Boxed sort lens (`Fn(&mut State) -> &mut SortState`), the write path
/// for header-click cycling.
type SortLens<State> = Arc<dyn for<'a> Fn(&'a mut State) -> &'a mut SortState + Send + Sync>;

/// One column's rendering + layout slot — the half of [`ColumnDef`]
/// that's needed at row-build time. Shared (via `Arc`) between the
/// header builder and the row builder closures.
struct ColumnRender<R, State> {
    title: String,
    width: f64,
    align: CellAlign,
    render: CellRenderer<R, State>,
}

/// Translates [`CellAlign`] into the [`MainAxisAlignment`] used by the
/// single-child flex wrapper that aligns the cell view inside its
/// fixed-width slot.
const fn align_to_main(align: CellAlign) -> MainAxisAlignment {
    match align {
        CellAlign::Start => MainAxisAlignment::Start,
        CellAlign::Center => MainAxisAlignment::Center,
        CellAlign::End => MainAxisAlignment::End,
    }
}

/// Memoized sorted display order, shared (behind a `Mutex`) into the
/// `virtual_scroll` row builder.
///
/// `virtual_scroll` invokes the row builder once per *visible* row.
/// Re-sorting the whole dataset inside each call would be O(n log n)
/// *per row*; instead we recompute only when the sort inputs change —
/// keyed on `(column, direction, row_count)` — so a rebuild sorts at
/// most once and every row after the first reuses the cached `Arc`.
///
/// `row_count` is a sufficient data-version proxy for the append-only
/// streams this grid targets (see the module docs): an append changes
/// the length and invalidates the cache. In-place row mutation that
/// preserves length is out of scope for v1 (the grid is read-only).
#[derive(Default)]
struct SortOrderCache {
    key: Option<(Option<usize>, SortDirection, usize)>,
    order: Arc<Vec<usize>>,
}

/// Map a virtual (on-screen) row index to its source row index under
/// the active sort, recomputing the memoized order only when the sort
/// inputs change.
///
/// Returns `None` when the virtual index falls outside the data (e.g.
/// a row scrolled into view past the end of a shrinking dataset). The
/// unsorted case is the hot path: an identity mapping with no lock and
/// no allocation.
fn resolve_source_idx<R>(
    sort: SortState,
    comparators: &[Option<RowComparator<R>>],
    cache: &Mutex<SortOrderCache>,
    data: &[R],
    virtual_idx: usize,
) -> Option<usize> {
    let Some(col) = sort.column() else {
        return Some(virtual_idx);
    };
    let key = (Some(col), sort.direction(), data.len());
    // Recover the guard if a prior holder panicked — the critical
    // section can't itself panic, so the cached order is still valid.
    let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if cache.key != Some(key) {
        let comparator = comparators.get(col).and_then(Option::as_ref);
        cache.order = Arc::new(display_order(data, comparator, sort.direction()));
        cache.key = Some(key);
    }
    cache.order.get(virtual_idx).copied()
}

/// Builder for a virtualized, theme-driven data grid view.
///
/// Construct with [`DataGrid::new`], attach data and behavior through
/// the chained setters, then materialize the xilem view with
/// [`DataGrid::render`]. Lenses are stored boxed, so each future
/// feature is a new method rather than another positional parameter.
///
/// ```ignore
/// DataGrid::new(columns)
///     .rows(|s: &State| &s.ticks[..])
///     .row_count(n)
///     .selection(|s| &mut s.selection)
///     .sort(sort_snapshot, |s| &mut s.sort)
///     .row_height(22.0)
///     .render(&theme)
/// ```
///
/// `selection` and `sort` are optional — omit them for a
/// non-selectable / unsorted grid.
///
/// # Notes
///
/// - If `sum(column.width) > viewport_width`, columns clip off the
///   right edge (a one-shot `tracing::warn!` fires). Horizontal
///   scroll/resize is a roadmap item.
/// - The clipboard (TSV) payload is rebuilt every frame from the
///   current selection; columns without a `text` projector contribute
///   empty cells so spreadsheet paste keeps the column layout.
#[must_use = "DataGrid does nothing until rendered with .render(&theme)"]
pub struct DataGrid<State, R> {
    columns: Vec<ColumnDef<R, State>>,
    row_count: u64,
    row_height: f64,
    rows: Option<RowsFn<State, R>>,
    selection_lens: Option<SelectionLens<State>>,
    sort: SortState,
    sort_lens: Option<SortLens<State>>,
    filter: FilterState,
}

/// Default fixed row height when [`DataGrid::row_height`] is unset.
const DEFAULT_ROW_HEIGHT: f64 = 24.0;

impl<State, R> DataGrid<State, R>
where
    State: 'static,
    R: 'static,
{
    /// Starts a grid from its column descriptors. Attach data with
    /// [`Self::rows`] + [`Self::row_count`] before rendering.
    pub fn new(columns: Vec<ColumnDef<R, State>>) -> Self {
        Self {
            columns,
            row_count: 0,
            row_height: DEFAULT_ROW_HEIGHT,
            rows: None,
            selection_lens: None,
            sort: SortState::new(),
            sort_lens: None,
            filter: FilterState::new(),
        }
    }

    /// Sets the row-data accessor. Required for the grid to show data;
    /// without it the body renders empty.
    pub fn rows<F>(mut self, rows: F) -> Self
    where
        F: for<'a> Fn(&'a State) -> &'a [R] + Send + Sync + 'static,
    {
        let rows: RowsFn<State, R> = Arc::new(rows);
        self.rows = Some(rows);
        self
    }

    /// Sets the current row count — the body virtualizes over
    /// `0..row_count`. Snapshot it from host state at frame time.
    pub fn row_count(mut self, row_count: u64) -> Self {
        self.row_count = row_count;
        self
    }

    /// Fixed pixel row height (defaults to [`DEFAULT_ROW_HEIGHT`]).
    pub fn row_height(mut self, row_height: f64) -> Self {
        self.row_height = row_height;
        self
    }

    /// Enables row selection via a lens into the host's
    /// [`SelectionState`]. Selection tracks *source* row indices, so it
    /// stays attached to the same data rows across sort changes. Omit
    /// for a non-selectable grid.
    pub fn selection<F>(mut self, lens: F) -> Self
    where
        F: for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync + 'static,
    {
        let lens: SelectionLens<State> = Arc::new(lens);
        self.selection_lens = Some(lens);
        self
    }

    /// Enables sorting: `state` is the current [`SortState`] snapshot
    /// (drives the header arrow + the virtual→source row order) and
    /// `lens` is the write path for header-click cycling. A column is
    /// only sortable if its [`ColumnDef`] carries a comparator (see
    /// [`ColumnDef::sortable_by_key`]). Omit for an unsorted grid.
    pub fn sort<F>(mut self, state: SortState, lens: F) -> Self
    where
        F: for<'a> Fn(&'a mut State) -> &'a mut SortState + Send + Sync + 'static,
    {
        let lens: SortLens<State> = Arc::new(lens);
        self.sort = state;
        self.sort_lens = Some(lens);
        self
    }

    /// Supplies the active [`FilterState`] snapshot so the grid can mark
    /// filtered columns with a persistent indicator. Filtering itself is
    /// applied host-side (see
    /// [`filtered_indices`](super::filter::filtered_indices)); this is
    /// the *display* half — a column with an active query gets an
    /// always-visible accent + marker so a filtered view is never
    /// mistaken for the full data set.
    pub fn filter(mut self, filter: FilterState) -> Self {
        self.filter = filter;
        self
    }

    /// Materializes the xilem view at the supplied theme.
    #[must_use]
    pub fn render(self, theme: &Theme) -> impl WidgetView<State, ()> + use<State, R> {
        build_grid_view(self, theme)
    }
}

/// The parts of a `Vec<ColumnDef>` the view layer needs, decomposed
/// into parallel, `Arc`-shared vectors: per-column rendering slots,
/// clipboard text projectors, a `sortable` flag, and comparators.
type DecomposedColumns<R, State> = (
    Arc<Vec<ColumnRender<R, State>>>,
    Arc<Vec<Option<TextProjector<R>>>>,
    Vec<bool>,
    Arc<Vec<Option<RowComparator<R>>>>,
);

/// Splits each [`ColumnDef`] into its rendering slot, clipboard text
/// projector, and comparator (positionally aligned), plus a `sortable`
/// flag per column. The `Arc`s are shared between the synchronous
/// header builder and the `virtual_scroll` row-builder closure.
fn decompose_columns<R, State>(columns: Vec<ColumnDef<R, State>>) -> DecomposedColumns<R, State> {
    let mut render_slots: Vec<ColumnRender<R, State>> = Vec::with_capacity(columns.len());
    let mut text_projectors: Vec<Option<TextProjector<R>>> = Vec::with_capacity(columns.len());
    let mut comparators: Vec<Option<RowComparator<R>>> = Vec::with_capacity(columns.len());
    for col in columns {
        text_projectors.push(col.text);
        comparators.push(col.comparator);
        render_slots.push(ColumnRender {
            title: col.title,
            width: col.width,
            align: col.align,
            render: col.render,
        });
    }
    let sortable: Vec<bool> = comparators.iter().map(Option::is_some).collect();
    (
        Arc::new(render_slots),
        Arc::new(text_projectors),
        sortable,
        Arc::new(comparators),
    )
}

/// Turns a finished [`DataGrid`] builder into the view tree. Kept as a
/// free function so the body stays flat and `render` is a thin call.
fn build_grid_view<State, R>(
    grid: DataGrid<State, R>,
    theme: &Theme,
) -> impl WidgetView<State, ()> + use<State, R>
where
    State: 'static,
    R: 'static,
{
    let theme = *theme;
    let DataGrid {
        columns,
        row_count,
        row_height,
        rows,
        selection_lens,
        sort,
        sort_lens,
        filter,
    } = grid;

    // Default the data accessor to an empty slice when unset.
    let rows: RowsFn<State, R> = rows.unwrap_or_else(|| {
        let empty: RowsFn<State, R> = Arc::new(|_: &State| -> &[R] { &[] });
        empty
    });

    let (render_slots, text_projectors, sortable, comparators) = decompose_columns(columns);

    // --- Header row. Sortable columns get a clickable header that
    //     cycles the column's sort, plus an arrow on the active
    //     column; columns with an active filter get a persistent
    //     accent + marker. Non-sortable columns render an inert label.
    let header_cells: Vec<AnyFlexChild<State, ()>> = render_slots
        .iter()
        .enumerate()
        .map(|(idx, slot)| {
            let filtered = filter.get(idx).is_some();
            header_cell(idx, slot, sortable[idx], filtered, sort, sort_lens.as_ref(), &theme)
        })
        .collect();
    let header = sized_box(flex_row(header_cells).cross_axis_alignment(CrossAxisAlignment::Center))
        .fixed_height(Length::px(row_height))
        .background_color(theme.palette.surface_2)
        .border(theme.palette.border, Length::px(1.0));

    // --- Body: virtual_scroll. The row builder captures the
    //     rendering slots Arc, the data accessor, the per-column
    //     comparators, the selection + sort lenses, and a shared
    //     sort-order memo. Each visible row is mapped from its
    //     *virtual* position to a *source* row index through the
    //     active sort order; selection styling, rendering, and the
    //     click handler all operate on that source index.
    let valid_range_end = i64::try_from(row_count).unwrap_or(i64::MAX);
    let render_slots_for_body = Arc::clone(&render_slots);
    let rows_for_body = rows.clone();
    let selection_lens_for_body = selection_lens.clone();
    let sort_cache = Arc::new(Mutex::new(SortOrderCache::default()));
    let body = virtual_scroll(0..valid_range_end, move |state: &mut State, idx: i64| {
        let virtual_idx = usize::try_from(idx).unwrap_or(usize::MAX);

        // Map virtual row → source row through the active sort order.
        // `sort` is the frame-time snapshot captured by this closure;
        // it can't change without a rebuild (which re-captures it).
        let source_idx = resolve_source_idx(
            sort,
            &comparators,
            &sort_cache,
            (*rows_for_body)(state),
            virtual_idx,
        );

        let is_selected = match (selection_lens_for_body.as_ref(), source_idx) {
            (Some(sel), Some(s)) => (**sel)(state).contains(u64::try_from(s).unwrap_or(u64::MAX)),
            _ => false,
        };

        let data = (*rows_for_body)(state);
        let cells: Vec<AnyFlexChild<State, ()>> =
            if let Some(row) = source_idx.and_then(|s| data.get(s)) {
                render_slots_for_body
                    .iter()
                    .map(|slot| {
                        let cell_view = (slot.render)(row, &theme);
                        let cell = aligned_cell(cell_view, slot.width, slot.align);
                        flex_item(cell, 0.0).into()
                    })
                    .collect()
            } else {
                Vec::new()
            };

        let row_bg = if is_selected {
            theme.palette.surface_2
        } else {
            Color::TRANSPARENT
        };
        let row_view = sized_box(flex_row(cells).cross_axis_alignment(CrossAxisAlignment::Center))
            .fixed_height(Length::px(row_height))
            .background_color(row_bg);

        // Each row's click handler closes over the (cloned) selection
        // lens + the row's *source* index. Modifiers route to the
        // matching SelectionState op.
        //
        // KNOWN LIMITATION (v1): shift-extend fills an inclusive range
        // in *source-index* space, so under an active sort it does not
        // track the visual range between anchor and target. Single
        // click and ctrl/cmd-toggle are order-independent and correct.
        // Visual-range extend is deferred to a follow-up.
        let lens_for_click = selection_lens_for_body.clone();
        clickable_row(row_view, move |state: &mut State, action| {
            let Some(source) = source_idx else { return };
            let Ok(row) = u64::try_from(source) else {
                return;
            };
            let Some(sel_lens) = lens_for_click.as_ref() else {
                return;
            };
            let sel = (**sel_lens)(state);
            if action.shift {
                sel.extend_to(row);
            } else if action.action_mod {
                sel.toggle(row);
            } else {
                sel.replace_with(row);
            }
        })
    });

    // The body flexes to fill the height the grid is given; the header
    // keeps its fixed row height. A virtualized grid must fill a
    // *bounded* viewport (it can't size itself to the full row count),
    // so callers should place the grid in a bounded-height slot — e.g.
    // `sized_box(grid).flex(1.0)`. In an unbounded-height parent the
    // body falls back to its intrinsic size (a few rows).
    let stack =
        flex_col((header, flex_item(body, 1.0))).cross_axis_alignment(CrossAxisAlignment::Start);

    // --- Wrap in OverflowWarn so a viewport narrower than the sum of
    //     column widths emits a one-shot tracing::warn! — backs the
    //     doc claim on ColumnDef and helps callers notice when their
    //     column configuration silently clips on the right.
    let sum_widths: f64 = render_slots.iter().map(|slot| slot.width).sum();
    let inner = overflow_warn(stack, sum_widths);

    // --- Wrap in CopyOnShortcut so Ctrl/Cmd+C dumps the
    //     selection-projected TSV. The wrapper captures the text
    //     projectors Arc, the data accessor, and the selection lens
    //     so it can recompute the payload on every rebuild.
    CopyOnShortcutView {
        child: inner,
        text_projectors,
        rows,
        selection_lens,
        phantom: PhantomData,
    }
}

// --- MARK: TSV projection ----------------------------------------------

fn project_tsv<R>(
    text_projectors: &[Option<TextProjector<R>>],
    data: &[R],
    selection: &SelectionState,
) -> Option<String> {
    if selection.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut first_row = true;
    for idx_u64 in selection.iter() {
        let i = usize::try_from(idx_u64).unwrap_or(usize::MAX);
        let Some(row) = data.get(i) else { continue };
        if !first_row {
            out.push('\n');
        }
        first_row = false;
        let mut first_col = true;
        for projector in text_projectors {
            if !first_col {
                out.push('\t');
            }
            first_col = false;
            if let Some(p) = projector {
                let cell = p(row);
                // Literal-escape tab/newline so they don't break the
                // row × column layout in the spreadsheet target.
                out.push_str(&cell.replace('\t', "\\t").replace('\n', "\\n"));
            }
        }
    }
    Some(out)
}

// --- MARK: CopyOnShortcutView ------------------------------------------

/// Internal wrapper view that pushes a TSV payload into a
/// [`CopyOnShortcut`] widget on every rebuild. Stores the boxed row
/// accessor and the optional selection lens so it can recompute the
/// clipboard payload from the current selection each frame.
struct CopyOnShortcutView<V, R, State> {
    child: V,
    text_projectors: Arc<Vec<Option<TextProjector<R>>>>,
    rows: RowsFn<State, R>,
    selection_lens: Option<SelectionLens<State>>,
    phantom: PhantomData<fn() -> State>,
}

impl<V, R, State> ViewMarker for CopyOnShortcutView<V, R, State> {}

impl<V, R, State> View<State, (), ViewCtx> for CopyOnShortcutView<V, R, State>
where
    V: WidgetView<State, ()>,
    R: 'static,
    State: 'static,
{
    type Element = Pod<CopyOnShortcut>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let payload = self.compute_payload(app_state);
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = CopyOnShortcut::new(child_pod.new_widget).with_payload(payload);
        (ctx.create_pod(widget), child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let payload = self.compute_payload(app_state);
        CopyOnShortcut::set_payload(&mut element, payload);
        let mut child = CopyOnShortcut::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = CopyOnShortcut::child_mut(&mut element);
        self.child.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        let mut child = CopyOnShortcut::child_mut(&mut element);
        self.child
            .message(view_state, message, child.downcast(), app_state)
    }
}

impl<V, R, State> CopyOnShortcutView<V, R, State>
where
    R: 'static,
    State: 'static,
{
    fn compute_payload(&self, app_state: &mut State) -> Option<String> {
        // No selection lens → nothing to copy.
        let selection_lens = self.selection_lens.as_ref()?;
        // We need both `&[R]` and `&SelectionState` simultaneously,
        // but the lenses return references whose lifetimes overlap
        // app_state. Walk the selection first to copy out the
        // indices, then look up rows.
        let selection_snapshot = (**selection_lens)(app_state).clone();
        let data = (*self.rows)(app_state);
        project_tsv(&self.text_projectors, data, &selection_snapshot)
    }
}

// --- MARK: CELL ALIGNMENT ---------------------------------------------

/// Wraps a cell view in a fixed-width `sized_box` with the cell's
/// content packed via a single-child `flex_row` whose
/// `main_axis_alignment` is derived from [`CellAlign`].
///
/// Returning a boxed `AnyWidgetView` keeps the concrete type out of
/// the call sites in the header / body builders (which already
/// erase further).
fn aligned_cell<State: 'static>(
    inner: Box<AnyWidgetView<State>>,
    width: f64,
    align: CellAlign,
) -> Box<AnyWidgetView<State>> {
    let aligned = flex_row((flex_item(inner, 0.0),))
        .main_axis_alignment(align_to_main(align))
        .cross_axis_alignment(CrossAxisAlignment::Center);
    Box::new(sized_box(aligned).fixed_width(Length::px(width)))
}

// --- MARK: HEADER CELL -------------------------------------------------

/// Builds one header cell as a fixed-width flex child.
///
/// Sortable columns (`sortable == true`) are wrapped in
/// [`clickable_header`] so a click cycles `sort_lens`'s state for that
/// column, and the active sort column gains an ascending/descending
/// arrow. A column with an active filter (`filtered == true`) is drawn
/// in the theme accent with a trailing marker, so a filtered view is
/// always unmistakable. Non-sortable columns render an inert label.
fn header_cell<State, R>(
    idx: usize,
    slot: &ColumnRender<R, State>,
    sortable: bool,
    filtered: bool,
    sort: SortState,
    sort_lens: Option<&SortLens<State>>,
    theme: &Theme,
) -> AnyFlexChild<State, ()>
where
    State: 'static,
    R: 'static,
{
    let mut title = match sort.direction_for(idx) {
        Some(SortDirection::Ascending) => format!("{}  ▲", slot.title),
        Some(SortDirection::Descending) => format!("{}  ▼", slot.title),
        None => slot.title.clone(),
    };
    // Persistent filter indicator: a filtered column gets a trailing
    // marker and the theme accent color, so a hidden-data view can't be
    // mistaken for the full set (even after the trigger loses focus).
    if filtered {
        title.push_str("  ●");
    }
    let title_color = if filtered {
        theme.palette.teal
    } else {
        theme.palette.text_muted
    };
    let header_label = label(title)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(title_color);
    let cell = aligned_cell(Box::new(header_label), slot.width, slot.align);
    // Interactive only when the column is sortable *and* a write lens
    // is available; otherwise an inert label.
    match (sortable, sort_lens) {
        (true, Some(lens)) => {
            let lens = Arc::clone(lens);
            let clickable = clickable_header(
                cell,
                theme.palette.border_strong,
                move |state: &mut State| {
                    (*lens)(state).cycle(idx);
                },
            );
            flex_item(clickable, 0.0).into()
        }
        _ => flex_item(cell, 0.0).into(),
    }
}
