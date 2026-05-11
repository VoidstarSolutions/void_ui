//! Masonry widget that packs children left-to-right and wraps to a new
//! row when the next child would overflow the available width.
//!
//! Layout primitive — no painting, no event handling. Wraps any
//! collection of widgets (chips, tags, swatches, buttons) without the
//! call site having to know how many will fit per row.
//!
//! ## Step 1 limitation
//!
//! The current implementation lays children out in a single row (no
//! wrap). The struct fields and the public API are already shaped to
//! accept gap and alignment configuration; the actual row-partitioning
//! algorithm and alignment math land in the follow-up commit. Callers
//! with children that all fit on one row see correct behavior now;
//! callers whose children overflow see the children spill off the right
//! edge until step 2 lands.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, CollectionWidget, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesRef, RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, SizeDef};
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
        cross_length: Option<f64>,
    ) -> f64 {
        if self.children.is_empty() {
            return 0.0;
        }
        let auto_length = len_req.into();
        match axis {
            Axis::Horizontal => {
                // Step 1: report the natural single-row width. Step 2
                // will refine to honor MinContent/MaxContent/FitContent
                // semantics around wrapping.
                let mut total: f64 = 0.0;
                for child in &mut self.children {
                    let w = ctx.compute_length(
                        child,
                        auto_length,
                        LayoutSize::default(),
                        Axis::Horizontal,
                        None,
                    );
                    total += w;
                }
                #[expect(clippy::cast_precision_loss, reason = "child count is small (<< 2^53)")]
                let gap_total = self.col_gap * (self.children.len() - 1) as f64;
                total + gap_total
            }
            Axis::Vertical => {
                // Step 1: assume one row; report the tallest child.
                let mut max_h: f64 = 0.0;
                let context = LayoutSize::maybe(Axis::Horizontal, cross_length);
                for child in &mut self.children {
                    let h = ctx.compute_length(
                        child,
                        auto_length,
                        context,
                        Axis::Vertical,
                        cross_length,
                    );
                    max_h = max_h.max(h);
                }
                max_h
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.children.is_empty() {
            return;
        }
        // Step 1: pack children flat from x=0, all at y=0. Wrap +
        // alignment math arrive in step 2.
        let mut x: f64 = 0.0;
        let auto_size = SizeDef::fit(size);
        for child in &mut self.children {
            let child_size = ctx.compute_size(child, auto_size, size.into());
            ctx.run_layout(child, child_size);
            ctx.place_child(child, Point::new(x, 0.0));
            x += child_size.width + self.col_gap;
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
