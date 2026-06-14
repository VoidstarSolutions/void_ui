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
use xilem::view::sized_box;
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{InputCleared, InputFrame};
use crate::Theme;
use crate::label;

/// Compose a field's baseline-aligned content row — prefix affix, the
/// flex-growing editor, then suffix affix — with the theme's column gap. Each
/// slot is a flex child: `Some(view)` / `None` for an optional affix, or `()`
/// for none (masked).
///
/// A macro rather than a `fn`: xilem's generic flex bounds make a generic helper
/// impractical (`Flex<Seq>: Style` needs a concrete sequence), so this expands
/// at each call site — keeping the baseline-alignment decision in one place.
macro_rules! affixed_row {
    ($prefix:expr, $core:expr, $suffix:expr, $theme:expr $(,)?) => {{
        use ::xilem::style::Style as _;
        use ::xilem::view::FlexExt as _;
        ::xilem::view::flex_row(($prefix, $core.flex(1.0), $suffix))
            .cross_axis_alignment(::xilem::view::CrossAxisAlignment::FirstBaseline)
            .gap(::masonry::layout::Length::px(f64::from($theme.density.col)))
    }};
}
pub(crate) use affixed_row;

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
        let prefix = self.prefix.map(|text| affix_label(text, theme));
        let suffix = self.suffix.map(|text| affix_label(text, theme));

        let core = InputView::new(
            self.contents,
            self.placeholder,
            self.disabled,
            theme,
            self.callback,
        );

        field_chrome(affixed_row!(prefix, core, suffix, theme), theme)
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

/// A muted, non-interactive affix label (prefix/suffix, currency symbol). Shared
/// so every flavor styles its affixes identically.
pub(crate) fn affix_label<State, Action>(
    text: ArcStr,
    theme: &Theme,
) -> impl WidgetView<State, Action> + use<State, Action>
where
    State: 'static,
    Action: 'static,
{
    label(text).color(theme.palette.text_muted).render(theme)
}

