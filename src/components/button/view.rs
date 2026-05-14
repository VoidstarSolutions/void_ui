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
/// Created with [`button`]. Returns a xilem `WidgetView` via [`Self::render`].
#[must_use = "Button does nothing until rendered with .render(&theme)"]
pub struct Button<F> {
    label: ArcStr,
    active: bool,
    disabled: bool,
    variant: ButtonVariant,
    icon: Option<Arc<BezPath>>,
    callback: F,
}

/// Create a new button with the given label and click callback.
///
/// The callback is invoked on primary-pointer release inside the widget and on
/// Space / Enter while the widget is focused.
pub fn button<F>(label: impl Into<ArcStr>, callback: F) -> Button<F> {
    Button {
        label: label.into(),
        active: false,
        disabled: false,
        variant: ButtonVariant::Default,
        icon: None,
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

    /// Set the visual style variant.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
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

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ButtonView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        ButtonView {
            label: self.label,
            active: self.active,
            disabled: self.disabled,
            variant: self.variant,
            icon: self.icon,
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
    label: ArcStr,
    active: bool,
    disabled: bool,
    variant: ButtonVariant,
    icon: Option<Arc<BezPath>>,
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
        } else {
            self.theme.palette.text
        };
        let mut label = Label::new(self.label.clone())
            .with_style(StyleProperty::FontSize(self.theme.density.ui_font_size))
            .prepare();
        label.properties.insert(ContentColor::new(text_color));
        let widget = ThemedButton::new(label, &self.theme)
            .with_active(self.active)
            .with_disabled(self.disabled)
            .with_variant(self.variant)
            .with_icon(self.icon.clone());
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
            ThemedButton::set_theme(&mut element, &self.theme);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
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
        if self.disabled != prev.disabled {
            ThemedButton::set_disabled(&mut element, self.disabled);
            let text_color = if self.disabled {
                self.theme.palette.text_faint
            } else {
                self.theme.palette.text
            };
            let mut child = ThemedButton::child_mut(&mut element);
            child.insert_prop(ContentColor::new(text_color));
        }
        if self.variant != prev.variant {
            ThemedButton::set_variant(&mut element, self.variant);
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
        match message.take_message::<ButtonPress>() {
            Some(_press) => MessageResult::Action((self.callback)(app_state)),
            None => MessageResult::Stale,
        }
    }
}
