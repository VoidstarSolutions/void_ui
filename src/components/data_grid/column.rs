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

use crate::Theme;
use crate::label;

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
/// `Send + Sync` is required because xilem propagates these bounds
/// through its view tree (the row builder we hand to `virtual_scroll`
/// must be `Sync`, which forces every captured value — including the
/// renderer closures — to be `Sync` too). Closures that own their
/// captures or hold only `Sync` types satisfy this automatically;
/// reach for `Arc<…>` over `Rc<…>` inside a renderer if you need
/// shared state.
pub type CellRenderer<R, State> =
    Box<dyn Fn(&R, &Theme) -> Box<AnyWidgetView<State>> + Send + Sync + 'static>;

/// Text-only projector used to materialize a row's cell value for
/// clipboard copy.
///
/// A [`ColumnDef`] may carry one of these alongside its widget-
/// returning [`render`](ColumnDef::render). When the user copies a
/// selection, the grid walks the selected rows and joins each
/// column's text projector output with tabs. Columns without a text
/// projector emit an empty string (a tab will still appear so the
/// row's columns line up in the spreadsheet target).
pub type TextProjector<R> = Box<dyn Fn(&R) -> String + Send + Sync + 'static>;

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
    /// Optional text-only projector used for clipboard copy. The
    /// helpers [`text_column`] and [`optional_text_column`] populate
    /// this automatically; custom callers using [`ColumnDef::new`]
    /// can attach one via [`ColumnDef::with_text`]. Columns without a
    /// text projector contribute an empty TSV cell.
    pub text: Option<TextProjector<R>>,
}

impl<R, State> ColumnDef<R, State> {
    /// Constructs a column from an explicit cell-builder. Most callers
    /// should prefer [`text_column`] or [`optional_text_column`].
    pub fn new<F>(title: impl Into<String>, width: f64, align: CellAlign, render: F) -> Self
    where
        F: Fn(&R, &Theme) -> Box<AnyWidgetView<State>> + Send + Sync + 'static,
    {
        Self {
            title: title.into(),
            width,
            align,
            render: Box::new(render),
            text: None,
        }
    }

    /// Attaches a text projector for clipboard copy. Builders chain
    /// `.with_text(...)` after [`ColumnDef::new`] when the column's
    /// cells aren't built by [`text_column`] / [`optional_text_column`]
    /// but still need to participate in copy.
    pub fn with_text<F>(mut self, projector: F) -> Self
    where
        F: Fn(&R) -> String + Send + Sync + 'static,
    {
        self.text = Some(Box::new(projector));
        self
    }
}

/// A column whose cells are plain text labels.
///
/// `fmt` projects a row into the text shown in that row's cell. The
/// label inherits the active theme's body font size and primary text
/// color. The same projector is wired into the column's clipboard
/// path (see [`ColumnDef::text`]).
pub fn text_column<R, State, F>(
    title: impl Into<String>,
    width: f64,
    align: CellAlign,
    fmt: F,
) -> ColumnDef<R, State>
where
    R: 'static,
    State: 'static,
    F: Fn(&R) -> String + Send + Sync + 'static,
{
    let fmt = std::sync::Arc::new(fmt);
    let fmt_for_render = std::sync::Arc::clone(&fmt);
    let fmt_for_text = std::sync::Arc::clone(&fmt);
    ColumnDef::new(title, width, align, move |row, theme| {
        let text = fmt_for_render(row);
        let view = label(text)
            .text_size(theme.typography.size_body)
            .color(theme.palette.text)
            .render(theme);
        Box::new(view)
    })
    .with_text(move |row| fmt_for_text(row))
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
    F: Fn(&R) -> Option<String> + Send + Sync + 'static,
{
    let fmt = std::sync::Arc::new(fmt);
    let fmt_for_render = std::sync::Arc::clone(&fmt);
    let fmt_for_text = std::sync::Arc::clone(&fmt);
    ColumnDef::new(title, width, align, move |row, theme| {
        let (text, color) = match fmt_for_render(row) {
            Some(s) => (s, theme.palette.text),
            None => ("—".to_string(), theme.palette.text_faint),
        };
        let view = label(text)
            .text_size(theme.typography.size_body)
            .color(color)
            .render(theme);
        Box::new(view)
    })
    // Clipboard cells get an empty string for None, not "—" — the
    // em dash is presentation-only; a spreadsheet target wants the
    // structural absence.
    .with_text(move |row| fmt_for_text(row).unwrap_or_default())
}
