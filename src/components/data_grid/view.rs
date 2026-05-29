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
use super::overflow_warn::overflow_warn;
use super::row_click::clickable_row;
use super::selection::SelectionState;
use super::sort::{SortDirection, SortState, display_order};
use crate::Theme;

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

/// Builds a virtualized data grid view.
///
/// # Parameters
///
/// - `columns` — column descriptors, consumed; decomposed into
///   parallel rendering/clipboard slots that the closures share via
///   `Arc`.
/// - `row_count` — current row count. The body's virtual scroll uses
///   `0..row_count` as its valid index range. When live data appends,
///   the caller passes a larger `row_count` on the next rebuild.
/// - `rows` — `Fn(&State) -> &[R]` accessor used by both the row
///   builder (to look up rendered cells) and the TSV builder (to
///   look up clipboard text).
/// - `selection_lens` — `Fn(&mut State) -> &mut SelectionState`
///   accessor. The grid reads it to (a) decide which rows render in
///   the selected style, and (b) project the TSV payload for
///   clipboard copy. Selection tracks *source* row indices, so it is
///   stable across sort changes.
/// - `sort_lens` — `Fn(&mut State) -> &mut SortState` accessor. The
///   grid reads the active [`SortState`] to map each visible row to
///   its source row through the sorted display order. A column is
///   only sorted if its [`ColumnDef`] carries a comparator
///   (see [`ColumnDef::sortable_by_key`]); otherwise the state is
///   ignored and rows display in natural order.
/// - `theme` — color/typography source. Captured by value (Copy).
/// - `row_height` — fixed pixel row height.
///
/// # Notes
///
/// - If `sum(column.width) > viewport_width`, columns clip off the
///   right edge. v2 may add horizontal scroll syncing.
/// - The TSV payload is rebuilt every frame from the current
///   selection. Columns without a `text` projector contribute empty
///   cells (so spreadsheet paste keeps the column layout).
pub fn data_grid<State, R, FRows, FSel, FSort>(
    columns: Vec<ColumnDef<R, State>>,
    row_count: u64,
    rows: FRows,
    selection_lens: FSel,
    sort_lens: FSort,
    theme: &Theme,
    row_height: f64,
) -> impl WidgetView<State, ()> + use<State, R, FRows, FSel, FSort>
where
    State: 'static,
    R: 'static,
    FRows: for<'a> Fn(&'a State) -> &'a [R] + Clone + Send + Sync + 'static,
    FSel: for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Clone + Send + Sync + 'static,
    FSort: for<'a> Fn(&'a mut State) -> &'a mut SortState + Clone + Send + Sync + 'static,
{
    let theme = *theme;

    // Split each ColumnDef into a rendering slot + a clipboard text
    // projector. The rendering slots are shared between the header
    // builder (synchronous, here) and the row builder (captured by
    // the virtual_scroll closure) via Arc.
    let mut render_slots: Vec<ColumnRender<R, State>> = Vec::with_capacity(columns.len());
    let mut text_projectors: Vec<Option<TextProjector<R>>> = Vec::with_capacity(columns.len());
    // Per-column comparators (positionally aligned with render_slots).
    // Shared into the row builder so it can sort by the active column.
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
    let render_slots = Arc::new(render_slots);
    let text_projectors = Arc::new(text_projectors);
    let comparators = Arc::new(comparators);

    // --- Header row.
    let header_cells: Vec<AnyFlexChild<State, ()>> = render_slots
        .iter()
        .map(|slot| {
            let header_label = label(slot.title.clone())
                .text_size(theme.typography.size_caption)
                .letter_spacing(1.2)
                .color(theme.palette.text_muted);
            let cell = aligned_cell(Box::new(header_label), slot.width, slot.align);
            flex_item(cell, 0.0).into()
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

        // Snapshot the sort state (Copy) and release the borrow before
        // reborrowing `state` for the row data.
        let sort = *sort_lens(state);

        // Map virtual row → source row through the active sort order.
        let source_idx = resolve_source_idx(
            sort,
            &comparators,
            &sort_cache,
            rows_for_body(state),
            virtual_idx,
        );

        let is_selected = source_idx.is_some_and(|s| {
            selection_lens_for_body(state).contains(u64::try_from(s).unwrap_or(u64::MAX))
        });

        let data = rows_for_body(state);
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
            let Ok(row) = u64::try_from(source) else { return };
            let sel = lens_for_click(state);
            if action.shift {
                sel.extend_to(row);
            } else if action.action_mod {
                sel.toggle(row);
            } else {
                sel.replace_with(row);
            }
        })
    });

    let stack = flex_col((header, body)).cross_axis_alignment(CrossAxisAlignment::Start);

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
/// [`CopyOnShortcut`] widget on every rebuild.
struct CopyOnShortcutView<V, R, State, FRows, FSel> {
    child: V,
    text_projectors: Arc<Vec<Option<TextProjector<R>>>>,
    rows: FRows,
    selection_lens: FSel,
    phantom: PhantomData<fn() -> State>,
}

impl<V, R, State, FRows, FSel> ViewMarker for CopyOnShortcutView<V, R, State, FRows, FSel> {}

impl<V, R, State, FRows, FSel> View<State, (), ViewCtx>
    for CopyOnShortcutView<V, R, State, FRows, FSel>
where
    V: WidgetView<State, ()>,
    R: 'static,
    State: 'static,
    FRows: for<'a> Fn(&'a State) -> &'a [R] + Send + Sync + 'static,
    FSel: for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync + 'static,
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

impl<V, R, State, FRows, FSel> CopyOnShortcutView<V, R, State, FRows, FSel>
where
    R: 'static,
    State: 'static,
    FRows: for<'a> Fn(&'a State) -> &'a [R],
    FSel: for<'a> Fn(&'a mut State) -> &'a mut SelectionState,
{
    fn compute_payload(&self, app_state: &mut State) -> Option<String> {
        // We need both `&[R]` and `&SelectionState` simultaneously,
        // but the lenses return references whose lifetimes overlap
        // app_state. Walk the selection first to copy out the
        // indices, then look up rows.
        let selection_snapshot = (self.selection_lens)(app_state).clone();
        let data = (self.rows)(app_state);
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
