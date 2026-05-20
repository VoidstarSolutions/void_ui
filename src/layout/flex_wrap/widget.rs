//! Masonry widget that packs children left-to-right and wraps to a new
//! row when the next child would overflow the available width.
//!
//! Layout primitive — no painting, no event handling. Wraps any
//! collection of widgets (chips, tags, swatches, buttons) without the
//! call site having to know how many will fit per row.
//!
//! ## Algorithm
//!
//! `measure` and `layout` share a row-partitioning pass: children are
//! visited in order, their natural widths summed (with `col_gap`
//! between siblings), and a new row is started whenever adding the next
//! child would exceed the available width. Each row's height is the
//! height of its tallest child; total height is the sum of row heights
//! plus `row_gap` between adjacent rows.
//!
//! Within each row, [`MainAxisAlignment`] decides how the slack between
//! the row's natural width and the available width is distributed.
//! [`CrossAxisAlignment`] decides how each child sits inside its row's
//! height — `Stretch` is special-cased to re-resize the child to the
//! row's height before layout.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, CollectionWidget, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesRef, RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length, SizeDef};
use masonry::properties::types::{CrossAxisAlignment, MainAxisAlignment};

/// A wrapping flex container.
///
/// Children flow left-to-right. When the next child would overflow the
/// available width, the layout starts a new row. Each row's height is
/// the height of its tallest child; the total height is the sum of row
/// heights plus `row_gap` between adjacent rows.
///
/// Use [`Self::with`] / [`CollectionWidget::add`] to add children,
/// [`Self::with_gap`] (or `with_row_gap` / `with_col_gap`) to set
/// spacing, and the alignment builders to control how children pack
/// within each row.
pub struct FlexWrap {
    children: Vec<WidgetPod<dyn Widget>>,
    row_gap: f64,
    col_gap: f64,
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
}

// --- MARK: LAYOUT HELPERS

/// A single row in the wrapped layout — half-open range over the
/// `children` vector.
#[derive(Clone, Copy, Debug)]
struct Row {
    start: usize,
    end: usize,
}

/// Walks `widths` in order and groups them into rows. A new row starts
/// whenever appending the next width (plus `col_gap`) would exceed
/// `available`. A single child wider than `available` still becomes its
/// own row — overflow is preferred to dropping the child.
fn partition_rows(widths: &[f64], available: f64, col_gap: f64) -> Vec<Row> {
    let mut rows = Vec::new();
    if widths.is_empty() {
        return rows;
    }
    let mut start = 0;
    let mut width_acc = widths[0];
    for (i, &w) in widths.iter().enumerate().skip(1) {
        let candidate = width_acc + col_gap + w;
        if candidate > available && i > start {
            rows.push(Row { start, end: i });
            start = i;
            width_acc = w;
        } else {
            width_acc = candidate;
        }
    }
    rows.push(Row {
        start,
        end: widths.len(),
    });
    rows
}

/// Returns the x-position of each child within a row given the row's
/// child widths, the row's available width, the configured `col_gap`,
/// and the [`MainAxisAlignment`].
///
/// For `Space*` alignments, the natural `col_gap` is replaced by an
/// effective gap distributing whatever slack the row has. For `n == 1`
/// the `Space*` cases degenerate to centering.
fn main_axis_positions(
    widths: &[f64],
    available: f64,
    col_gap: f64,
    alignment: MainAxisAlignment,
) -> Vec<f64> {
    let n = widths.len();
    if n == 0 {
        return Vec::new();
    }
    let content_width: f64 = widths.iter().sum();
    #[expect(
        clippy::cast_precision_loss,
        reason = "child counts are small (<< 2^53)"
    )]
    let n_f = n as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "child counts are small (<< 2^53)"
    )]
    let gap_count = (n.saturating_sub(1)) as f64;
    let natural_total = content_width + col_gap * gap_count;
    let slack = (available - natural_total).max(0.0);

    // The `Space*` modes distribute slack between children. When
    // content overflows the row (`available < content_width`), the
    // raw slack is negative and naively dividing would shift the
    // first child to a negative x and overlap the rest. Clamp to
    // `>= 0.0` so overflow falls back to natural `col_gap` packing
    // from x=0 — matching the `Start`/`Center`/`End` behavior above.
    let extra = (available - content_width).max(0.0);
    let (mut x, gap) = match alignment {
        MainAxisAlignment::Start => (0.0, col_gap),
        MainAxisAlignment::Center => (slack / 2.0, col_gap),
        MainAxisAlignment::End => (slack, col_gap),
        MainAxisAlignment::SpaceBetween => {
            if n == 1 {
                (0.0, 0.0)
            } else {
                (0.0, extra / gap_count)
            }
        }
        MainAxisAlignment::SpaceEvenly => {
            let g = extra / (n_f + 1.0);
            (g, g)
        }
        MainAxisAlignment::SpaceAround => {
            let pad = extra / n_f;
            (pad / 2.0, pad)
        }
    };

    let mut positions = Vec::with_capacity(n);
    for (i, &w) in widths.iter().enumerate() {
        positions.push(x);
        if i + 1 < n {
            x += w + gap;
        }
    }
    positions
}

