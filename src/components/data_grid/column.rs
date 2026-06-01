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

use std::cmp::Ordering;

use xilem::AnyWidgetView;
use xilem::peniko::Color;
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

/// Total order over two rows for a sortable column.
///
/// Returning a comparator rather than a key projector keeps the column
/// generic over the row type without forcing the sort key to be an
/// owned, `'static` value — the closure borrows each row only for the
/// duration of the comparison. The grid wraps the result with the
/// active [`SortDirection`](super::sort::SortDirection): the comparator
/// always describes the *ascending* order, and descending is the
/// reverse. `Send + Sync` for the same reason as
/// [`CellRenderer`] — xilem propagates the bound through the row
/// builder closure.
pub type RowComparator<R> = Box<dyn Fn(&R, &R) -> Ordering + Send + Sync + 'static>;

/// Predicate that tests a row against a column's filter query.
///
/// Given a row and the active query string for this column, returns
/// whether the row should survive the filter. The grid never calls this
/// itself — the host runs it via
/// [`filtered_indices`](super::filter::filtered_indices). `Send + Sync`
/// for the same reason as [`CellRenderer`].
pub type RowFilter<R> = Box<dyn Fn(&R, &str) -> bool + Send + Sync + 'static>;

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
    /// Fixed pixel width. When the column total exceeds the viewport the
    /// grid scrolls horizontally (header/filter/body share the offset),
    /// so wide tables stay reachable rather than clipping.
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
    /// Optional ascending-order comparator that makes this column
    /// sortable. `None` (the default) leaves the column unsortable —
    /// its header doesn't react to clicks. Attach one via
    /// [`ColumnDef::sortable_by`] or [`ColumnDef::sortable_by_key`].
    ///
    /// Deliberately *not* auto-populated by [`text_column`]: a column
    /// whose cells render a formatted number (e.g. `"$100.00"`) would
    /// sort lexicographically — wrong — if it inherited a string
    /// comparator from its display text. Sortable columns opt in with
    /// a key that reflects the underlying value.
    pub comparator: Option<RowComparator<R>>,
    /// Optional predicate that makes this column filterable. `None`
    /// (the default) leaves the column unfilterable. Attach one via
    /// [`ColumnDef::filterable_by`] or [`ColumnDef::filterable_by_text`].
    /// The host applies it through
    /// [`filtered_indices`](super::filter::filtered_indices); the grid
    /// uses its presence to decide whether to show a filter affordance.
    pub filter: Option<RowFilter<R>>,
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
            comparator: None,
            filter: None,
        }
    }

    /// Makes this column filterable with an explicit predicate:
    /// `Fn(&row, &query) -> bool`. Reach for this when matching needs
    /// more than a case-insensitive substring (numeric thresholds,
    /// ranges, custom parsing); most callers want
    /// [`ColumnDef::filterable_by_text`].
    pub fn filterable_by<F>(mut self, predicate: F) -> Self
    where
        F: Fn(&R, &str) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Box::new(predicate));
        self
    }

    /// Makes this column filterable by a case-insensitive substring
    /// match against text projected from each row — the common case
    /// (`.filterable_by_text(|r| r.symbol.clone())`). A row passes when
    /// the projected text contains the query, ignoring case.
    pub fn filterable_by_text<F>(self, key: F) -> Self
    where
        F: Fn(&R) -> String + Send + Sync + 'static,
    {
        self.filterable_by(move |row, query| {
            key(row).to_lowercase().contains(&query.to_lowercase())
        })
    }

    /// Makes this column sortable using an explicit ascending-order
    /// comparator. The grid applies the active
    /// [`SortDirection`](super::sort::SortDirection) on top — pass the
    /// comparator that describes the *ascending* order and the grid
    /// reverses it for descending.
    ///
    /// Most callers want [`ColumnDef::sortable_by_key`]; reach for this
    /// when ordering needs more than a single `Ord` key (e.g. a custom
    /// collation or a multi-field tiebreak).
    pub fn sortable_by<F>(mut self, comparator: F) -> Self
    where
        F: Fn(&R, &R) -> Ordering + Send + Sync + 'static,
    {
        self.comparator = Some(Box::new(comparator));
        self
    }

    /// Makes this column sortable by a comparable key projected from
    /// each row. The common case: `.sortable_by_key(|r| r.price_units)`.
    ///
    /// The key is computed twice per comparison rather than cached, so
    /// keep the projection cheap (a field read or a `Copy` value). For
    /// expensive keys, precompute them onto the row type instead.
    pub fn sortable_by_key<K, F>(self, key: F) -> Self
    where
        K: Ord,
        F: Fn(&R) -> K + Send + Sync + 'static,
    {
        self.sortable_by(move |a, b| key(a).cmp(&key(b)))
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
            .color(theme.palette.text);
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
            .color(color);
        Box::new(view)
    })
    // Clipboard cells get an empty string for None, not "—" — the
    // em dash is presentation-only; a spreadsheet target wants the
    // structural absence.
    .with_text(move |row| fmt_for_text(row).unwrap_or_default())
}

