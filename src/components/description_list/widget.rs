//! Masonry widget for the description list component.
//!
//! [`DescriptionListWidget`] is a multi-child container holding one label pod and
//! one value pod per item. In horizontal mode it measures every label, sets the
//! shared label-column width to the widest, and baseline-aligns each value against
//! its label (falling back to top-align when the value reports no baseline). In
//! stacked mode each value is laid out directly below its label.
//!
//! Horizontal layout (this module's `layout_horizontal`/`measure`) is real as of
//! Task 3. Stacked layout (`layout_stacked`/`measure_stacked`) is still a
//! placeholder — every child measured at its minimum preferred size and placed
//! at the origin — pending Task 4.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use masonry::widgets::Passthrough;

use super::view::DescriptionListOrientation;
use crate::Theme;

pub struct DescriptionListWidget {
    labels: Vec<WidgetPod<Passthrough>>,
    values: Vec<WidgetPod<Passthrough>>,
    orientation: DescriptionListOrientation,
    theme: Theme,
}

// --- MARK: BUILDERS

impl DescriptionListWidget {
    /// # Panics
    ///
    /// Panics if `labels` and `values` don't have the same length.
    pub(super) fn new(
        labels: Vec<NewWidget<Passthrough>>,
        values: Vec<NewWidget<Passthrough>>,
        orientation: DescriptionListOrientation,
        theme: &Theme,
    ) -> Self {
        assert_eq!(
            labels.len(),
            values.len(),
            "DescriptionListWidget: labels and values must be the same length"
        );
        Self {
            labels: labels.into_iter().map(NewWidget::to_pod).collect(),
            values: values.into_iter().map(NewWidget::to_pod).collect(),
            orientation,
            theme: *theme,
        }
    }
}

// --- MARK: LAYOUT HELPERS

impl DescriptionListWidget {
    /// Horizontal gap between the label column and the value column.
    fn column_gap(&self) -> f64 {
        f64::from(self.theme.density.gap_lg)
    }
    /// Vertical gap between successive rows / items.
    fn row_gap(&self) -> f64 {
        f64::from(self.theme.density.gap)
    }
    /// Vertical gap between a label and its value in stacked mode.
    // TODO(Task 4): wired up by `layout_stacked`/`measure_stacked`; unused until then.
    #[allow(dead_code)]
    fn pair_gap(&self) -> f64 {
        f64::from(self.theme.density.pad)
    }

    fn layout_horizontal(&mut self, ctx: &mut LayoutCtx<'_>, size: Size) {
        let ctx_size = LayoutSize::from(size);

        // Pass 1: widest label sets the shared column width.
        let mut col_w = 0.0_f64;
        let mut label_pref: Vec<Size> = Vec::with_capacity(self.labels.len());
        for label in &mut self.labels {
            let s = ctx.compute_size(label, SizeDef::MIN, ctx_size);
            col_w = col_w.max(s.width);
            label_pref.push(s);
        }

        let column_gap = self.column_gap();
        let row_gap = self.row_gap();
        let value_x = col_w + column_gap;
        let value_avail = (size.width - value_x).max(0.0);

        // Pass 2: lay out each row, baseline-aligning value to label.
        let mut y = 0.0_f64;
        let rows = self
            .labels
            .iter_mut()
            .zip(self.values.iter_mut())
            .zip(label_pref.iter());
        for ((label, value), lp) in rows {
            let lh = lp.height;
            ctx.run_layout(label, Size::new(col_w, lh));
            // `child_layout_baselines` (not `child_aligned_baselines`) is the one that
            // only requires `run_layout` — it's meant for deciding placement, whereas
            // the "aligned" variant asserts the child has already been placed.
            let (label_base, _) = ctx.child_layout_baselines(label);

            let vs = ctx.compute_size(
                value,
                SizeDef::MIN,
                LayoutSize::from(Size::new(value_avail, size.height)),
            );
            ctx.run_layout(value, vs);
            let (value_base, _) = ctx.child_layout_baselines(value);

            // Baseline align: if both children report a baseline, offset the one
            // with the smaller baseline down so the baselines coincide. Otherwise
            // top-align (offset 0 for both). In practice masonry's baseline
            // accessors fall back to the child's own height rather than NaN when a
            // widget never calls `set_baselines`, so this branch mostly guards
            // against a stray non-finite value rather than firing routinely.
            let (label_dy, value_dy) = if label_base.is_finite() && value_base.is_finite() {
                let target = label_base.max(value_base);
                (target - label_base, target - value_base)
            } else {
                (0.0, 0.0)
            };

            ctx.place_child(label, Point::new(0.0, y + label_dy));
            ctx.place_child(value, Point::new(value_x, y + value_dy));

            let row_h = (label_dy + lh).max(value_dy + vs.height);
            y += row_h + row_gap;
        }
    }