// --- MARK: BUILDERS
impl FlexWrap {
    /// Creates an empty `FlexWrap` with zero gaps and start-axis
    /// alignment on both axes.
    #[must_use]
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            row_gap: 0.0,
            col_gap: 0.0,
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: CrossAxisAlignment::Start,
        }
    }

    /// Appends a child widget.
    #[must_use]
    pub fn with(mut self, child: NewWidget<impl Widget + ?Sized>) -> Self {
        self.children.push(child.erased().to_pod());
        self
    }

    /// Sets both `row_gap` and `col_gap` to the same value.
    #[must_use]
    pub fn with_gap(mut self, gap: f64) -> Self {
        self.row_gap = gap;
        self.col_gap = gap;
        self
    }

    /// Sets the spacing between rows.
    #[must_use]
    pub fn with_row_gap(mut self, gap: f64) -> Self {
        self.row_gap = gap;
        self
    }

    /// Sets the spacing between children within a row.
    #[must_use]
    pub fn with_col_gap(mut self, gap: f64) -> Self {
        self.col_gap = gap;
        self
    }

    /// Sets how children are distributed within a row when the row's
    /// content is narrower than the container.
    #[must_use]
    pub fn with_main_axis_alignment(mut self, alignment: MainAxisAlignment) -> Self {
        self.main_axis_alignment = alignment;
        self
    }

    /// Sets how children align vertically within their row (each row's
    /// height being the tallest child in that row).
    #[must_use]
    pub fn with_cross_axis_alignment(mut self, alignment: CrossAxisAlignment) -> Self {
        self.cross_axis_alignment = alignment;
        self
    }
}

impl Default for FlexWrap {
    fn default() -> Self {
        Self::new()
    }
}

// --- MARK: WIDGETMUT
impl FlexWrap {
    /// Sets the spacing between rows. Requests layout on change.
    pub fn set_row_gap(this: &mut WidgetMut<'_, Self>, gap: f64) {
        #[expect(
            clippy::float_cmp,
            reason = "skip layout only when the value is bit-identical; epsilon comparison would silently swallow caller-intended updates"
        )]
        if this.widget.row_gap != gap {
            this.widget.row_gap = gap;
            this.ctx.request_layout();
        }
    }

    /// Sets the spacing between children within a row. Requests layout
    /// on change.
    pub fn set_col_gap(this: &mut WidgetMut<'_, Self>, gap: f64) {
        #[expect(
            clippy::float_cmp,
            reason = "skip layout only when the value is bit-identical; epsilon comparison would silently swallow caller-intended updates"
        )]
        if this.widget.col_gap != gap {
            this.widget.col_gap = gap;
            this.ctx.request_layout();
        }
    }

    /// Sets both gaps in one call. Requests layout on change.
    pub fn set_gap(this: &mut WidgetMut<'_, Self>, gap: f64) {
        Self::set_row_gap(this, gap);
        Self::set_col_gap(this, gap);
    }

    /// Sets the main-axis alignment. Requests layout on change.
    pub fn set_main_axis_alignment(this: &mut WidgetMut<'_, Self>, alignment: MainAxisAlignment) {
        if this.widget.main_axis_alignment != alignment {
            this.widget.main_axis_alignment = alignment;
            this.ctx.request_layout();
        }
    }

    /// Sets the cross-axis alignment. Requests layout on change.
    pub fn set_cross_axis_alignment(this: &mut WidgetMut<'_, Self>, alignment: CrossAxisAlignment) {
        if this.widget.cross_axis_alignment != alignment {
            this.widget.cross_axis_alignment = alignment;
            this.ctx.request_layout();
        }
    }
}

