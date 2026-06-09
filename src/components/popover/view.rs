//! Xilem view layer for the popover component.
//!
//! `Popover<State, Action, TriggerV, ContentV>` is the builder; `.render(&theme)`
//! produces a `PopoverView`.
//!
//! ```ignore
//! use void_ui::components::popover;
//! popover(
//!     button("Show info").render(&theme),
//!     label("Some helpful information here.").render(&theme),
//! )
//! .anchor(PopoverAnchor::BottomStart)
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::PopoverAnchor;
use super::widget::{PopoverHost, PopoverSurface};
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;

/// Builder for a popover.
///
/// Create with [`popover`]; configure with builder methods; materialize as a
/// xilem view via [`Self::render`].
#[must_use = "Popover does nothing until rendered with .render(&theme)"]
pub struct Popover<State, Action, TriggerV, ContentV> {
    trigger: TriggerV,
    content: ContentV,
    anchor: PopoverAnchor,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Construct a popover with the given trigger and content views.
///
/// Clicking the trigger toggles the floating content panel.  Clicking outside
/// or pressing Escape closes it.
pub fn popover<State, Action, TriggerV, ContentV>(
    trigger: TriggerV,
    content: ContentV,
) -> Popover<State, Action, TriggerV, ContentV>
where
    State: 'static,
    Action: 'static,
{
    Popover {
        trigger,
        content,
        anchor: PopoverAnchor::BottomStart,
        phantom: PhantomData,
    }
}

impl<State, Action, TriggerV, ContentV> Popover<State, Action, TriggerV, ContentV> {
    /// Set where the content panel appears relative to the trigger.
    pub fn anchor(mut self, anchor: PopoverAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> PopoverView<TriggerV, ContentV, State, Action> {
        PopoverView {
            trigger: self.trigger,
            content: self.content,
            anchor: self.anchor,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`Popover`].
///
/// Not constructed directly; use [`Popover::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct PopoverView<TriggerV, ContentV, State, Action> {
    trigger: TriggerV,
    content: ContentV,
    anchor: PopoverAnchor,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<TriggerV, ContentV, State, Action> ViewMarker
    for PopoverView<TriggerV, ContentV, State, Action>
{
}

/// View state for `PopoverView`: the trigger's and content's child view
/// states, both permanently mounted inside `PopoverHost`'s `overlay_host`.
pub struct PopoverViewState<TriggerVS, ContentVS> {
    trigger_vs: TriggerVS,
    content_vs: ContentVS,
}

impl<TriggerV, ContentV, State, Action> View<State, Action, ViewCtx>
    for PopoverView<TriggerV, ContentV, State, Action>
where
    State: 'static,
    Action: 'static,
    TriggerV: WidgetView<State, Action>,
    ContentV: WidgetView<State, Action>,
{
    type Element = Pod<PopoverHost>;
    type ViewState = PopoverViewState<TriggerV::ViewState, ContentV::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (trigger_pod, trigger_vs) = self.trigger.build(ctx, app_state);
        let (content_pod, content_vs) = self.content.build(ctx, app_state);
        let widget = PopoverHost::new(
            trigger_pod.new_widget.erased(),
            content_pod.new_widget.erased(),
            self.anchor,
            &self.theme,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (
            element,
            PopoverViewState {
                trigger_vs,
                content_vs,
            },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        let mut overlay_host = PopoverHost::overlay_host_mut(&mut element);
        let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
        self.trigger.rebuild(
            &prev.trigger,
            &mut view_state.trigger_vs,
            ctx,
            primary.downcast(),
            app_state,
        );
        drop(primary);

        let mut overlay = AnchoredOverlay::overlay_mut(&mut overlay_host);
        let mut surface = overlay.downcast::<PopoverSurface>();
        let mut content = PopoverSurface::content_mut(&mut surface);
        self.content.rebuild(
            &prev.content,
            &mut view_state.content_vs,
            ctx,
            content.downcast(),
            app_state,
        );
        drop(content);
        drop(surface);
        drop(overlay);
        drop(overlay_host);

        if self.theme != prev.theme {
            PopoverHost::set_theme(&mut element, &self.theme);
        }
        if self.anchor != prev.anchor {
            PopoverHost::set_anchor(&mut element, self.anchor);
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut overlay_host = PopoverHost::overlay_host_mut(&mut element);
        let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
        self.trigger
            .teardown(&mut view_state.trigger_vs, ctx, primary.downcast());
        drop(primary);
        let mut overlay = AnchoredOverlay::overlay_mut(&mut overlay_host);
        let mut surface = overlay.downcast::<PopoverSurface>();
        let mut content = PopoverSurface::content_mut(&mut surface);
        self.content
            .teardown(&mut view_state.content_vs, ctx, content.downcast());
        drop(content);
        drop(surface);
        drop(overlay);
        drop(overlay_host);
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        let mut overlay_host = PopoverHost::overlay_host_mut(&mut element);
        let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
        let result = self.trigger.message(
            &mut view_state.trigger_vs,
            message,
            primary.downcast(),
            app_state,
        );
        drop(primary);
        match result {
            MessageResult::Nop => {
                let mut overlay = AnchoredOverlay::overlay_mut(&mut overlay_host);
                let mut surface = overlay.downcast::<PopoverSurface>();
                let mut content = PopoverSurface::content_mut(&mut surface);
                self.content.message(
                    &mut view_state.content_vs,
                    message,
                    content.downcast(),
                    app_state,
                )
            }
            other => other,
        }
    }
}
