//! Xilem view layer for the popover component.
//!
//! `Popover<State, Action, TriggerV, ContentV>` is the builder; `.render(&theme)`
//! produces a `PopoverView`.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! use void_ui::components::popover;
//! use void_ui::{button, label, OverlayAnchor};
//!
//! popover(
//!     button(|_: &mut State| {}).label("Show info").render(&theme),
//!     label("Some helpful information here.").render(&theme),
//! )
//! .anchor(OverlayAnchor::BottomStart)
//! .render(&theme)
//! # ;
//! ```
//!
//! `render` erases the content view into an `Arc`. At `build`, the view looks
//! for the nearest [`crate::overlay_scope`]'s [`OverlayPortal`] in the xilem
//! `Environment`: if present, the content is *registered* with the portal (the
//! scope's own view mounts it in the always-on-top `PortalSlot`) and the
//! `PopoverHost` hosts only the trigger; otherwise the content is built
//! in-tree under the host's `AnchoredOverlay`, exactly as before.

use std::marker::PhantomData;
use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{PopoverHost, PopoverOpenChanged};
use crate::Theme;
use crate::anchored_overlay::AnchoredOverlay;
use crate::overlay::{OverlayAnchor, OverlaySurface, SurfaceStyle};
use crate::overlay_portal::{
    OverlayPortal, PortalContentView, PortalContentViewState, PortalPlacement, portal_from_env,
};

/// Builder for a popover.
///
/// Create with [`popover`]; configure with builder methods; materialize as a
/// xilem view via [`Self::render`].
#[must_use = "Popover does nothing until rendered with .render(&theme)"]
pub struct Popover<State, Action, TriggerV, ContentV> {
    trigger: TriggerV,
    content: ContentV,
    anchor: OverlayAnchor,
    open: Option<bool>,
    on_open_change: Option<OpenChangeFn<State, Action>>,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Boxed open-change observer: `Fn(&mut State, new_open) -> Action`.
type OpenChangeFn<State, Action> = Arc<dyn Fn(&mut State, bool) -> Action + Send + Sync>;

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
        anchor: OverlayAnchor::BottomStart,
        open: None,
        on_open_change: None,
        phantom: PhantomData,
    }
}

