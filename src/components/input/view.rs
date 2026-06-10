//! Xilem view for the themed single-line text input.
//!
//! The public [`input`] builder composes three dogfooded views: a themed
//! [`sized_box`](xilem::view::sized_box) that draws the field chrome (border,
//! background, corner radius, padding), a [`flex_row`](xilem::view::flex_row)
//! holding the optional prefix/suffix affixes and the editor, and the
//! [`InputView`] core. The core builds `masonry::widgets::TextInput` directly
//! (reusing its `TextArea` child for caret/selection/IME) inside an
//! [`InputFrame`], which is the masonry seam that adds Esc-to-clear; the
//! `TextInput`'s own chrome is stripped to transparent so only the surrounding
//! `sized_box` paints it.
//!
//! ```ignore
//! use void_ui::components::input::input;
//! input(state.amount.clone(), |s: &mut State, text| s.amount = text)
//!     .prefix("$")
//!     .suffix("USD")
//!     .placeholder("0.00")
//!     .render(&theme)
//! ```
//!
//! The contents are host-controlled: the field emits the new string on every
//! edit via the change callback, and the host stores it and passes it back in
//! on the next render. This mirrors every other interactive void-ui component.

use std::marker::PhantomData;

use masonry::core::{ArcStr, NewWidget, PropertySet};
use masonry::layout::Length;
use masonry::parley::StyleProperty;
use masonry::peniko::Color;
use masonry::properties::{
    Background, BorderWidth, CaretColor, ContentColor, Padding, PlaceholderColor, SelectionColor,
};
use masonry::widgets::{self, TextAction};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, FlexExt as _, flex_row, sized_box};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{InputCleared, InputFrame};
use crate::Theme;
use crate::label;

/// Hairline border around the field. Component-local like every other bordered
/// widget (tooltip, checkbox, code view); a 1px stroke doesn't scale with
/// density. Inner padding, by contrast, is read from `Theme.density`.
const BORDER_WIDTH: Length = Length::const_px(1.0);

/// Fully transparent fill, used to suppress the inner `TextInput`'s default
/// masonry chrome so only the surrounding `sized_box` paints the field.
const TRANSPARENT: Color = Color::from_rgba8(0, 0, 0, 0);

/// Builder for a themed single-line text input.
///
/// Created with [`input`]. Returns a xilem view via [`Self::render`].
#[must_use = "Input does nothing until rendered with .render(&theme)"]
pub struct Input<F> {
    contents: String,
    placeholder: ArcStr,
    disabled: bool,
    prefix: Option<ArcStr>,
    suffix: Option<ArcStr>,
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
        prefix: None,
        suffix: None,
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

    /// Set a leading affix shown inside the border, before the editor (e.g.
    /// `$`, `https://`). Decorative and non-interactive.
    pub fn prefix(mut self, text: impl Into<ArcStr>) -> Self {
        self.prefix = Some(text.into());
        self
    }

    /// Set a trailing affix shown inside the border, after the editor (e.g.
    /// `USD`, `.00`). Decorative and non-interactive.
    pub fn suffix(mut self, text: impl Into<ArcStr>) -> Self {
        self.suffix = Some(text.into());
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    {
        // Affixes are muted labels sharing the field background. Absent slots
        // become `None`, which renders nothing (no stray gap).
        let prefix = self
            .prefix
            .map(|text| label(text).color(theme.palette.text_muted).render(theme));
        let suffix = self
            .suffix
            .map(|text| label(text).color(theme.palette.text_muted).render(theme));

        let core = InputView::new(
            self.contents,
            self.placeholder,
            self.disabled,
            theme,
            self.callback,
        );

        // Editor takes the remaining width; affixes hug the ends.
        let row = flex_row((prefix, core.flex(1.0), suffix))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.col)));

        field_chrome(row, theme)
    }
}

/// Inner padding between the field border and its content, from the theme's
/// button density so the field lines up with buttons of the same theme.
fn field_padding(theme: &Theme) -> Padding {
    Padding::from_vh(
        Length::px(f64::from(theme.density.button_pad_v)),
        Length::px(f64::from(theme.density.button_pad_h)),
    )
}