/// A text column whose label color is computed per row — the building
/// block for conditional formatting (e.g. green gains / red losses, or
/// categorical coloring).
///
/// `fmt` projects the displayed (and clipboard) text exactly like
/// [`text_column`]; `color` picks the label color from the row and the
/// active [`Theme`] (so it resolves correctly across theme variants —
/// pass `theme.palette.green` / `theme.palette.coral`, etc.). Number
/// and string *formatting* is just the `fmt` closure; this adds the
/// conditional *color* on top.
pub fn colored_text_column<R, State, F, C>(
    title: impl Into<String>,
    width: f64,
    align: CellAlign,
    fmt: F,
    color: C,
) -> ColumnDef<R, State>
where
    R: 'static,
    State: 'static,
    F: Fn(&R) -> String + Send + Sync + 'static,
    C: Fn(&R, &Theme) -> Color + Send + Sync + 'static,
{
    let fmt = std::sync::Arc::new(fmt);
    let fmt_for_render = std::sync::Arc::clone(&fmt);
    let fmt_for_text = std::sync::Arc::clone(&fmt);
    ColumnDef::new(title, width, align, move |row, theme| {
        let view = label(fmt_for_render(row))
            .text_size(theme.typography.size_body)
            .color(color(row, theme));
        Box::new(view)
    })
    .with_text(move |row| fmt_for_text(row))
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{CellAlign, ColumnDef, text_column};

    #[derive(Clone)]
    struct Row {
        n: i64,
        name: &'static str,
    }

    #[test]
    fn columns_are_unsortable_by_default() {
        let col: ColumnDef<Row, ()> =
            text_column("N", 10.0, CellAlign::End, |r: &Row| r.n.to_string());
        assert!(
            col.comparator.is_none(),
            "text_column must not auto-attach a comparator"
        );
    }

    #[test]
    fn sortable_by_key_orders_by_the_underlying_value() {
        // The display projection stringifies, which would sort
        // lexicographically; the key sorts numerically instead.
        let col: ColumnDef<Row, ()> =
            text_column("N", 10.0, CellAlign::End, |r: &Row| r.n.to_string())
                .sortable_by_key(|r: &Row| r.n);
        let cmp = col.comparator.expect("sortable_by_key attaches a comparator");
        let small = Row { n: 9, name: "a" };
        let large = Row { n: 100, name: "b" };
        // 9 < 100 numerically, even though "100" < "9" as strings.
        assert_eq!(cmp(&small, &large), Ordering::Less);
        assert_eq!(cmp(&large, &small), Ordering::Greater);
        assert_eq!(cmp(&small, &small), Ordering::Equal);
    }

    #[test]
    fn sortable_by_uses_the_explicit_comparator() {
        let col: ColumnDef<Row, ()> =
            text_column("Name", 10.0, CellAlign::Start, |r: &Row| r.name.to_string())
                .sortable_by(|a: &Row, b: &Row| a.name.cmp(b.name));
        let cmp = col.comparator.expect("sortable_by attaches a comparator");
        let a = Row { n: 0, name: "alpha" };
        let b = Row { n: 0, name: "beta" };
        assert_eq!(cmp(&a, &b), Ordering::Less);
    }
}
