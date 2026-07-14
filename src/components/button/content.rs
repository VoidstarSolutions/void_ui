//! Themed button wrapped around an arbitrary child view.
//!
//! [`button`](super::button) renders a `Label` (plus optional icons) it builds
//! itself, which is the right shape for the overwhelmingly common case but
//! leaves no way to make a *composite* — a `flex_row` of columns, a card — the
//! clickable thing. [`content_button`] fills that gap: it wraps any child view
//! in the same [`ThemedButton`] widget, so a whole row or card gets the button's
//! variants, hover/press fills, focus ring, Space/Enter activation and
//! `Role::Button` accessibility without the caller dropping to the widget layer.
//!
//! ```ignore
//! use void_ui::{content_button, label};
//! use xilem::view::flex_row;
//!
//! content_button(
//!     flex_row((
//!         label("AAPL").render(&theme),
//!         label("+1.2%").render(&theme),
//!     )),
//!     |s: &mut State| s.open_symbol(),
//! )
//! .variant(ButtonVariant::Ghost)
//! .accessible_name("Open AAPL")
//! .render(&theme)
//! ```
//!
//! The child is **content, not a control**: [`ThemedButton`] does not propagate
//! pointer interaction to its children, so an interactive child (a nested
//! button, a checkbox) never sees hover or press — it would be swallowed by the
//! outer button's pointer capture. Compose static content here; if a row needs
//! *both* a whole-row click and interactive controls inside it, that's
//! [`clickable_row`](crate::clickable_row), which reserves a hit zone for them.
//!
//! Because the child is an arbitrary view, it also owns its own colors: the
//! caller renders it with a theme, so `disabled` mutes the button's background
//! but cannot mute the child's text (pass an already-muted child when the
//! button is disabled). That's the same boundary [`label`](crate::label) draws.

use std::marker::PhantomData;

use masonry::core::ArcStr;
use masonry::kurbo::RoundedRectRadii;
use masonry::widgets::ButtonPress;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::ButtonVariant;
use super::widget::ThemedButton;
use crate::Theme;

/// Builder for a themed button wrapped around an arbitrary child view.
///
/// Created with [`content_button`]; materialized with [`Self::render`].
#[must_use = "ContentButton does nothing until rendered with .render(&theme)"]
pub struct ContentButton<V, F> {
    child: V,
    accessible_name: Option<ArcStr>,
    selected: bool,
    disabled: bool,
    variant: ButtonVariant,
    corners: Option<RoundedRectRadii>,
    fill_content: bool,
    callback: F,
}

/// Wrap `child` in a themed, clickable button.
///
/// The callback fires on primary-pointer release inside the button and on
/// Space / Enter while it is focused — the same activation contract as
/// [`button`](super::button).
///
/// The child is static content, not a control (see the [module docs](self)).
/// Set an [`accessible_name`](ContentButton::accessible_name): a composite
/// child has no single string for assistive tech to announce.
pub fn content_button<V, F>(child: V, callback: F) -> ContentButton<V, F> {
    ContentButton {
        child,
        accessible_name: None,
        selected: false,
        disabled: false,
        variant: ButtonVariant::Default,
        corners: None,
        fill_content: true,
        callback,
    }
}