/// Wrap a field's content row in the themed chrome: background, border, corner
/// radius, and inner padding. Shared by [`Input`] and the number input so every
/// flavor of field draws an identical box.
pub(crate) fn field_chrome<State, Action, V>(
    content: V,
    theme: &Theme,
) -> impl WidgetView<State, Action> + use<State, Action, V>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    sized_box(content)
        .background_color(theme.palette.surface)
        .border(theme.palette.border, BORDER_WIDTH)
        .corner_radius(Length::px(f64::from(theme.radius.small)))
        .padding(field_padding(theme))
}

/// The materialized [`View`] backing the editor core of an [`Input`].
///
/// Built only through [`Input::render`]; not constructed directly by callers.
/// Carries no chrome of its own — the surrounding `sized_box` does.
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
    /// Construct the chrome-less editor core. Used by [`Input`] and the number
    /// input, which supply their own surrounding affixes/steppers and chrome.
    pub(crate) fn new(
        contents: String,
        placeholder: ArcStr,
        disabled: bool,
        theme: &Theme,
        callback: F,
    ) -> Self {
        Self {
            contents,
            placeholder,
            disabled,
            theme: *theme,
            callback,
            phantom: PhantomData,
        }
    }

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
}

impl<F, State, Action> ViewMarker for InputView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for InputView<F, State, Action>
where
    F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    State: 'static,
    Action: 'static,
{
    type Element = Pod<InputFrame>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let text_area = widgets::TextArea::new_editable(&self.contents)
            .with_style(StyleProperty::FontSize(self.theme.typography.size_body));

        let text_input = widgets::TextInput::from_text_area(
            NewWidget::new(text_area).with_props(self.area_props()),
        )
        .with_placeholder(self.placeholder.clone())
        .with_clip(true);

        // The inner TextArea emits the text edits; capture its id before the
        // TextInput is moved into the frame.
        let area_id = text_input.area_pod().id();

        // Strip the masonry default chrome (border/background/padding): the
        // surrounding sized_box paints the field. Keep only the themed
        // placeholder color and the disabled flag.
        let mut input = NewWidget::new(text_input);
        input.properties.insert(Background::Color(TRANSPARENT));
        input
            .properties
            .insert(BorderWidth::all(Length::const_px(0.0)));
        input.properties.insert(Padding::all(Length::const_px(0.0)));
        input
            .properties
            .insert(PlaceholderColor::new(self.theme.palette.text_muted));
        input.options.disabled = self.disabled;

        let pod = ctx.create_pod(InputFrame::new(input));
        // Route both sources to this view: the frame (Escape -> InputCleared)
        // and the inner TextArea (edits -> TextAction).
        ctx.record_action_source(pod.new_widget.id());
        ctx.record_action_source(area_id);
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

        // The element is the frame; reach the TextInput it hosts.
        let mut child = InputFrame::child_mut(&mut element);
        let mut text_input = child.downcast::<widgets::TextInput>();

        if theme_changed {
            text_input.insert_prop(PlaceholderColor::new(self.theme.palette.text_muted));
        }
        if self.placeholder != prev.placeholder {
            widgets::TextInput::set_placeholder(&mut text_input, self.placeholder.clone());
        }
        if self.disabled != prev.disabled {
            text_input.ctx.set_disabled(self.disabled);
        }

        let mut text_area = widgets::TextInput::text_mut(&mut text_input);
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
        if let Some(action) = message.take_message::<TextAction>() {
            return match *action {
                TextAction::Changed(text) => {
                    MessageResult::Action((self.callback)(app_state, text))
                }
                // Enter-to-submit is wired up in a later chunk; ignore for now.
                TextAction::Entered(_) => MessageResult::Nop,
            };
        }
        // Escape in the focused field clears it: emit an empty-string change so
        // the host updates its own state.
        if message.take_message::<InputCleared>().is_some() {
            return MessageResult::Action((self.callback)(app_state, String::new()));
        }
        tracing::error!(?message, "unexpected message type in InputView::message");
        MessageResult::Stale
    }
}
