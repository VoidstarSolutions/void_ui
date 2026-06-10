//! Xilem view for the themed single-line text input.
//!
//! Wraps `masonry::widgets::TextInput` (which itself owns a `TextArea` child
//! that does the actual text editing — caret, selection, IME) and themes its
//! chrome from a [`Theme`] snapshot. We build the masonry widget directly
//! rather than going through xilem's `text_input` view because that view only
//! exposes the text/caret/selection/placeholder colors: the box chrome
//! (background, border, corner radius, padding) is gated behind
//! `UsesProperty`, which `TextInput` does not declare for those properties.
//! Inserting them on the raw masonry `PropertySet` / `WidgetMut` — as we do
//! here — bypasses that gate and lets the field match the active theme.
//!
//! ```ignore
//! use void_ui::components::input::input;
//! input(state.name.clone(), |s: &mut State, text| s.name = text)
//!     .placeholder("Your name")
//!     .render(&theme)
//! ```
//!
//! The contents are host-controlled: the widget emits the new string on every
//! edit via the change callback, and the host stores it and passes it back in
//! on the next render. This mirrors every other interactive void-ui component.

use std::marker::PhantomData;

use masonry::core::{ArcStr, NewWidget, PropertySet};
use masonry::layout::Length;
use masonry::parley::StyleProperty;
use masonry::properties::{
    Background, BorderColor, BorderWidth, CaretColor, ContentColor, CornerRadius, Padding,
    PlaceholderColor, SelectionColor,
};
use masonry::widgets::{self, TextAction};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::Theme;

/// Hairline border around the field. Component-local like every other bordered
/// widget (tooltip, checkbox, code view); a 1px stroke doesn't scale with
/// density. Inner padding, by contrast, is read from `Theme.density`.
const BORDER_WIDTH: Length = Length::const_px(1.0);

/// Builder for a themed single-line text input.
///
/// Created with [`input`]. Returns a xilem view via [`Self::render`].
#[must_use = "Input does nothing until rendered with .render(&theme)"]
pub struct Input<F> {
    contents: String,
    placeholder: ArcStr,
    disabled: bool,
    callback: F,
}

/// Create a single-line text input with the given contents and change callback.
///
/// `contents` is host-controlled — the widget never mutates it directly.
/// `on_changed` is invoked with the full updated string on every edit; the
/// host is responsible for storing it and passing it back in on the next
/// render.
pub fn input<F>(contents: impl Into<String>, on_changed: F) -> Input<F> {
    Input {
        contents: contents.into(),
        placeholder: ArcStr::default(),
        disabled: false,
        callback: on_changed,
    }
}

impl<F> Input<F> {
    /// Set the placeholder shown, in a muted color, while the field is empty.
    pub fn placeholder(mut self, placeholder: impl Into<ArcStr>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Disable the field: it stops accepting input and paints muted.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> InputView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    {
        InputView {
            contents: self.contents,
            placeholder: self.placeholder,
            disabled: self.disabled,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing an [`Input`].
///
/// Built only through [`Input::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct InputView<F, State, Action> {
    contents: String,
    placeholder: ArcStr,
    disabled: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> InputView<F, State, Action> {
    /// Theme the inner `TextArea`: text color, caret, and selection highlight.
    fn area_props(&self) -> PropertySet {
        let mut props = PropertySet::new();
        props.insert(ContentColor::new(self.theme.palette.text));
        props.insert(CaretColor {
            color: self.theme.palette.teal,
        });
        props.insert(SelectionColor {
            color: self.theme.palette.teal_soft,
        });
        props
    }

    /// Padding inside the field, derived from the theme's button density so
    /// the field's height lines up with buttons of the same theme.
    fn padding(&self) -> Padding {
        Padding::from_vh(
            Length::px(f64::from(self.theme.density.button_pad_v)),
            Length::px(f64::from(self.theme.density.button_pad_h)),
        )
    }
}

impl<F, State, Action> ViewMarker for InputView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for InputView<F, State, Action>
where
    F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<widgets::TextInput>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let text_area = widgets::TextArea::new_editable(&self.contents)
            .with_style(StyleProperty::FontSize(self.theme.typography.size_body));

        let text_input = widgets::TextInput::from_text_area(
            NewWidget::new(text_area).with_props(self.area_props()),
        )
        .with_placeholder(self.placeholder.clone())
        .with_clip(true);

        // The inner TextArea is the widget that actually emits text actions;
        // route them so this view's `message` is reached.
        let id = text_input.area_pod().id();
        ctx.record_action_source(id);

        let mut pod = ctx.create_pod(text_input);
        pod.new_widget
            .properties
            .insert(Background::Color(self.theme.palette.surface));
        pod.new_widget
            .properties
            .insert(BorderColor::new(self.theme.palette.border));
        pod.new_widget.properties.insert(BorderWidth::all(BORDER_WIDTH));
        pod.new_widget.properties.insert(CornerRadius {
            radius: Length::px(f64::from(self.theme.radius.small)),
        });
        pod.new_widget.properties.insert(self.padding());
        pod.new_widget
            .properties
            .insert(PlaceholderColor::new(self.theme.palette.text_muted));
        pod.new_widget.options.disabled = self.disabled;
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let theme_changed = self.theme != prev.theme;

        if theme_changed {
            element.insert_prop(Background::Color(self.theme.palette.surface));
            element.insert_prop(BorderColor::new(self.theme.palette.border));
            element.insert_prop(BorderWidth::all(BORDER_WIDTH));
            element.insert_prop(CornerRadius {
                radius: Length::px(f64::from(self.theme.radius.small)),
            });
            element.insert_prop(self.padding());
            element.insert_prop(PlaceholderColor::new(self.theme.palette.text_muted));
        }
        if self.placeholder != prev.placeholder {
            widgets::TextInput::set_placeholder(&mut element, self.placeholder.clone());
        }
        if self.disabled != prev.disabled {
            element.ctx.set_disabled(self.disabled);
        }

        let mut text_area = widgets::TextInput::text_mut(&mut element);
        if theme_changed {
            text_area.insert_prop(ContentColor::new(self.theme.palette.text));
            text_area.insert_prop(CaretColor {
                color: self.theme.palette.teal,
            });
            text_area.insert_prop(SelectionColor {
                color: self.theme.palette.teal_soft,
            });
            widgets::TextArea::insert_style(
                &mut text_area,
                StyleProperty::FontSize(self.theme.typography.size_body),
            );
        }
        // Compare against the widget's own text rather than `prev.contents` so
        // that the in-flight edit (which produced this rebuild) is not undone.
        if text_area.widget.text() != &self.contents {
            widgets::TextArea::reset_text(&mut text_area, &self.contents);
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
        let Some(action) = message.take_message::<TextAction>() else {
            tracing::error!(?message, "unexpected message type in InputView::message");
            return MessageResult::Stale;
        };
        match *action {
            TextAction::Changed(text) => MessageResult::Action((self.callback)(app_state, text)),
            // Enter-to-submit is wired up in a later chunk; ignore for now.
            TextAction::Entered(_) => MessageResult::Nop,
        }
    }
}
