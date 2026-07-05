//! Tessera `.tb-btn` button — interactive, theme-driven.
//!
//! Wraps [`super::widget::ThemedButton`] in a xilem [`View`]. Pointer state
//! (hover, press) is tracked by the masonry widget; the `active` flag is the
//! host-controlled selected-toggle state.
//!
//! ```ignore
//! use void_ui::components::button;
//! button("Reset view", |s: &mut State| s.reset())
//!     .active(false)
//!     .render(&theme)
//! ```

use std::marker::PhantomData;

use lucide_icons::Icon as LucideIcon;
use masonry::core::{ArcStr, StyleProperty, Widget as _};
use masonry::kurbo::RoundedRectRadii;
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::peniko::color::{AlphaColor, Srgb};
use xilem::{Pod, ViewCtx};

use super::ButtonVariant;
use super::widget::ThemedButton;
use crate::Theme;
use crate::components::spinner::widget::SpinnerWidget;

/// Builder for an interactive themed button.
///
/// Created with [`button`]; label and icon are optional and set via builder
/// methods. Returns a xilem [`View`] via [`Self::render`].
///
/// [`View`]: xilem::core::View
#[must_use = "Button does nothing until rendered with .render(&theme)"]
pub struct Button<F> {
    label: Option<ArcStr>,
    accessible_name: Option<ArcStr>,
    active: bool,
    disabled: bool,
    loading: bool,
    variant: ButtonVariant,
    icon: Option<LucideIcon>,
    trailing_icon: Option<LucideIcon>,
    corners: Option<RoundedRectRadii>,
    tint: Option<AlphaColor<Srgb>>,
    callback: F,
}

/// Create a new button with the given click callback.
///
/// Label and icon start as `None`; set them via `.label()` and `.icon()`.
/// The callback is invoked on primary-pointer release inside the widget and on
/// Space / Enter while the widget is focused.
pub fn button<F>(callback: F) -> Button<F> {
    Button {
        label: None,
        accessible_name: None,
        active: false,
        disabled: false,
        loading: false,
        variant: ButtonVariant::Default,
        icon: None,
        trailing_icon: None,
        corners: None,
        tint: None,
        callback,
    }
}

impl<F> Button<F> {
    /// Mark this button as the currently-selected toggle.
    pub fn active(mut self, on: bool) -> Self {
        self.active = on;
        self
    }

    /// Suppress all interaction and mute the visual appearance.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Show a spinner and block all interaction until the operation completes.
    pub fn loading(mut self, on: bool) -> Self {
        self.loading = on;
        self
    }

