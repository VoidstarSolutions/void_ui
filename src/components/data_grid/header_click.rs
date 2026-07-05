//! Single-child wrapper widget that detects a primary-button click and
//! emits a [`HeaderClicked`] action — the header-row counterpart to
//! `RowClickable` (`crate::collection::row_click`).
//!
//! Kept deliberately separate from `RowClickable`. Header clicks are
//! modifier-agnostic (a plain click cycles the column's sort) and must
//! not participate in row selection or take keyboard focus the way a
//! row does. Splitting the two keeps each one's intent obvious and
//! leaves room for header-specific behavior later (resize handles,
//! column menus) without entangling it with row selection.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;

use crate::collection::single_child;
use crate::components::click::{self, ClickPhase};

/// Action emitted by [`HeaderClickable`] on primary-button release
/// inside its bounds. Carries whether the Shift modifier was held, which
/// the grid maps to multi-column (tiebreak) sort: a plain click replaces
/// the sort; Shift+click adds/cycles the column as a tiebreaker.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeaderClicked {
    /// Shift was held — request a multi-sort (additive) cycle rather
    /// than a replacing single-sort cycle.
    pub multi: bool,
}

/// Single-child wrapper that emits [`HeaderClicked`] on primary-button
/// release inside its bounds, and paints a hover-tint background while
/// the pointer is over it — the affordance that signals "this column
/// is sortable" (only sortable headers are wrapped in this widget).
pub struct HeaderClickable {
    child: WidgetPod<dyn Widget>,
    /// Background painted while hovered. Theme-driven (typically
    /// `border_strong`, a clear step above the header's `surface_2`
    /// for an obvious-but-neutral hover cue).
    hover_bg: Color,
}

// --- MARK: BUILDERS
impl HeaderClickable {
    #[must_use]
    pub fn new(child: NewWidget<impl Widget + ?Sized>, hover_bg: Color) -> Self {
        Self {
            child: child.erased().to_pod(),
            hover_bg,
        }
    }
}

// --- MARK: WIDGETMUT
impl HeaderClickable {
    /// Returns a mutable reference to the wrapped child.
    pub fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.child)
    }

    /// Updates the hover-tint color (e.g. on a theme swap). Requests a
    /// repaint only when the value changes.
    pub fn set_hover_bg(this: &mut WidgetMut<'_, Self>, hover_bg: Color) {
        if this.widget.hover_bg != hover_bg {
            this.widget.hover_bg = hover_bg;
            this.ctx.request_paint_only();
        }
    }
}

// --- MARK: IMPL WIDGET
impl Widget for HeaderClickable {
    type Action = HeaderClicked;

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Shared Down→capture / Up-iff-active-and-hovered recognizer.
        // No focus request on Down — headers don't own the keyboard
        // (the grid's copy shortcut does).
        if let Some(ClickPhase::Up {
            state,
            completed: true,
        }) = click::primary_click(ctx, event)
        {
            ctx.submit_action::<Self::Action>(HeaderClicked {
                multi: state.modifiers.shift(),
            });
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }

    fn on_access_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &AccessEvent,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        // Repaint when the pointer enters/leaves so the hover tint
        // tracks the cursor — same mechanism the button widget uses.
        if let Update::HoveredChanged(_) = event {
            ctx.request_paint_only();
        }
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        single_child::register_children(ctx, &mut self.child);
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
        single_child::measure(ctx, &mut self.child, axis, len_req, cross_length)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        single_child::layout(ctx, &mut self.child, size);
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        // Fill the cell with the hover tint while hovered. Painted
        // before the child label (parent paints first), so the title
        // and sort arrow render on top.
        if ctx.is_hovered() && self.hover_bg.components[3] > 0.0 {
            let rect = Rect::from_origin_size(Point::ORIGIN, ctx.border_box_size());
            painter.fill(rect, self.hover_bg).draw();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ColumnHeader
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        single_child::children_ids(&self.child)
    }

    fn accepts_focus(&self) -> bool {
        false
    }

    fn accepts_text_input(&self) -> bool {
        false
    }

    fn propagates_pointer_interaction(&self) -> bool {
        false
    }
}

// --- MARK: XILEM VIEW WRAPPER

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

/// Wraps a child view in [`HeaderClickable`] and runs `on_click`
/// against the host's app state on each primary-button release.
///
/// Use it to cycle the grid's
/// [`SortState`](super::sort::SortState) for the clicked column.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ClickableHeader<V, State, F> {
    child: V,
    hover_bg: Color,
    on_click: F,
    phantom: PhantomData<fn() -> State>,
}

/// Constructor for [`ClickableHeader`]. `hover_bg` is the tint painted
/// while the header is hovered (theme-driven).
pub fn clickable_header<V, State, F>(
    child: V,
    hover_bg: Color,
    on_click: F,
) -> ClickableHeader<V, State, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State, bool) + Send + Sync + 'static,
    State: 'static,
{
    ClickableHeader {
        child,
        hover_bg,
        on_click,
        phantom: PhantomData,
    }
}

impl<V, State, F> ViewMarker for ClickableHeader<V, State, F> {}

impl<V, State, F> View<State, (), ViewCtx> for ClickableHeader<V, State, F>
where
    V: WidgetView<State, ()>,
    F: Fn(&mut State, bool) + Send + Sync + 'static,
    State: 'static,
{
    type Element = Pod<HeaderClickable>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = HeaderClickable::new(child_pod.new_widget, self.hover_bg);
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.hover_bg != prev.hover_bg {
            HeaderClickable::set_hover_bg(&mut element, self.hover_bg);
        }
        let mut child = HeaderClickable::child_mut(&mut element);
        self.child
            .rebuild(&prev.child, view_state, ctx, child.downcast(), app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        {
            let mut child = HeaderClickable::child_mut(&mut element);
            self.child.teardown(view_state, ctx, child.downcast());
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        if let Some(action) = message.take_message::<HeaderClicked>() {
            (self.on_click)(app_state, action.multi);
            MessageResult::Action(())
        } else {
            let mut child = HeaderClickable::child_mut(&mut element);
            self.child
                .message(view_state, message, child.downcast(), app_state)
        }
    }
}
