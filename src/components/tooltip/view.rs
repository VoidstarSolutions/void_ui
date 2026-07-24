//! Xilem view that wraps any child view with hover-driven tooltip behavior.
//!
//! Popup content is mounted through the outermost `overlay_scope`'s
//! `OverlayPortal` (`root_portal_lookup`), the same mechanism `dialog` uses —
//! required, no in-tree fallback, since Layer-hosted content can't route
//! `View::message` (see
//! `docs/superpowers/specs/2026-07-21-tooltip-arbitrary-content-design.md`).
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! use void_ui::components::{button, label, tooltip};
//! tooltip(
//!     label("Reset the chart to defaults").render(&theme),
//!     button(|_: &mut State| {}).label("Reset").render(&theme),
//! )
//! .render(&theme)
//! # ;
//! ```

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx, WidgetView};

use super::widget::TooltipHost;
use crate::Theme;
use crate::overlay::SurfaceStyle;
use crate::overlay_portal::{OverlayPortal, PortalContentView, PortalPlacement};
use crate::overlay_scope::root_portal_lookup;

/// Default hover-idle delay before the tooltip appears.
pub const DEFAULT_DELAY_MS: u64 = 300;

/// Builder for a hover-driven tooltip wrapping an inner view.
///
/// Created with [`tooltip`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Tooltip does nothing until rendered with .render(&theme)"]
pub struct Tooltip<ChildV, ContentV> {
    content: ContentV,
    child: ChildV,
    delay: Duration,
}

/// Wraps `child` with hover-driven tooltip behavior showing `content` — any
/// view, e.g. `label("...").render(theme)` for plain text, or a richer
/// composition for something like a status legend.
///
/// The tooltip surface appears after the pointer has been idle over the
/// child for the configured delay (default 300 ms). It dismisses itself on
/// the next pointer activity.
pub fn tooltip<ChildV, ContentV>(content: ContentV, child: ChildV) -> Tooltip<ChildV, ContentV> {
    Tooltip {
        content,
        child,
        delay: Duration::from_millis(DEFAULT_DELAY_MS),
    }
}

impl<ChildV, ContentV> Tooltip<ChildV, ContentV> {
    /// Sets the hover-idle delay before the tooltip appears.
    pub fn delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> TooltipView<ChildV, State, Action>
    where
        State: 'static,
        Action: 'static,
        ChildV: WidgetView<State, Action>,
        ContentV: WidgetView<State, Action>,
    {
        let content: Arc<PortalContentView<State, Action>> = Arc::new(self.content);
        TooltipView {
            content,
            child: self.child,
            delay: self.delay,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Tooltip`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TooltipView<ChildV, State, Action> {
    content: Arc<PortalContentView<State, Action>>,
    child: ChildV,
    delay: Duration,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<ChildV, State, Action> ViewMarker for TooltipView<ChildV, State, Action> {}

/// View state for `TooltipView`: the portal registration/key for the popup
/// content, plus the wrapped child's own view state.
#[doc(hidden)]
pub struct TooltipViewState<State: 'static, Action: 'static, ChildState> {
    portal: OverlayPortal<State, Action>,
    key: u64,
    child: ChildState,
}

impl<ChildV, State, Action> View<State, Action, ViewCtx> for TooltipView<ChildV, State, Action>
where
    State: 'static,
    Action: 'static,
    ChildV: WidgetView<State, Action>,
{
    type Element = Pod<TooltipHost>;
    type ViewState = TooltipViewState<State, Action, ChildV::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);

        // Targets the outermost scope ancestor, mirroring `dialog` exactly —
        // a tooltip has no trigger rect it needs to stay confined to, so
        // "no scope" is a clear build-time panic, the same tradeoff dialog
        // already made. `or_panic` distinguishes "no scope at all" from "a
        // scope that isn't yours".
        let portal = root_portal_lookup::<State, Action>().or_panic("tooltip");
        let key = portal.register(
            self.content.clone(),
            &self.theme,
            PortalPlacement::Trigger,
            SurfaceStyle::Tooltip,
        );

        let widget = TooltipHost::new(
            child_pod.new_widget.erased(),
            portal.scope().clone(),
            key,
            self.delay,
        );
        let element = ctx.create_pod(widget);
        (
            element,
            TooltipViewState {
                portal,
                key,
                child: child_state,
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
        // Content rebuild happens when the scope's view diffs the registry
        // (after our subtree's rebuild returns) — we only refresh the
        // registered view value here, unconditionally, same as `dialog`.
        view_state.portal.update(
            view_state.key,
            self.content.clone(),
            &self.theme,
            PortalPlacement::Trigger,
            SurfaceStyle::Tooltip,
        );
        if self.delay != prev.delay {
            TooltipHost::set_delay(&mut element, self.delay);
        }
        let mut child = TooltipHost::child_mut(&mut element);
        self.child.rebuild(
            &prev.child,
            &mut view_state.child,
            ctx,
            child.downcast(),
            app_state,
        );
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        view_state.portal.deregister(view_state.key);
        let mut child = TooltipHost::child_mut(&mut element);
        self.child
            .teardown(&mut view_state.child, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Content messages route through the scope's slot path, never
        // through us — same as `dialog`.
        let mut child = TooltipHost::child_mut(&mut element);
        self.child
            .message(&mut view_state.child, message, child.downcast(), app_state)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use xilem::ViewCtx;
    use xilem::core::View;

    use super::tooltip;
    use crate::Theme;
    use crate::label::label;
    use crate::overlay_scope::overlay_scope;
    use crate::test_support;

    #[derive(Default)]
    struct AppState;

    #[test]
    fn delay_is_the_canonical_builder_name() {
        let theme = Theme::default();
        let _ = tooltip(
            label("hint").render::<(), ()>(&theme),
            label("child").render::<(), ()>(&theme),
        )
        .delay(Duration::from_millis(500))
        .render::<(), ()>(&theme);
    }

    /// A `tooltip` rendered with the same `State`/`Action` pair as the root
    /// `overlay_scope` ancestor registers successfully — mirrors `dialog`'s
    /// identical test.
    #[test]
    fn tooltip_builds_under_a_same_typed_root_overlay_scope() {
        let theme = Theme::default();
        let runtime = test_support::current_thread_runtime();
        let proxy = test_support::noop_proxy();

        let content = label("hint").render::<AppState, ()>(&theme);
        let child = label("child").render::<AppState, ()>(&theme);
        let scope = overlay_scope(tooltip(content, child).render(&theme));

        let mut ctx = ViewCtx::new(proxy, runtime);
        let mut state = AppState;
        let _ = scope.build(&mut ctx, &mut state);
    }
}