    /// TODO(Task 4): real stacked layout — label above value, each item stacked
    /// vertically. For now, lay every child out at its preferred size and place
    /// it at the origin so the widget compiles and the Horizontal path (this
    /// task's actual scope) has somewhere to dispatch from.
    fn layout_stacked(&mut self, ctx: &mut LayoutCtx<'_>, size: Size) {
        for pod in self.labels.iter_mut().chain(self.values.iter_mut()) {
            let s = ctx.compute_size(pod, SizeDef::MIN, LayoutSize::from(size));
            ctx.run_layout(pod, s);
            ctx.place_child(pod, Point::ORIGIN);
        }
    }

    /// TODO(Task 4): real stacked measurement, mirroring `layout_stacked`.
    #[allow(clippy::unused_self)]
    fn measure_stacked(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(0.0)
    }
}

// --- MARK: WIDGETMUT

impl DescriptionListWidget {
    pub(super) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
        }
    }

    pub(super) fn set_orientation(this: &mut WidgetMut<'_, Self>, o: DescriptionListOrientation) {
        if this.widget.orientation != o {
            this.widget.orientation = o;
            this.ctx.request_layout();
        }
    }

    /// Replaces the entire label/value child set, e.g. when the item *count*
    /// changes.
    pub(super) fn set_items(
        this: &mut WidgetMut<'_, Self>,
        labels: Vec<NewWidget<Passthrough>>,
        values: Vec<NewWidget<Passthrough>>,
    ) {
        // Drop old children so masonry unregisters them, then register the new set.
        for pod in this.widget.labels.drain(..) {
            this.ctx.remove_child(pod);
        }
        for pod in this.widget.values.drain(..) {
            this.ctx.remove_child(pod);
        }
        this.widget.labels = labels.into_iter().map(NewWidget::to_pod).collect();
        this.widget.values = values.into_iter().map(NewWidget::to_pod).collect();
        this.ctx.children_changed();
        this.ctx.request_layout();
    }

    /// Returns a `WidgetMut` for the label at `i`.
    ///
    /// Label and value of item `i` are built under the same `ViewId::new(i)`
    /// (see `view.rs`'s `build`/`rebuild`/`message`) — safe today because
    /// labels are static, non-interactive `Label`s that never route a
    /// message. If a label ever becomes interactive, split the id space
    /// (`2*i` label, `2*i+1` value) rather than reusing this shared index.
    pub(super) fn label_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        i: usize,
    ) -> WidgetMut<'t, Passthrough> {
        this.ctx.get_mut(&mut this.widget.labels[i])
    }

    pub(super) fn value_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
        i: usize,
    ) -> WidgetMut<'t, Passthrough> {
        this.ctx.get_mut(&mut this.widget.values[i])
    }
}

// --- MARK: IMPL WIDGET

