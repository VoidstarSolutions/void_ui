//! Column descriptors for [`super::data_grid`].
//!
//! A column is defined by its [`title`](ColumnDef::title), its fixed
//! pixel [`width`](ColumnDef::width), its in-cell text
//! [`align`](ColumnDef::align)ment, and a [`render`](ColumnDef::render)
//! closure that builds a cell view from a row.
//!
//! The renderer signature is intentionally widget-returning rather than
//! string-returning so that future cells can be richer than text — a
//! colored chip for `Side`, a sparkline for `Volume`, etc. The two
//! ergonomic helpers [`text_column`] and [`optional_text_column`] cover
//! the common case (plain text driven by a `Fn(&R) -> String`
//! projection) without making callers think about boxing.

use xilem::AnyWidgetView;
use xilem::style::Style as _;
use xilem::view::label;

use crate::Theme;

/// In-cell horizontal alignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CellAlign {
    /// Pack to the leading edge — natural for text columns.
    #[default]
    Start,
    /// Center within the cell — natural for short status glyphs.
    Center,
    /// Pack to the trailing edge — natural for numeric columns.
    End,
}

/// Type-erased cell-builder used by every [`ColumnDef`].
///
/// The closure is `Send + 'static` (but **not** `Sync`) so callers can
/// capture e.g. an `Rc<…>` or a non-`Sync` formatter without fighting
/// the type system. xilem only needs single-threaded access during
/// rebuild.
pub type CellRenderer<R, State> =
    Box<dyn Fn(&R, &Theme) -> Box<AnyWidgetView<State>> + Send + 'static>;

/// Describes one column in a [`super::data_grid`] view.
///
/// `R` is the row type — every column in a single grid renders from the
/// same row. `State` is the host application's app state. Cells are
/// non-interactive in v1 (the row container intercepts pointer events
/// for selection), so the action type is fixed at `()`.
#[must_use]
pub struct ColumnDef<R, State> {
    /// Display title shown in the sticky header row.
    pub title: String,
    /// Fixed pixel width. The caller is responsible for ensuring the
    /// sum across columns fits the grid's viewport; otherwise the
    /// rightmost columns clip (a `tracing::warn!` is emitted in debug
    /// builds on first under-sized layout).
    pub width: f64,
    /// In-cell text alignment.
    pub align: CellAlign,
    /// Builds a cell view for the supplied row at the supplied theme.
    pub render: CellRenderer<R, State>,
}

impl<R, State> ColumnDef<R, State> {
    /// Constructs a column from an explicit cell-builder. Most callers
    /// should prefer [`text_column`] or [`optional_text_column`].
    pub fn new<F>(title: impl Into<String>, width: f64, align: CellAlign, render: F) -> Self
    where
        F: Fn(&R, &Theme) -> Box<AnyWidgetView<State>> + Send + 'static,
    {
        Self {
            title: title.into(),
            width,
            align,
            render: Box::new(render),
        }
    }
}

/// A column whose cells are plain text labels.
///
/// `fmt` projects a row into the text shown in that row's cell. The
/// label inherits the active theme's body font size and primary text
/// color.
pub fn text_column<R, State, F>(
    title: impl Into<String>,
    width: f64,
    align: CellAlign,
    fmt: F,
) -> ColumnDef<R, State>
where
    R: 'static,
    State: 'static,
    F: Fn(&R) -> String + Send + 'static,
{
    ColumnDef::new(title, width, align, move |row, theme| {
        let text = fmt(row);
        let view = label(text)
            .text_size(theme.typography.size_body)
            .color(theme.palette.text);
        Box::new(view)
    })
}

/// A column whose cells are plain text when the projection returns
/// `Some` and a faint em-dash placeholder when it returns `None`.
///
/// Useful for variant-flattened rows like `ColumnDelta`: a column that
/// applies only to one of the variants returns `None` for the others
/// and the cell renders as `—` in the theme's faint text color.
pub fn optional_text_column<R, State, F>(
    title: impl Into<String>,
    width: f64,
    align: CellAlign,
    fmt: F,
) -> ColumnDef<R, State>
where
    R: 'static,
    State: 'static,
    F: Fn(&R) -> Option<String> + Send + 'static,
{
    ColumnDef::new(title, width, align, move |row, theme| {
        let (text, color) = match fmt(row) {
            Some(s) => (s, theme.palette.text),
            None => ("—".to_string(), theme.palette.text_faint),
        };
        let view = label(text)
            .text_size(theme.typography.size_body)
            .color(color);
        Box::new(view)
    })
}