impl<V, F> ContentButton<V, F> {
    /// Mark this button as the currently-selected toggle.
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }

    /// Suppress all interaction and mute the button's background.
    ///
    /// The child renders its own colors — see the [module docs](self).
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Set the visual style variant.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Set the accessible name announced for this button.
    ///
    /// Effectively required here: a composite child gives assistive tech no
    /// single string to fall back on.
    pub fn accessible_name(mut self, name: impl Into<ArcStr>) -> Self {
        self.accessible_name = Some(name.into());
        self
    }

    /// Override the corner radii for the button background and focus ring.
    ///
    /// Defaults to the theme's small radius.
    pub fn corners(mut self, radii: RoundedRectRadii) -> Self {
        self.corners = Some(radii);
        self
    }

    /// Size the content to the button's full inner box (the default), or
    /// center it at its natural size.
    ///
    /// This only bites when the button is *wider than its content* — i.e. when
    /// a parent stretches it. Filling is the default because a composite child
    /// is usually a row: under a fit-and-center layout a flexed column
    /// (`.flex(1.0)`) collapses to its minimum width, so columns would not line
    /// up across rows. Pass `false` for a stretched button whose content should
    /// sit centered, like an oversized label-and-icon card.
    pub fn fill_content(mut self, fill: bool) -> Self {
        self.fill_content = fill;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ContentButtonView<V, F, State, Action>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, ()>,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        ContentButtonView {
            child: self.child,
            accessible_name: self.accessible_name,
            selected: self.selected,
            disabled: self.disabled,
            variant: self.variant,
            corners: self.corners,
            fill_content: self.fill_content,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`ContentButton`].
///
/// Built only through [`ContentButton::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ContentButtonView<V, F, State, Action> {
    child: V,
    accessible_name: Option<ArcStr>,
    selected: bool,
    disabled: bool,
    variant: ButtonVariant,
    corners: Option<RoundedRectRadii>,
    fill_content: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V, F, State, Action> ContentButtonView<V, F, State, Action> {
    /// The corner radii to paint with — the explicit override, else the
    /// theme's small radius.
    fn resolved_corners(&self) -> RoundedRectRadii {
        self.corners.unwrap_or_else(|| {
            RoundedRectRadii::from_single_radius(f64::from(self.theme.radius.small))
        })
    }
}

impl<V, F, State, Action> ViewMarker for ContentButtonView<V, F, State, Action> {}

impl<V, F, State, Action> View<State, Action, ViewCtx> for ContentButtonView<V, F, State, Action>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, ()>,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedButton>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_pod, child_state) = self.child.build(ctx, app_state);
        let widget = ThemedButton::new(child_pod.new_widget, &self.theme)
            .with_selected(self.selected)
            .with_disabled(self.disabled)
            .with_variant(self.variant)
            .with_accessibility_label(self.accessible_name.clone())
            .with_corners(self.resolved_corners())
            .with_stretch_child(self.fill_content);
        // Registers the widget as an action source so its `ButtonPress` bubbles
        // to this view's `message` rather than being dropped by xilem's dispatch.
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
        // Each ThemedButton setter guards behind a `!=` and only then requests
        // layout/paint, so diffing here just avoids redundant calls.
        if self.theme != prev.theme {
            ThemedButton::set_theme(&mut element, &self.theme);
        }
        if self.selected != prev.selected {
            ThemedButton::set_selected(&mut element, self.selected);
        }
        if self.disabled != prev.disabled {
            ThemedButton::set_disabled(&mut element, self.disabled);
        }
        if self.variant != prev.variant {
            ThemedButton::set_variant(&mut element, self.variant);
        }
        if self.accessible_name != prev.accessible_name {
            ThemedButton::set_accessibility_label(&mut element, self.accessible_name.clone());
        }
        if self.fill_content != prev.fill_content {
            ThemedButton::set_stretch_child(&mut element, self.fill_content);
        }
        // A theme swap moves the default radius, so recompute whenever either
        // the override or the theme changed.
        if self.corners != prev.corners || self.theme.radius != prev.theme.radius {
            ThemedButton::set_corners(&mut element, self.resolved_corners());
        }
        // The child owns its own theming: it was rendered by the caller with a
        // theme, so its rebuild pushes any color/size changes down itself.
        let mut child = ThemedButton::child_mut(&mut element);
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
            let mut child = ThemedButton::child_mut(&mut element);
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
    ) -> MessageResult<Action> {
        // A message addressed to *this* button arrives fully routed (empty
        // remaining path) — that's our own `ButtonPress`. A non-empty path is
        // bound for a descendant inside the content and must be forwarded
        // untouched: probing it with `take_message` would panic ("message has
        // not reached its target"). Same guard as `ClickableRow`.
        if message.remaining_path().is_empty() {
            return match message.take_message::<ButtonPress>() {
                Some(_press) => MessageResult::Action((self.callback)(app_state)),
                None => MessageResult::Stale,
            };
        }
        let mut child = ThemedButton::child_mut(&mut element);
        match self
            .child
            .message(view_state, message, child.downcast(), app_state)
        {
            // Content is `Action = ()` and can't reach the host's `Action`
            // type. It has already mutated app state, so ask for a rebuild
            // rather than swallowing the change.
            MessageResult::Action(()) | MessageResult::RequestRebuild => {
                MessageResult::RequestRebuild
            }
            MessageResult::Nop => MessageResult::Nop,
            MessageResult::Stale => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use masonry::kurbo::RoundedRectRadii;

    use super::content_button;
    use crate::Theme;
    use crate::components::ButtonVariant;
    use crate::label;

    /// The child view is `()`-actioned content; these assert the builder's
    /// plain-data surface. Click/keyboard activation and the fill-vs-center
    /// layout are covered at the widget layer in `widget.rs`, which is where
    /// `ThemedButton` owns them.
    #[test]
    fn corners_default_to_the_theme_small_radius() {
        let theme = Theme::dark();
        let view =
            content_button(label("row").render(&theme), |(): &mut ()| ()).render::<(), ()>(&theme);
        assert_eq!(
            view.resolved_corners(),
            RoundedRectRadii::from_single_radius(f64::from(theme.radius.small))
        );
    }

    #[test]
    fn explicit_corners_override_the_theme() {
        let theme = Theme::dark();
        let radii = RoundedRectRadii::from_single_radius(12.0);
        let view = content_button(label("row").render(&theme), |(): &mut ()| ())
            .corners(radii)
            .render::<(), ()>(&theme);
        assert_eq!(view.resolved_corners(), radii);
    }

    #[test]
    fn content_fills_the_button_by_default() {
        let theme = Theme::dark();
        let view =
            content_button(label("row").render(&theme), |(): &mut ()| ()).render::<(), ()>(&theme);
        assert!(view.fill_content, "a composite child is usually a row");

        let centered = content_button(label("row").render(&theme), |(): &mut ()| ())
            .fill_content(false)
            .render::<(), ()>(&theme);
        assert!(!centered.fill_content);
    }

    #[test]
    fn builder_carries_variant_and_state_flags() {
        let theme = Theme::dark();
        let view = content_button(label("row").render(&theme), |(): &mut ()| ())
            .variant(ButtonVariant::Ghost)
            .selected(true)
            .disabled(true)
            .accessible_name("Open AAPL")
            .render::<(), ()>(&theme);
        assert_eq!(view.variant, ButtonVariant::Ghost);
        assert!(view.selected);
        assert!(view.disabled);
        assert_eq!(view.accessible_name.as_deref(), Some("Open AAPL"));
    }
}
