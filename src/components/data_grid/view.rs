//! Xilem [`View`] composition for the data grid.
//!
//! - The body is `flex_col(header, virtual_scroll(body))` — pure xilem
//!   stock with a row builder closure that materializes one
//!   `flex_row` of fixed-width cells per loaded index.
//! - The grid is wrapped in [`CopyOnShortcutView`], a private wrapper
//!   for the [`CopyOnShortcut`](super::CopyOnShortcut) masonry widget.
//!   On every rebuild it pushes a fresh TSV projection of the current
//!   selection; the widget itself catches Ctrl/Cmd+C and dumps that
//!   payload to the platform clipboard.
//! - Selected rows are styled with the theme's `surface_2` panel
//!   color. v1 has no row-click widget yet; callers populate the
//!   [`SelectionState`] externally (a follow-up commit adds a
//!   modifier-aware row-click widget).

use std::marker::PhantomData;
use std::sync::Arc;

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

use super::column::{CellAlign, CellRenderer, ColumnDef, TextProjector};
use super::copy_shortcut::CopyOnShortcut;
use super::overflow_warn::overflow_warn;
use super::row_click::clickable_row;
use super::selection::SelectionState;
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
///   clipboard copy.
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
pub fn data_grid<State, R, FRows, FSel>(
    columns: Vec<ColumnDef<R, State>>,
    row_count: u64,
    rows: FRows,
    selection_lens: FSel,
    theme: &Theme,
    row_height: f64,
) -> impl WidgetView<State, ()> + use<State, R, FRows, FSel>
where
    State: 'static,
    R: 'static,
    FRows: for<'a> Fn(&'a State) -> &'a [R] + Clone + Send + Sync + 'static,
    FSel: for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Clone + Send + Sync + 'static,
{
    let theme = *theme;

    // Split each ColumnDef into a rendering slot + a clipboard text
    // projector. The rendering slots are shared between the header
    // builder (synchronous, here) and the row builder (captured by
    // the virtual_scroll closure) via Arc.
    let mut render_slots: Vec<ColumnRender<R, State>> = Vec::with_capacity(columns.len());
    let mut text_projectors: Vec<Option<TextProjector<R>>> = Vec::with_capacity(columns.len());
    for col in columns {
        text_projectors.push(col.text);
        render_slots.push(ColumnRender {
            title: col.title,
            width: col.width,
            align: col.align,
            render: col.render,
        });
    }
    let render_slots = Arc::new(render_slots);
    let text_projectors = Arc::new(text_projectors);

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
        .border(theme.palette.border, 1.0);

    // --- Body: virtual_scroll. The row builder captures the
    //     rendering slots Arc, the data accessor, and the selection
    //     lens. The lens is used in two ways: a read pass to decide
    //     selected-row styling, and a clone moved into each row's
    //     click handler so pointer-up can mutate the selection.
    let valid_range_end = i64::try_from(row_count).unwrap_or(i64::MAX);
    let render_slots_for_body = Arc::clone(&render_slots);
    let rows_for_body = rows.clone();
    let selection_lens_for_body = selection_lens.clone();
    let body = virtual_scroll(0..valid_range_end, move |state: &mut State, idx: i64| {
        let row_idx_u64 = u64::try_from(idx).unwrap_or(0);
        let is_selected = selection_lens_for_body(state).contains(row_idx_u64);

        let data = rows_for_body(state);
        let row_idx_usize = usize::try_from(idx).unwrap_or(usize::MAX);
        let cells: Vec<AnyFlexChild<State, ()>> = if let Some(row) = data.get(row_idx_usize) {
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

        // Each row gets its own click handler closing over the
        // (cloned) lens + the row's index. Modifiers route to the
        // matching SelectionState op.
        let lens_for_click = selection_lens_for_body.clone();
        clickable_row(row_view, move |state: &mut State, action| {
            let sel = lens_for_click(state);
            if action.shift {
                sel.extend_to(row_idx_u64);
            } else if action.action_mod {
                sel.toggle(row_idx_u64);
            } else {
                sel.replace_with(row_idx_u64);
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