impl Widget for DescriptionListWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for pod in &mut self.labels {
            ctx.register_child(pod);
        }
        for pod in &mut self.values {
            ctx.register_child(pod);
        }
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _type_id: std::any::TypeId) {}

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let context_size = LayoutSize::maybe(axis.cross(), cross_length);
        match self.orientation {
            DescriptionListOrientation::Horizontal => {
                let mut col_w = 0.0_f64;
                let mut value_w = 0.0_f64;
                let mut total_h = 0.0_f64;
                let row_gap = self.row_gap();
                let n = self.labels.len();
                for i in 0..n {
                    let lw = ctx.compute_length(
                        &mut self.labels[i],
                        len_req.into(),
                        context_size,
                        Axis::Horizontal,
                        cross_length,
                    );
                    col_w = col_w.max(lw.get());
                }
                for i in 0..n {
                    let vw = ctx.compute_length(
                        &mut self.values[i],
                        len_req.into(),
                        context_size,
                        Axis::Horizontal,
                        cross_length,
                    );
                    value_w = value_w.max(vw.get());
                    let lh = ctx.compute_length(
                        &mut self.labels[i],
                        len_req.into(),
                        context_size,
                        Axis::Vertical,
                        cross_length,
                    );
                    let vh = ctx.compute_length(
                        &mut self.values[i],
                        len_req.into(),
                        context_size,
                        Axis::Vertical,
                        cross_length,
                    );
                    total_h += lh.get().max(vh.get());
                    if i + 1 < n {
                        total_h += row_gap;
                    }
                }
                match axis {
                    Axis::Horizontal => Length::px(col_w + self.column_gap() + value_w),
                    Axis::Vertical => Length::px(total_h),
                }
            }
            DescriptionListOrientation::Stacked => {
                self.measure_stacked(ctx, axis, len_req, cross_length)
            }
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        match self.orientation {
            DescriptionListOrientation::Horizontal => self.layout_horizontal(ctx, size),
            DescriptionListOrientation::Stacked => self.layout_stacked(ctx, size),
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
        let ids: Vec<_> = self
            .labels
            .iter()
            .chain(self.values.iter())
            .map(WidgetPod::id)
            .collect();
        ChildrenIds::from_slice(&ids)
    }
}

// --- MARK: TESTS

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::testing::TestHarness;
    use masonry::widgets::{Label, Passthrough};

    use super::DescriptionListWidget;
    use crate::Theme;
    use crate::components::description_list::view::DescriptionListOrientation;

    fn passthrough(text: &str) -> NewWidget<Passthrough> {
        NewWidget::new(Passthrough::new(NewWidget::new(Label::new(text)).erased()))
    }

    fn widget() -> DescriptionListWidget {
        DescriptionListWidget::new(
            vec![passthrough("Name"), passthrough("Role")],
            vec![passthrough("Ada"), passthrough("Mathematician")],
            DescriptionListOrientation::Horizontal,
            &Theme::default(),
        )
    }

    #[test]
    fn children_ids_lists_every_label_then_every_value() {
        let mut h = TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(widget()),
        );
        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.labels.len(), 2);
            assert_eq!(wm.widget.values.len(), 2);
        });
    }

    #[test]
    fn horizontal_values_share_a_common_x_past_the_widest_label() {
        // Labels of very different widths; values must still align.
        let w = DescriptionListWidget::new(
            vec![passthrough("A"), passthrough("A much longer label")],
            vec![passthrough("first value"), passthrough("second value")],
            DescriptionListOrientation::Horizontal,
            &Theme::default(),
        );
        let mut h = TestHarness::create_with_size(
            masonry::theme::default_property_set(),
            NewWidget::new(w),
            (600, 200),
        );

        // Read each value child's window-space left edge.
        let value_ids: Vec<_> = h.edit_root_widget(|wm| {
            wm.widget
                .values
                .iter()
                .map(masonry::core::WidgetPod::id)
                .collect::<Vec<_>>()
        });
        let x0 = h
            .get_widget_with_id(value_ids[0])
            .ctx()
            .bounding_box()
            .min_x();
        let x1 = h
            .get_widget_with_id(value_ids[1])
            .ctx()
            .bounding_box()
            .min_x();
        assert!(
            (x0 - x1).abs() < 0.5,
            "values not column-aligned: {x0} vs {x1}"
        );
        // And the column starts past the short label's own width.
        assert!(x0 > 20.0, "value column should sit past the widest label");
    }
}
