//! Masonry widget for `kbd` — a raised keycap painted behind a mono label.
//!
//! Presentation only: no pointer/keyboard interaction, no emitted actions.
//! The keycap is two stacked rounded rects painted with masonry's own
//! [`paint_background`] helper (as `meter` does): a darker outer rect that
//! shows through as a 1px border ring plus a `LIP_PX` bottom "lip", and a
//! lighter body rect inset on top of it. Reads [`Theme`] as a value (no
//! `Theme` is reachable through the property stack — see `button/widget.rs`).

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ArcStr, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesRef, RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod, paint_background,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::{Background, BorderWidth, CornerRadius};
use masonry::widgets::Label;

use crate::Theme;

/// Bottom "lip" thickness in px — the exposed dark edge below the body that
/// gives the keycap its raised, physical-key look.
const LIP_PX: f64 = 2.0;

/// A raised keycap chip wrapping a single monospace [`Label`] child.
pub struct KbdWidget {
    child: WidgetPod<Label>,
    theme: Theme,
    spoken_name: ArcStr,
}

impl KbdWidget {
    pub(super) fn new(child: NewWidget<Label>, theme: &Theme, spoken_name: ArcStr) -> Self {
        Self {
            child: child.to_pod(),
            theme: *theme,
            spoken_name,
        }
    }

    pub(super) fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, Label> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    pub(super) fn set_text(this: &mut WidgetMut<'_, Self>, text: ArcStr) {
        {
            let mut label = Self::child_mut(this);
            Label::set_text(&mut label, text);
        }
        this.ctx.request_layout();
    }

    pub(super) fn set_spoken_name(this: &mut WidgetMut<'_, Self>, name: ArcStr) {
        if this.widget.spoken_name != name {
            this.widget.spoken_name = name;
            this.ctx.request_render();
        }
    }

    pub(super) fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }

    /// The raised-key surface (body) color.
    fn body_color(&self) -> Color {
        self.theme.palette.surface_hi
    }

    /// The border ring + bottom-lip color.
    fn edge_color(&self) -> Color {
        self.theme.palette.border_strong
    }

    fn corner_px(&self) -> f64 {
        f64::from(self.theme.radius.small)
    }
}

impl Widget for KbdWidget {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
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
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let (main_pad, cross_pad) = match axis {
            Axis::Horizontal => (2.0 * pad_h, 2.0 * pad_v + LIP_PX),
            Axis::Vertical => (2.0 * pad_v + LIP_PX, 2.0 * pad_h),
        };
        let inner_cross = cross_length.map(|c| Length::px((c.get() - cross_pad).max(0.0)));
        let context_size = LayoutSize::maybe(axis.cross(), inner_cross);
        let child_length = ctx.compute_length(
            &mut self.child,
            len_req.into(),
            context_size,
            axis,
            inner_cross,
        );
        Length::px(child_length.get() + main_pad)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let pad_v = f64::from(self.theme.density.button_pad_v);
        let pad_h = f64::from(self.theme.density.button_pad_h);
        let child_w = (size.width - 2.0 * pad_h).max(0.0);
        let child_h = (size.height - 2.0 * pad_v - LIP_PX).max(0.0);
        ctx.run_layout(&mut self.child, Size::new(child_w, child_h));
        // Child sits in the padded box above the lip.
        ctx.place_child(&mut self.child, Point::new(pad_h, pad_v));
    }

    fn pre_paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let bbox = ctx.border_box();
        let radius = CornerRadius::all(Length::px(self.corner_px()));
        // 1. Outer darker rect — becomes the 1px border ring + the LIP_PX
        //    bottom lip once the body is painted on top.
        paint_background(
            painter,
            bbox,
            &Background::Color(self.edge_color()),
            &BorderWidth::default(),
            &radius,
        );
        // 2. Body — inset 1px top/left/right and (1 + LIP_PX) bottom, so the
        //    dark outer shows as a hairline ring plus a thicker bottom edge.
        // Clamped so a host-forced undersized box (smaller than ~4px) can't
        // invert the rect (max coords are never less than the min coords).
        let x1 = (bbox.x1 - 1.0).max(bbox.x0 + 1.0);
        let y1 = (bbox.y1 - 1.0 - LIP_PX).max(bbox.y0 + 1.0);
        let body = Rect::new(bbox.x0 + 1.0, bbox.y0 + 1.0, x1, y1);
        let inner_radius = CornerRadius::all(Length::px((self.corner_px() - 1.0).max(0.0)));
        paint_background(
            painter,
            body,
            &Background::Color(self.body_color()),
            &BorderWidth::default(),
            &inner_radius,
        );
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::Label
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_value(self.spoken_name.as_ref());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

#[cfg(test)]
mod tests {
    use masonry::accesskit::Role;
    use masonry::core::{ArcStr, NewWidget, Widget};
    use masonry::testing::TestHarness;
    use masonry::theme::default_property_set;
    use masonry::widgets::Label;

    use super::KbdWidget;
    use crate::Theme;

    fn harness(text: &str, spoken: &str) -> TestHarness<KbdWidget> {
        let theme = Theme::default();
        let label = Label::new(ArcStr::from(text)).prepare();
        let widget = KbdWidget::new(label, &theme, ArcStr::from(spoken));
        TestHarness::create_with_size(default_property_set(), NewWidget::new(widget), (200, 40))
    }

    #[test]
    fn mounts_and_paints_without_panicking() {
        for (text, spoken) in [("K", "K"), ("⇧\u{2009}⌘\u{2009}K", "Shift Command K")] {
            let mut h = harness(text, spoken);
            h.redraw();
        }
    }

    #[test]
    fn reports_label_role_and_spoken_name() {
        let mut h = harness("⌘\u{2009}K", "Command K");
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.value().as_deref(), Some("Command K"));
    }

    #[test]
    fn set_spoken_name_updates_accessibility() {
        let mut h = harness("⌘\u{2009}K", "Command K");
        h.redraw();
        h.edit_root_widget(|mut wm| {
            KbdWidget::set_spoken_name(&mut wm, ArcStr::from("Command J"));
        });
        h.redraw();
        let node = h.access_node(h.root_id()).expect("node exists");
        assert_eq!(node.value().as_deref(), Some("Command J"));
    }

    /// `set_theme` must request layout (not just repaint) when density
    /// changes, since `button_pad_v`/`button_pad_h` feed `measure`/`layout`.
    /// The root widget itself always fills the harness window (tight
    /// constraints), so a relayout can't be observed via the root's own
    /// border box; instead this checks the child label's placed position,
    /// which `layout` derives directly from `pad_h`/`pad_v`
    /// (`ctx.place_child(&mut self.child, Point::new(pad_h, pad_v))`) — a
    /// post-mount theme swap to a wider density must shift it.
    #[test]
    fn set_theme_with_new_density_relayouts() {
        let mut h = harness("⌘\u{2009}K", "Command K");
        h.redraw();
        let child_origin = |h: &TestHarness<KbdWidget>| {
            let root = h.root_widget();
            let child = root.children().into_iter().next().expect("has child");
            child.ctx().to_window(masonry::kurbo::Point::ORIGIN)
        };
        let before = child_origin(&h);

        let mut grown = Theme::default();
        grown.density.button_pad_h += 20.0;
        grown.density.button_pad_v += 20.0;
        h.edit_root_widget(|mut wm| {
            KbdWidget::set_theme(&mut wm, &grown);
        });
        h.redraw();
        let after = child_origin(&h);

        assert_ne!(
            before, after,
            "set_theme should trigger a relayout when density padding changes"
        );
    }
}