    /// Set the visual style variant.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Set or replace the text label.
    ///
    /// Useful when constructing via [`icon_button`] and a label is also desired.
    pub fn label(mut self, text: impl Into<ArcStr>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// Attach a leading icon from the Lucide icon set.
    pub fn icon(mut self, name: LucideIcon) -> Self {
        self.icon = Some(name);
        self
    }

    /// Attach a trailing icon from the Lucide icon set (e.g. a dropdown caret).
    pub fn trailing_icon(mut self, name: LucideIcon) -> Self {
        self.trailing_icon = Some(name);
        self
    }

    /// Set an explicit accessible name for this button.
    ///
    /// Required for icon-only buttons so screen readers can describe the action.
    /// When a visible label is present this is redundant but harmless.
    pub fn accessible_name(mut self, name: impl Into<ArcStr>) -> Self {
        self.accessible_name = Some(name.into());
        self
    }

    /// Override the corner radii for the button background and focus ring.
    ///
    /// Used by button groups to round only the outer edges of the group.
    /// Standalone buttons do not need this — the default `5px` radius is used.
    pub fn corners(mut self, radii: RoundedRectRadii) -> Self {
        self.corners = Some(radii);
        self
    }

    /// Override the label and icon color, e.g. to match a surrounding
    /// accent-colored card. Ignored while [`Self::disabled`].
    pub fn tint(mut self, color: AlphaColor<Srgb>) -> Self {
        self.tint = Some(color);
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ButtonView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        ButtonView {
            label: self.label,
            accessible_name: self.accessible_name,
            active: self.active,
            disabled: self.disabled,
            loading: self.loading,
            variant: self.variant,
            icon: self.icon,
            trailing_icon: self.trailing_icon,
            corners: self.corners,
            tint: self.tint,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Button`].
///
/// Built only through [`Button::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ButtonView<F, State, Action> {
    label: Option<ArcStr>,
    accessible_name: Option<ArcStr>,
    active: bool,
    disabled: bool,
    loading: bool,
    variant: ButtonVariant,
    icon: Option<LucideIcon>,
    trailing_icon: Option<LucideIcon>,
    corners: Option<RoundedRectRadii>,
    tint: Option<AlphaColor<Srgb>>,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}
impl<F, State, Action> ButtonView<F, State, Action> {
    fn text_color(&self) -> AlphaColor<Srgb> {
        if self.disabled {
            self.theme.palette.text_faint
        } else if let Some(tint) = self.tint {
            tint
        } else if self.variant == ButtonVariant::Link {
            self.theme.palette.accent
        } else {
            self.theme.palette.text
        }
    }

    fn icon_color(&self) -> AlphaColor<Srgb> {
        if self.disabled {
            self.theme.palette.text_faint
        } else if let Some(tint) = self.tint {
            tint
        } else {
            self.theme.palette.text
        }
    }
}

impl<F, State, Action> ViewMarker for ButtonView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for ButtonView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ThemedButton>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let text_color = self.text_color();
        let mut label = Label::new(self.label.clone().unwrap_or_default())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        label.properties.insert(ContentColor::new(text_color));
        let icon_color = self.icon_color();
        let mut widget = ThemedButton::new(label, &self.theme)
            .with_active(self.active)
            .with_disabled(self.disabled)
            .with_loading(self.loading)
            .with_variant(self.variant)
            .with_accessibility_label(self.accessible_name.clone());
        if let Some(name) = self.icon {
            widget = widget.with_icon(
                super::super::icon::icon(name)
                    .color(icon_color)
                    .build_widget(&self.theme),
            );
        }
        if let Some(name) = self.trailing_icon {
            widget = widget.with_trailing_icon(
                super::super::icon::icon(name)
                    .color(icon_color)
                    .build_widget(&self.theme),
            );
        }
        if let Some(corners) = self.corners {
            widget = widget.with_corners(corners);
        }
        // `with_action_widget` registers the widget as an action source so that
        // `ButtonPress` events bubble up to this view's `message` handler rather
        // than being dropped by xilem's dispatch loop.
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
        // Only push properties that actually changed; each setter on ThemedButton
        // guards behind a `!=` check and calls `request_layout` / `request_paint_only`
        // only when needed, so diffing here avoids spurious repaints.
        if self.theme != prev.theme {
            ThemedButton::set_theme(&mut element, &self.theme);
            let text_color = self.text_color();
            let mut child = ThemedButton::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
            let mut lbl = child.downcast::<Label>();
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.theme != prev.theme
            && self.loading
            && let Some(mut s) = ThemedButton::spinner_mut(&mut element)
        {
            SpinnerWidget::set_color(&mut s, self.theme.palette.text_muted);
            SpinnerWidget::set_size(&mut s, f64::from(self.theme.density.ui_font_size));
        }
        if self.active != prev.active {
            ThemedButton::set_active(&mut element, self.active);
        }
        if self.loading != prev.loading {
            ThemedButton::set_loading(&mut element, self.loading);
        }
        if self.disabled != prev.disabled {
            ThemedButton::set_disabled(&mut element, self.disabled);
            let text_color = self.text_color();
            let mut child = ThemedButton::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
        }
        if self.variant != prev.variant {
            ThemedButton::set_variant(&mut element, self.variant);
            // Link gains/loses teal text; Text and others revert to default.
            if !self.disabled
                && (self.variant == ButtonVariant::Link || prev.variant == ButtonVariant::Link)
            {
                let text_color = self.text_color();
                let mut child = ThemedButton::child_mut(&mut element);
                child.insert_prop(ContentColor::new(text_color));
            }
        }
        let icon_color = self.icon_color();
        if self.icon.map(char::from) != prev.icon.map(char::from) {
            match self.icon {
                Some(name) => ThemedButton::attach_icon(
                    &mut element,
                    super::super::icon::icon(name)
                        .color(icon_color)
                        .build_widget(&self.theme),
                ),
                None => ThemedButton::detach_icon(&mut element),
            }
        } else if (self.theme != prev.theme
            || self.disabled != prev.disabled
            || self.tint != prev.tint)
            && let Some(mut m) = ThemedButton::icon_mut(&mut element)
        {
            m.insert_prop(ContentColor::new(icon_color));
            Label::insert_style(
                &mut m,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.trailing_icon.map(char::from) != prev.trailing_icon.map(char::from) {
            match self.trailing_icon {
                Some(name) => ThemedButton::attach_trailing_icon(
                    &mut element,
                    super::super::icon::icon(name)
                        .color(icon_color)
                        .build_widget(&self.theme),
                ),
                None => ThemedButton::detach_trailing_icon(&mut element),
            }
        } else if (self.theme != prev.theme
            || self.disabled != prev.disabled
            || self.tint != prev.tint)
            && let Some(mut m) = ThemedButton::trailing_icon_mut(&mut element)
        {
            m.insert_prop(ContentColor::new(icon_color));
            Label::insert_style(
                &mut m,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.accessible_name != prev.accessible_name {
            ThemedButton::set_accessibility_label(&mut element, self.accessible_name.clone());
        }
        if self.corners != prev.corners
            || (self.corners.is_none() && self.theme.radius != prev.theme.radius)
        {
            use masonry::kurbo::RoundedRectRadii as R;
            let radii = self
                .corners
                .unwrap_or_else(|| R::from_single_radius(f64::from(self.theme.radius.small)));
            ThemedButton::set_corners(&mut element, radii);
        }
        // Label text is not re-applied here; masonry's Label doesn't expose
        // stable post-construction text mutation. If the label string changes
        // the parent view should replace the whole button, which xilem will do
        // via teardown+build when the identity changes.
        let _ = &prev.label;
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
        // `take_message` consumes the `ButtonPress` posted by the widget; returning
        // `Stale` for any other message type tells xilem to skip this view in its
        // dispatch walk (the message was meant for a different handler).
        match message.take_message::<ButtonPress>() {
            Some(_press) => MessageResult::Action((self.callback)(app_state)),
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ButtonVariant;
    use super::button;
    use crate::Theme;

    #[test]
    fn default_colors_are_theme_text() {
        let theme = Theme::dark();
        let view = button(|(): &mut ()| ()).render::<(), ()>(&theme);
        assert_eq!(view.text_color(), theme.palette.text);
        assert_eq!(view.icon_color(), theme.palette.text);
    }

    #[test]
    fn link_variant_uses_accent_text_but_default_icon_color() {
        let theme = Theme::dark();
        let view = button(|(): &mut ()| ())
            .variant(ButtonVariant::Link)
            .render::<(), ()>(&theme);
        assert_eq!(view.text_color(), theme.palette.accent);
        assert_eq!(view.icon_color(), theme.palette.text);
    }

    #[test]
    fn tint_overrides_text_and_icon_color() {
        let theme = Theme::dark();
        let view = button(|(): &mut ()| ())
            .tint(theme.palette.blue)
            .render::<(), ()>(&theme);
        assert_eq!(view.text_color(), theme.palette.blue);
        assert_eq!(view.icon_color(), theme.palette.blue);
    }

    #[test]
    fn disabled_ignores_tint_and_variant() {
        let theme = Theme::dark();
        let view = button(|(): &mut ()| ())
            .variant(ButtonVariant::Link)
            .tint(theme.palette.blue)
            .disabled(true)
            .render::<(), ()>(&theme);
        assert_eq!(view.text_color(), theme.palette.text_faint);
        assert_eq!(view.icon_color(), theme.palette.text_faint);
    }
}
