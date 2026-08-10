//! Masonry widget for the description list component.
//!
//! [`DescriptionListWidget`] is a multi-child container holding one label pod and
//! one value pod per item. In horizontal mode it measures every label, sets the
//! shared label-column width to the widest, and baseline-aligns each value against
//! its label (falling back to top-align when the value reports no baseline). In
//! stacked mode each value is laid out directly below its label.
//!
//! Layout is currently a stub — every child is measured at its minimum
//! preferred size and placed at the origin. Real column/baseline layout lands
//! in Tasks 3 (horizontal) and 4 (stacked).

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
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        // Replaced with real measurement in Task 3.
        Length::px(0.0)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        // Stub: lay every child out at its preferred size, stacked at the origin.
        // Replaced with real column/baseline layout in Tasks 3 and 4.
        for pod in self.labels.iter_mut().chain(self.values.iter_mut()) {
            let s = ctx.compute_size(pod, SizeDef::MIN, LayoutSize::from(size));
            ctx.run_layout(pod, s);
            ctx.place_child(pod, Point::ORIGIN);
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
}