impl<State, Action, TriggerV, ContentV> Popover<State, Action, TriggerV, ContentV>
where
    State: 'static,
    Action: 'static,
    TriggerV: WidgetView<State, Action>,
    ContentV: WidgetView<State, Action>,
{
    /// Set where the content panel appears relative to the trigger.
    pub fn anchor(mut self, anchor: OverlayAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Host-control the open state (controlled mode). When set, the widget
    /// mirrors this prop: user intents (trigger click, Escape, outside
    /// dismissal) no longer self-toggle — they fire
    /// [`Self::on_open_change`] with the desired state, and the host applies
    /// it by passing the new value here on the next rebuild. Omit for the
    /// default uncontrolled behavior.
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Observe open/close transitions (fires in both controlled and
    /// uncontrolled mode) with the new open state.
    pub fn on_open_change<G>(mut self, f: G) -> Self
    where
        G: Fn(&mut State, bool) -> Action + Send + Sync + 'static,
    {
        self.on_open_change = Some(Arc::new(f));
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> PopoverView<TriggerV, State, Action> {
        let content: Arc<PortalContentView<State, Action>> = Arc::new(self.content);
        PopoverView {
            trigger: self.trigger,
            content,
            anchor: self.anchor,
            theme: *theme,
            open: self.open,
            on_open_change: self.on_open_change,
            phantom: PhantomData,
        }
    }
}

/// The materialized xilem `View` backing a [`Popover`].
///
/// Not constructed directly; use [`Popover::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct PopoverView<TriggerV, State, Action> {
    trigger: TriggerV,
    content: Arc<PortalContentView<State, Action>>,
    anchor: OverlayAnchor,
    theme: Theme,
    open: Option<bool>,
    on_open_change: Option<OpenChangeFn<State, Action>>,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<TriggerV, State, Action> ViewMarker for PopoverView<TriggerV, State, Action> {}

/// Map an anchor for use with the in-tree `AnchoredOverlay` fallback (no
/// scope ancestor). [`OverlayAnchor::ViewportQuarter`] is meaningless without
/// an enclosing scope/viewport to center against — `AnchoredOverlay` would
/// place it relative to the trigger's own (typically tiny) footprint instead,
/// so it's mapped to the default trigger-relative anchor.
fn in_tree_anchor(anchor: OverlayAnchor) -> OverlayAnchor {
    match anchor {
        OverlayAnchor::ViewportQuarter => OverlayAnchor::BottomStart,
        other => other,
    }
}

/// Where this popover's content is bound: the nearest scope's portal
/// (registered by key; the scope's view mounts/rebuilds it), or in-tree under
/// our own `PopoverHost` (fallback).
enum ContentBinding<State: 'static, Action: 'static> {
    Portal {
        portal: OverlayPortal<State, Action>,
        key: u64,
    },
    InTree {
        content_vs: PortalContentViewState<State, Action>,
    },
}

/// View state for `PopoverView`: the trigger's child view state plus the
/// content binding (see [`ContentBinding`]).
#[doc(hidden)]
pub struct PopoverViewState<TriggerVS, State: 'static, Action: 'static> {
    trigger_vs: TriggerVS,
    binding: ContentBinding<State, Action>,
}

impl<TriggerV, State, Action> View<State, Action, ViewCtx> for PopoverView<TriggerV, State, Action>
where
    State: 'static,
    Action: 'static,
    TriggerV: WidgetView<State, Action>,
{
    type Element = Pod<PopoverHost>;
    type ViewState = PopoverViewState<TriggerV::ViewState, State, Action>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let portal = portal_from_env::<State, Action>(ctx);
        let (trigger_pod, trigger_vs) = self.trigger.build(ctx, app_state);
        if let Some(portal) = portal {
            let key = portal.register(
                self.content.clone(),
                &self.theme,
                PortalPlacement::Trigger,
                SurfaceStyle::Popover,
            );
            let widget = PopoverHost::new_portal(
                trigger_pod.new_widget.erased(),
                self.anchor,
                &self.theme,
                portal.scope().clone(),
                key,
            )
            .with_open_state(self.open.unwrap_or(false), self.open.is_some());
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                PopoverViewState {
                    trigger_vs,
                    binding: ContentBinding::Portal { portal, key },
                },
            )
        } else {
            let (content_pod, content_vs) = self.content.build(ctx, app_state);
            let widget = PopoverHost::new(
                trigger_pod.new_widget.erased(),
                content_pod.new_widget.erased(),
                in_tree_anchor(self.anchor),
                &self.theme,
            )
            .with_open_state(self.open.unwrap_or(false), self.open.is_some());
            let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
            (
                element,
                PopoverViewState {
                    trigger_vs,
                    binding: ContentBinding::InTree { content_vs },
                },
            )
        }
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        match &mut view_state.binding {
            ContentBinding::Portal { portal, key } => {
                {
                    let mut trigger = PopoverHost::trigger_mut(&mut element);
                    self.trigger.rebuild(
                        &prev.trigger,
                        &mut view_state.trigger_vs,
                        ctx,
                        trigger.downcast(),
                        app_state,
                    );
                }
                // Content rebuild happens when the scope's view diffs the
                // registry (after our subtree's rebuild returns) — we only
                // refresh the registered view value here.
                portal.update(
                    *key,
                    self.content.clone(),
                    &self.theme,
                    PortalPlacement::Trigger,
                    SurfaceStyle::Popover,
                );
            }
            ContentBinding::InTree { content_vs } => {
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
                let mut surface = overlay.downcast::<OverlaySurface>();
                let mut content = OverlaySurface::content_mut(&mut surface);
                self.content.rebuild(
                    &prev.content,
                    content_vs,
                    ctx,
                    content.downcast(),
                    app_state,
                );
            }
        }
        if self.theme != prev.theme {
            PopoverHost::set_theme(&mut element, &self.theme);
        }
        let anchor = match &view_state.binding {
            ContentBinding::Portal { .. } => self.anchor,
            ContentBinding::InTree { .. } => in_tree_anchor(self.anchor),
        };
        let prev_anchor = match &view_state.binding {
            ContentBinding::Portal { .. } => prev.anchor,
            ContentBinding::InTree { .. } => in_tree_anchor(prev.anchor),
        };
        if anchor != prev_anchor {
            PopoverHost::set_anchor(&mut element, anchor);
        }
        if self.open.is_some() != prev.open.is_some() {
            PopoverHost::set_controlled(&mut element, self.open.is_some());
        }
        if let Some(open) = self.open {
            // Unconditional: `set_open` no-ops when current, and re-applying
            // heals any mirror drift after safety closes (stash, slot
            // dismissal).
            PopoverHost::set_open(&mut element, open);
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        match &mut view_state.binding {
            ContentBinding::Portal { portal, key } => {
                {
                    let mut trigger = PopoverHost::trigger_mut(&mut element);
                    self.trigger
                        .teardown(&mut view_state.trigger_vs, ctx, trigger.downcast());
                }
                // The scope's next rebuild (same pass) unmounts the slot child.
                portal.deregister(*key);
            }
            ContentBinding::InTree { content_vs } => {
                let mut overlay_host = PopoverHost::overlay_host_mut(&mut element);
                let mut primary = AnchoredOverlay::primary_mut(&mut overlay_host);
                self.trigger
                    .teardown(&mut view_state.trigger_vs, ctx, primary.downcast());
                drop(primary);
                let mut overlay = AnchoredOverlay::overlay_mut(&mut overlay_host);
                let mut surface = overlay.downcast::<OverlaySurface>();
                let mut content = OverlaySurface::content_mut(&mut surface);
                self.content.teardown(content_vs, ctx, content.downcast());
            }
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if message.remaining_path().is_empty() {
            if let Some(action) = message.take_message::<PopoverOpenChanged>() {
                let PopoverOpenChanged(open) = *action;
                return match &self.on_open_change {
                    Some(f) => MessageResult::Action(f(app_state, open)),
                    None => MessageResult::Nop,
                };
            }
            return MessageResult::Stale;
        }
        match &mut view_state.binding {
            ContentBinding::Portal { .. } => {
                // Content messages route through the scope's slot path, never
                // through us.
                let mut trigger = PopoverHost::trigger_mut(&mut element);
                self.trigger.message(
                    &mut view_state.trigger_vs,
                    message,
                    trigger.downcast(),
                    app_state,
                )
            }
            ContentBinding::InTree { content_vs } => {
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
                        let mut surface = overlay.downcast::<OverlaySurface>();
                        let mut content = OverlaySurface::content_mut(&mut surface);
                        self.content
                            .message(content_vs, message, content.downcast(), app_state)
                    }
                    other => other,
                }
            }
        }
    }
}
