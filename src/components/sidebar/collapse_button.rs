//! Sidebar-integrated collapse toggle button.
//!
//! Looks and feels like a [`super::widget::ThemedSidebarItem`] row but shows a
//! `‹` chevron icon instead of a text label. Drop one at the top of the items
//! list inside a [`super::panel_view::SidebarPanelView`] to give users a
//! one-click way to close the sidebar.
//!
//! ```ignore
//! sidebar_collapse_button(|s: &mut State| s.sidebar_collapsed = true)
//!     .render(&theme)
//! ```

use std::marker::PhantomData;
use std::sync::LazyLock;

use masonry::accesskit;
use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget, WidgetMut,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, BezPath, Point, RoundedRect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::widgets::ButtonPress;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::Theme;

/// Horizontal gap between the notional accent area and the icon (mirrors
/// `PAD_H` in the sidebar item widget).
const PAD_H: f64 = 8.0;
/// Vertical padding above and below the icon.
const PAD_V: f64 = 6.0;
/// Width of the left accent slot — kept to align visually with sidebar items.
const ACCENT_WIDTH: f64 = 3.0;
/// Focus-ring stroke width.
const FOCUS_RING_WIDTH: f64 = 1.5;
/// Inset of the focus ring from the item edge.
const FOCUS_RING_INSET: f64 = 2.0;

/// `‹` chevron in unit-square (0..1) coords, stroked at paint time.
static CHEVRON_LEFT: LazyLock<BezPath> = LazyLock::new(|| {
    let mut p = BezPath::new();
    p.move_to((0.65, 0.2));
    p.line_to((0.35, 0.5));
    p.line_to((0.65, 0.8));
    p
});

// --- MARK: WIDGET

/// Themed sidebar collapse toggle button (icon-only, no child widget).
pub struct ThemedSidebarCollapseButton {
    theme: Theme,
}

impl ThemedSidebarCollapseButton {
    #[must_use]
    pub fn new(theme: &Theme) -> Self {
        Self { theme: *theme }
    }
}

// --- MARK: WIDGETMUT
impl ThemedSidebarCollapseButton {
    pub fn set_theme(this: &mut WidgetMut<'_, Self>, theme: &Theme) {
        if this.widget.theme != *theme {
            this.widget.theme = *theme;
            this.ctx.request_layout();
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: PAINT STATE
impl ThemedSidebarCollapseButton {
    fn resolve_bg(&self, hovered: bool, pressed: bool) -> Color {
        let p = &self.theme.palette;
        if pressed && hovered {
            p.surface_hi
        } else if hovered {
            p.surface_2
        } else {
            Color::TRANSPARENT
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for ThemedSidebarCollapseButton {
    type Action = ButtonPress;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.request_focus();
                ctx.capture_pointer();
                ctx.request_paint_only();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: button @ Some(PointerButton::Primary),
                ..
            }) => {
                if ctx.is_active() && ctx.is_hovered() {
                    ctx.submit_action::<Self::Action>(ButtonPress { button: *button });
                }
                ctx.request_paint_only();
            }
            _ => (),
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == accesskit::Action::Click {
            ctx.submit_action::<Self::Action>(ButtonPress { button: None });
        }
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &Update,
    ) {
        match event {
            Update::HoveredChanged(_) | Update::FocusChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _property_type: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        let icon_size = f64::from(self.theme.density.ui_font_size) * 1.2;
        match axis {
            Axis::Horizontal => Length::px(ACCENT_WIDTH + 2.0 * PAD_H + icon_size),
            Axis::Vertical => Length::px(icon_size + 2.0 * PAD_V),
        }
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &PropertiesRef<'_>,
        _size: Size,
    ) {
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let size = ctx.border_box_size();
        let hovered = ctx.is_hovered();
        let pressed = ctx.is_active() && hovered;
        let focused = ctx.is_focus_target();
        let p = &self.theme.palette;

        let bg = self.resolve_bg(hovered, pressed);
        if bg.components[3] > 0.0 {
            painter
                .fill(RoundedRect::from_origin_size(Point::ORIGIN, size, 0.0), bg)
                .draw();
        }

        if focused {
            let inset = FOCUS_RING_INSET;
            let focus_rect = RoundedRect::from_origin_size(
                Point::new(inset, inset),
                Size::new(
                    (size.width - 2.0 * inset).max(0.0),
                    (size.height - 2.0 * inset).max(0.0),
                ),
                0.0,
            );
            painter
                .stroke(focus_rect, &Stroke::new(FOCUS_RING_WIDTH), p.teal)
                .draw();
        }

        let icon_color = p.text_muted;
        let icon_size = f64::from(self.theme.density.ui_font_size) * 1.2;
        let icon_x = (size.width - icon_size) * 0.5;
        let icon_y = (size.height - icon_size) * 0.5;
        let transform = Affine::translate((icon_x, icon_y)) * Affine::scale(icon_size);
        painter
            .stroke(transform * &*CHEVRON_LEFT, &Stroke::new(1.5), icon_color)
            .draw();
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label("Collapse sidebar");
        node.add_action(accesskit::Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[])
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
        false
    }
}

// --- MARK: VIEW BUILDER

/// Builder for a sidebar collapse toggle button.
///
/// Created with [`sidebar_collapse_button`]. Returns a xilem `WidgetView` via
/// [`Self::render`].
#[must_use = "SidebarCollapseButton does nothing until rendered with .render(&theme)"]
pub struct SidebarCollapseButton<F> {
    callback: F,
}

/// Create a sidebar-integrated collapse button.
///
/// The callback is invoked on primary-pointer release and on Space / Enter
/// while focused.
pub fn sidebar_collapse_button<F>(callback: F) -> SidebarCollapseButton<F> {
    SidebarCollapseButton { callback }
}

impl<F> SidebarCollapseButton<F> {
    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> SidebarCollapseButtonView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        SidebarCollapseButtonView {
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

// --- MARK: VIEW

/// The materialized [`View`] backing a [`SidebarCollapseButton`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SidebarCollapseButtonView<F, State, Action> {
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for SidebarCollapseButtonView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for SidebarCollapseButtonView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedSidebarCollapseButton>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = ThemedSidebarCollapseButton::new(&self.theme);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            ThemedSidebarCollapseButton::set_theme(&mut element, &self.theme);
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<ButtonPress>() {
            Some(_press) => MessageResult::Action((self.callback)(app_state)),
            None => MessageResult::Stale,
        }
    }
}
