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
use std::sync::Arc;

use masonry::core::{ArcStr, StyleProperty, Widget as _};
use masonry::kurbo::BezPath;
use masonry::properties::ContentColor;
use masonry::widgets::{ButtonPress, Label};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::ButtonVariant;
use super::widget::ThemedButton;
use crate::Theme;

/// Builder for an interactive themed button.
///
/// Created with [`button`] (label required) or [`icon_button`] (icon required).
/// Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Button does nothing until rendered with .render(&theme)"]
pub struct Button<F> {
    label: Option<ArcStr>,
    accessible_name: Option<ArcStr>,
    active: bool,
    disabled: bool,
    loading: bool,
    variant: ButtonVariant,
    icon: Option<Arc<BezPath>>,
    trailing_icon: Option<Arc<BezPath>>,
    callback: F,
}

/// Create a new button with the given label and click callback.
///
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

    /// Attach a leading icon.
    ///
    /// `path` must be in a 0..1 coordinate space (unit square); it is scaled
    /// uniformly to the theme's UI font size at paint time.
    pub fn icon(mut self, path: BezPath) -> Self {
        self.icon = Some(Arc::new(path));
        self
    }

    /// Attach a trailing icon (e.g. a dropdown caret).
    ///
    /// Same coordinate space and scaling rules as [`Self::icon`].
    pub fn trailing_icon(mut self, path: BezPath) -> Self {
        self.trailing_icon = Some(Arc::new(path));
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
    icon: Option<Arc<BezPath>>,
    trailing_icon: Option<Arc<BezPath>>,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
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
        let text_color = if self.disabled {
            self.theme.palette.text_faint
        } else if self.variant == ButtonVariant::Link {
            self.theme.palette.teal
        } else {
            self.theme.palette.text
        };
        let mut label = Label::new(self.label.clone().unwrap_or_default())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        label.properties.insert(ContentColor::new(text_color));
        let widget = ThemedButton::new(label, &self.theme)
            .with_active(self.active)
            .with_disabled(self.disabled)
            .with_loading(self.loading)
            .with_variant(self.variant)
            .with_icon(self.icon.clone())
            .with_trailing_icon(self.trailing_icon.clone())
            .with_accessibility_label(self.accessible_name.clone());
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
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else if self.variant == ButtonVariant::Link {
                self.theme.palette.teal
            } else {
                self.theme.palette.text
            };
            let mut child = ThemedButton::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
            let mut lbl = child.downcast::<Label>();
            Label::insert_style(
                &mut lbl,
                StyleProperty::FontSize(self.theme.density.ui_font_size),
            );
        }
        if self.active != prev.active {
            ThemedButton::set_active(&mut element, self.active);
        }
        if self.loading != prev.loading {
            ThemedButton::set_loading(&mut element, self.loading);
        }
        if self.disabled != prev.disabled {
            ThemedButton::set_disabled(&mut element, self.disabled);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else if self.variant == ButtonVariant::Link {
                self.theme.palette.teal
            } else {
                self.theme.palette.text
            };
            let mut child = ThemedButton::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
        }
        if self.variant != prev.variant {
            ThemedButton::set_variant(&mut element, self.variant);
            // Link gains/loses teal text; Text and others revert to default.
            if !self.disabled
                && (self.variant == ButtonVariant::Link || prev.variant == ButtonVariant::Link)
            {
                let text_color = if self.variant == ButtonVariant::Link {
                    self.theme.palette.teal
                } else {
                    self.theme.palette.text
                };
                let mut child = ThemedButton::child_mut(&mut element);
                child.insert_prop(ContentColor::new(text_color));
            }
        }
        // BezPath has no PartialEq — compare Arc pointers instead.
        let icon_changed = match (&self.icon, &prev.icon) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        };
        if icon_changed {
            ThemedButton::set_icon(&mut element, self.icon.clone());
        }
        let trailing_icon_changed = match (&self.trailing_icon, &prev.trailing_icon) {
            (None, None) => false,
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            _ => true,
        };
        if trailing_icon_changed {
            ThemedButton::set_trailing_icon(&mut element, self.trailing_icon.clone());
        }
        if self.accessible_name != prev.accessible_name {
            ThemedButton::set_accessibility_label(&mut element, self.accessible_name.clone());
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
