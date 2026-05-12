//! Xilem [`View`] composition for the data grid.
//!
//! Pure composition over xilem stock: a `flex_col` of a sticky header
//! row above a `virtual_scroll`-driven body. The body's row builder
//! captures the column descriptors plus a row accessor closure and
//! materializes one [`flex_row`] per index on demand — only the rows
//! actually inside the viewport are built, so a 1M-row grid pays the
//! cost of ~30 visible rows × N columns of cell views per frame.
//!
//! Selection visuals and Ctrl/Cmd+C handling (via
//! [`super::copy_shortcut::CopyOnShortcut`]) arrive in a follow-up
//! commit; v0 is display-only.

use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{
    AnyFlexChild, CrossAxisAlignment, flex_col, flex_item, flex_row, label, sized_box,
    virtual_scroll,
};

use super::column::ColumnDef;
use crate::Theme;

/// Builds a virtualized data grid view.
///
/// # Parameters
///
/// - `columns` — column descriptors, consumed; each column's renderer
///   closure is captured into the row builder.
/// - `row_count` — current row count. The body's virtual scroll uses
///   `0..row_count` as its valid index range. When live data appends,
///   the caller passes a larger `row_count` on the next rebuild; the
///   underlying widget grows its valid range without disturbing
///   currently-loaded rows.
/// - `rows` — `Fn(&State) -> &[R]` accessor. The grid's row builder
///   closure must be `'static`, so it cannot borrow a `&[R]` from the
///   outer scope; instead it stores this accessor and calls it on the
///   `&mut State` provided by `virtual_scroll` at materialization
///   time. Must be `Clone` so the row builder can take its own copy.
/// - `theme` — color/typography source. Captured by value (Copy).
/// - `row_height` — fixed pixel row height.
///
/// # Notes
///
/// - If `sum(column.width) > viewport_width`, columns clip off the
///   right edge — `VirtualScroll`'s internal clip rect handles this.
///   A v2 follow-up may add horizontal scroll syncing.
/// - The row builder is invoked on every rebuild for every loaded
///   index, so column renderers should be cheap.
pub fn data_grid<State, R, FRows>(
    columns: Vec<ColumnDef<R, State>>,
    row_count: u64,
    rows: FRows,
    theme: &Theme,
    row_height: f64,
) -> impl WidgetView<State, ()> + use<State, R, FRows>
where
    State: 'static,
    R: 'static,
    FRows: for<'a> Fn(&'a State) -> &'a [R] + Clone + Send + Sync + 'static,
{
    let theme = *theme;

    // --- Header row (sticky-top look, doesn't scroll).
    let header_cells: Vec<AnyFlexChild<State, ()>> = columns
        .iter()
        .map(|col| {
            let header_label = label(col.title.clone())
                .text_size(theme.typography.size_caption)
                .letter_spacing(1.2)
                .color(theme.palette.text_muted);
            let cell = sized_box(header_label).fixed_width(Length::px(col.width));
            flex_item(cell, 0.0).into()
        })
        .collect();
    let header = sized_box(flex_row(header_cells).cross_axis_alignment(CrossAxisAlignment::Center))
        .fixed_height(Length::px(row_height))
        .background_color(theme.palette.surface_2)
        .border(theme.palette.border, 1.0);

    // --- Body: virtual_scroll. Move `columns` and `rows` into the
    //     row builder so we can call them on demand for each loaded
    //     index. The closure is `Fn`, not `FnMut` — iterating columns
    //     uses `&self`, and calling `rows(state)` uses the captured
    //     `Fn` accessor.
    let valid_range_end = i64::try_from(row_count).unwrap_or(i64::MAX);
    let body = virtual_scroll(0..valid_range_end, move |state: &mut State, idx: i64| {
        let data = rows(state);
        let row_idx = usize::try_from(idx).unwrap_or(usize::MAX);
        // Defensive: if the valid range shrinks due to upstream
        // truncation, virtual_scroll may still request the old idx
        // before it observes the bounds change. Emit an empty row
        // rather than panicking.
        let cells: Vec<AnyFlexChild<State, ()>> = if let Some(row) = data.get(row_idx) {
            columns
                .iter()
                .map(|col| {
                    let cell_view = (col.render)(row, &theme);
                    let cell = sized_box(cell_view).fixed_width(Length::px(col.width));
                    flex_item(cell, 0.0).into()
                })
                .collect()
        } else {
            Vec::new()
        };
        sized_box(flex_row(cells).cross_axis_alignment(CrossAxisAlignment::Center))
            .fixed_height(Length::px(row_height))
    });

    flex_col((header, body)).cross_axis_alignment(CrossAxisAlignment::Start)
}