// --- MARK: COLLECTIONWIDGET
impl CollectionWidget<()> for FlexWrap {
    fn len(&self) -> usize {
        self.children.len()
    }

    fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    fn get_mut<'t>(this: &'t mut WidgetMut<'_, Self>, idx: usize) -> WidgetMut<'t, dyn Widget> {
        let child = &mut this.widget.children[idx];
        this.ctx.get_mut(child)
    }

    fn add(
        this: &mut WidgetMut<'_, Self>,
        child: NewWidget<impl Widget + ?Sized>,
        _params: impl Into<()>,
    ) {
        this.widget.children.push(child.erased().to_pod());
        this.ctx.children_changed();
    }

    fn insert(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: NewWidget<impl Widget + ?Sized>,
        _params: impl Into<()>,
    ) {
        this.widget.children.insert(idx, child.erased().to_pod());
        this.ctx.children_changed();
    }

    fn set(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: NewWidget<impl Widget + ?Sized>,
        _params: impl Into<()>,
    ) {
        let new_pod = child.erased().to_pod();
        let old_pod = std::mem::replace(&mut this.widget.children[idx], new_pod);
        this.ctx.remove_child(old_pod);
        this.ctx.children_changed();
    }

    fn set_params(this: &mut WidgetMut<'_, Self>, idx: usize, _params: impl Into<()>) {
        // FlexWrap has no per-child params; this is a no-op modulo the
        // bounds check that matches sibling collection widgets.
        assert!(
            idx < this.widget.children.len(),
            "FlexWrap::set_params: idx out of bounds"
        );
    }

    fn swap(this: &mut WidgetMut<'_, Self>, a: usize, b: usize) {
        this.widget.children.swap(a, b);
        this.ctx.children_changed();
    }

    fn remove(this: &mut WidgetMut<'_, Self>, idx: usize) {
        let pod = this.widget.children.remove(idx);
        this.ctx.remove_child(pod);
    }