/// Infer where the caret sits in `current` after the single contiguous edit that
/// turned `prev` into it: the end of an insertion, or the point of a deletion.
/// (We can't read the real caret from masonry, so we reconstruct it.) Returns a
/// char index.
fn caret_after_edit(prev: &str, current: &str) -> usize {
    let prev: Vec<char> = prev.chars().collect();
    let curr: Vec<char> = current.chars().collect();
    let mut prefix = 0;
    while prefix < prev.len() && prefix < curr.len() && prev[prefix] == curr[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < prev.len() - prefix
        && suffix < curr.len() - prefix
        && prev[prev.len() - 1 - suffix] == curr[curr.len() - 1 - suffix]
    {
        suffix += 1;
    }
    curr.len() - suffix
}

/// Byte offset in `text` just past its `n`-th digit (or the end if there are
/// fewer than `n`). The digit count is the stable anchor across reformatting:
/// grouping separators and mask literals move, but the digits keep their order.
fn byte_pos_after_n_digits(text: &str, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut seen = 0;
    for (i, ch) in text.char_indices() {
        if ch.is_ascii_digit() {
            seen += 1;
            if seen == n {
                return i + ch.len_utf8();
            }
        }
    }
    text.len()
}

/// The materialized [`View`] backing the editor core of an [`Input`].
///
/// Built only through [`Input::render`] / the number input; not constructed
/// directly by callers, and not part of the public API (both builders return an
/// opaque `impl WidgetView`). Carries no chrome of its own — the surrounding
/// `sized_box` does.
#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct InputView<F, State, Action> {
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
    /// `true` once the placeholder font-size override has been applied (see
    /// `rebuild`). masonry builds the placeholder Label at its default size and
    /// gives no build-time style hook, so we correct it on the first rebuild
    /// rather than every rebuild.
    type ViewState = bool;

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
        (pod, false)
    }

    fn rebuild(
        &self,
        prev: &Self,
        placeholder_sized: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let theme_changed = self.theme != prev.theme;

        // The element is the frame; reach the TextInput it hosts.
        let mut child = InputFrame::child_mut(&mut element);
        let mut text_input = child.downcast::<widgets::TextInput>();

        // Match the placeholder's font size to the editor's. masonry builds the
        // placeholder Label at its default size (15px) while our body text is
        // 13px, so left alone the placeholder renders a couple of pixels low.
        // `with_placeholder` exposes no build-time style hook, so we apply the
        // override on the first rebuild (and on theme changes). It can't live in
        // `build`; the trade-off is that the very first painted frame uses
        // masonry's default until this runs. `insert_style` always invalidates,
        // so we gate it rather than re-asserting it on every keystroke.
        if !*placeholder_sized || theme_changed {
            let mut placeholder = widgets::TextInput::placeholder_mut(&mut text_input);
            widgets::Label::insert_style(
                &mut placeholder,
                StyleProperty::FontSize(self.theme.typography.size_body),
            );
            *placeholder_sized = true;
        }

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
        // Reformatting fields (currency/mask) rebuild `contents` into a
        // different string than the user's just-typed text, so the text must be
        // replaced — but `reset_text` slams the caret to the end, breaking
        // mid-string editing. masonry exposes no caret *getter*, so we infer the
        // edit position by diffing the previous display against the current
        // text, anchor it by digit offset (separators move, digits don't), and
        // restore it with `select_byte_range` after the reset.
        let current = text_area.widget.text().to_string();
        if current != self.contents {
            let caret = caret_after_edit(&prev.contents, &current);
            let digits_before = current
                .chars()
                .take(caret)
                .filter(char::is_ascii_digit)
                .count();
            widgets::TextArea::reset_text(&mut text_area, &self.contents);
            let pos = byte_pos_after_n_digits(&self.contents, digits_before);
            widgets::TextArea::select_byte_range(&mut text_area, pos, pos);
        }
    }

    fn teardown(
        &self,
        _: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        // `build` records two routing sources: the inner TextArea and the
        // frame. Remove both — tearing down only the frame would leak the
        // TextArea's entry in the action-source map across mount/unmount.
        {
            let mut child = InputFrame::child_mut(&mut element);
            let mut text_input = child.downcast::<widgets::TextInput>();
            let text_area = widgets::TextInput::text_mut(&mut text_input);
            ctx.teardown_action_source(text_area);
        }
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        _: &mut Self::ViewState,
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

#[cfg(test)]
mod tests {
    use masonry::core::ArcStr;
    use masonry::testing::TestHarness;
    use masonry::widgets::TextAction;
    use xilem::ViewCtx;
    use xilem::core::{DynMessage, Environment, MessageCtx, MessageResult, View};

    use crate::test_support;
    use super::super::widget::InputCleared;
    use super::{InputView, byte_pos_after_n_digits, caret_after_edit};
    use crate::Theme;

    #[test]
    fn caret_after_insert_delete_append() {
        // insert "9" into "1,2|34" -> "1,2934": caret after the inserted 9.
        assert_eq!(caret_after_edit("1,234", "1,2934"), 4);
        // append at the end -> caret at the end.
        assert_eq!(caret_after_edit("1,234", "1,2345"), 6);
        // backspace "1,23|4" -> "1,24": caret at the deletion point.
        assert_eq!(caret_after_edit("1,234", "1,24"), 3);
    }

    #[test]
    fn digit_anchor_survives_regrouping() {
        // 3 digits before the caret -> just past the 3rd digit in the new text.
        assert_eq!(byte_pos_after_n_digits("12,934", 3), 4);
        assert_eq!(byte_pos_after_n_digits("12,934", 0), 0);
        // Fewer digits than asked -> clamp to the end.
        assert_eq!(byte_pos_after_n_digits("12,934", 9), 6);
    }

    fn build_view_and_state() -> (ViewCtx, String) {
        let ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        (ctx, "hello".to_string())
    }

    #[test]
    fn input_cleared_emits_empty_string_to_callback() {
        let (mut ctx, mut state) = build_view_and_state();
        let theme = Theme::default();
        let view = InputView::new(
            state.clone(),
            ArcStr::default(),
            false,
            &theme,
            |state: &mut String, text: String| *state = text,
        );

        let (pod, mut view_state) = View::<String, (), ViewCtx>::build(&view, &mut ctx, &mut state);
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), pod.new_widget);

        harness.edit_root_widget(|element| {
            let mut message =
                MessageCtx::new(Environment::new(), vec![], DynMessage::new(InputCleared));
            let result = View::<String, (), ViewCtx>::message(
                &view,
                &mut view_state,
                &mut message,
                element,
                &mut state,
            );
            assert!(matches!(result, MessageResult::Action(())));
        });

        assert_eq!(state, "");
    }

    #[test]
    fn changed_action_passes_new_text_to_callback() {
        let (mut ctx, mut state) = build_view_and_state();
        let theme = Theme::default();
        let view = InputView::new(
            state.clone(),
            ArcStr::default(),
            false,
            &theme,
            |state: &mut String, text: String| *state = text,
        );

        let (pod, mut view_state) = View::<String, (), ViewCtx>::build(&view, &mut ctx, &mut state);
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), pod.new_widget);

        harness.edit_root_widget(|element| {
            let mut message = MessageCtx::new(
                Environment::new(),
                vec![],
                DynMessage::new(TextAction::Changed("world".to_string())),
            );
            let result = View::<String, (), ViewCtx>::message(
                &view,
                &mut view_state,
                &mut message,
                element,
                &mut state,
            );
            assert!(matches!(result, MessageResult::Action(())));
        });

        assert_eq!(state, "world");
    }
}