    fn clear(this: &mut WidgetMut<'_, Self>) {
        for pod in this.widget.children.drain(..) {
            this.ctx.remove_child(pod);
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for FlexWrap {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for child in &mut self.children {
            ctx.register_child(child);
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        if self.children.is_empty() {
            return Length::ZERO;
        }
        let auto_length: LenDef = len_req.into();

        // Collect each child's natural max-content width — both axes
        // need this. The width pre-pass is unconditional; masonry caches
        // measurement queries internally, so downstream `compute_length`
        // calls for the same child won't re-walk its tree. We unwrap
        // each `Length` to `f64` here so the partition / alignment
        // helpers (which interop with kurbo's `Size`) stay in f64.
        let widths: Vec<f64> = self
            .children
            .iter_mut()
            .map(|child| {
                ctx.compute_length(
                    child,
                    LenDef::MaxContent,
                    LayoutSize::default(),
                    Axis::Horizontal,
                    None,
                )
                .get()
            })
            .collect();

        #[expect(
            clippy::cast_precision_loss,
            reason = "child counts are small (<< 2^53)"
        )]
        let gap_count = widths.len().saturating_sub(1) as f64;

        let result = match axis {
            Axis::Horizontal => {
                let natural_total = widths.iter().sum::<f64>() + self.col_gap * gap_count;
                let min_total = widths.iter().fold(0.0_f64, |a, &b| a.max(b));
                match len_req {
                    LenReq::MinContent => min_total,
                    LenReq::MaxContent => natural_total,
                    // Prefer the natural single-row width, capped at
                    // what the parent offered, but never claim less
                    // than the widest single child (forcing overflow
                    // beneath that bound gains nothing).
                    LenReq::FitContent(space) => natural_total.min(space.get()).max(min_total),
                }
            }
            Axis::Vertical => match cross_length {
                None => {
                    // No width constraint — assume one row; report the
                    // tallest child given its natural width.
                    self.children
                        .iter_mut()
                        .zip(widths.iter())
                        .fold(0.0_f64, |a, (child, &w)| {
                            let w_len = Length::px(w);
                            let h = ctx.compute_length(
                                child,
                                auto_length,
                                LayoutSize::maybe(Axis::Horizontal, Some(w_len)),
                                Axis::Vertical,
                                Some(w_len),
                            );
                            a.max(h.get())
                        })
                }
                Some(available_width) => {
                    let rows = partition_rows(&widths, available_width.get(), self.col_gap);
                    let mut total: f64 = 0.0;
                    for (row_idx, row) in rows.iter().enumerate() {
                        let row_children = &mut self.children[row.start..row.end];
                        let row_widths = &widths[row.start..row.end];
                        let row_h = row_children.iter_mut().zip(row_widths.iter()).fold(
                            0.0_f64,
                            |a, (child, &w)| {
                                let w_len = Length::px(w);
                                let h = ctx.compute_length(
                                    child,
                                    auto_length,
                                    LayoutSize::maybe(Axis::Horizontal, Some(w_len)),
                                    Axis::Vertical,
                                    Some(w_len),
                                );
                                a.max(h.get())
                            },
                        );
                        total += row_h;
                        if row_idx + 1 < rows.len() {
                            total += self.row_gap;
                        }
                    }
                    total
                }
            },
        };
        Length::px(result)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.children.is_empty() {
            ctx.clear_baselines();
            return;
        }

        // Pass 1: query each child's natural (max-content) size. We
        // must *not* pass `SizeDef::fit(size)` here — masonry's `Flex`
        // treats `FitContent` as "fill the offered space up to your
        // min-content," which would make every child report the full
        // available width and collapse the wrap into a single column.
        // `SizeDef::MAX` (MaxContent on both axes) returns each child's
        // natural size, which is what the partition pass needs.
        let natural: Vec<Size> = self
            .children
            .iter_mut()
            .map(|child| ctx.compute_size(child, SizeDef::MAX, size.into()))
            .collect();
        let widths: Vec<f64> = natural.iter().map(|s| s.width).collect();
        let rows = partition_rows(&widths, size.width, self.col_gap);

        // Pass 2: place each child. We track `y` (top of current row)
        // as we walk rows, and within each row use
        // `main_axis_positions` for the x-offsets plus
        // `CrossAxisAlignment::offset` for the y-offset within the row.
        let mut y: f64 = 0.0;
        for (row_idx, row) in rows.iter().enumerate() {
            let row_widths = &widths[row.start..row.end];
            let row_natural = &natural[row.start..row.end];
            let row_height = row_natural.iter().fold(0.0_f64, |a, s| a.max(s.height));
            let positions = main_axis_positions(
                row_widths,
                size.width,
                self.col_gap,
                self.main_axis_alignment,
            );

            for (slot, child_idx) in (row.start..row.end).enumerate() {
                let child_natural = natural[child_idx];
                let x = positions[slot];

                let (final_size, cross_offset) = if self.cross_axis_alignment
                    == CrossAxisAlignment::Stretch
                {
                    // Re-query the child with its height pinned to the
                    // row's height; the child can refuse and report a
                    // smaller size, in which case `run_layout` commits
                    // what it reported (no enforcement here — masonry
                    // already documents Stretch as a best-effort hint).
                    let stretch_def =
                        SizeDef::MAX.with_height(LenDef::Fixed(Length::px(row_height)));
                    let stretched =
                        ctx.compute_size(&mut self.children[child_idx], stretch_def, size.into());
                    (stretched, 0.0)
                } else {
                    let slack = row_height - child_natural.height;
                    (child_natural, self.cross_axis_alignment.offset(slack))
                };

                let child = &mut self.children[child_idx];
                ctx.run_layout(child, final_size);
                ctx.place_child(child, Point::new(x, y + cross_offset));
            }

            y += row_height;
            if row_idx + 1 < rows.len() {
                y += self.row_gap;
            }
        }

        // Derive baselines from the first row's first child and the
        // last row's first child. Matches the multi-cell pattern from
        // masonry's Grid — close enough for a wrapping container where
        // every row's leading child is the most likely text anchor.
        if let (Some(first_row), Some(last_row)) = (rows.first(), rows.last()) {
            let first_origin = ctx.child_origin(&self.children[first_row.start]);
            let last_origin = ctx.child_origin(&self.children[last_row.start]);
            let (first_baseline, _) = ctx.child_aligned_baselines(&self.children[first_row.start]);
            let (_, last_baseline) = ctx.child_aligned_baselines(&self.children[last_row.start]);
            ctx.set_baselines(
                first_origin.y + first_baseline,
                last_origin.y + last_baseline,
            );
        } else {
            ctx.clear_baselines();
        }
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        let ids: Vec<_> = self.children.iter().map(WidgetPod::id).collect();
        ChildrenIds::from_slice(&ids)
    }
}
