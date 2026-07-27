//! Internal xilem view constructing `masonry::widgets::Label` directly.
//!
//! Exists because `xilem`'s own `label()` convenience view (`xilem_masonry
//! ::view::label::Label`) has no field for parley's `Underline`/
//! `Strikethrough` `StyleProperty` and no generic escape hatch to inject
//! one — see the design doc at
//! `docs/superpowers/specs/2026-07-27-label-text-decoration-design.md`.
//! Mirrors that upstream view's `build`/`rebuild` field-for-field, adding
//! only the two decoration properties.

use masonry::core::{ArcStr, StyleProperty};
use masonry::parley::style::FontWeight;
use masonry::parley::{FontFamily, FontFamilyName, GenericFamily, LineHeight};
use masonry::widgets;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, TextAlign, ViewCtx};

use crate::TextDecoration;

pub(super) fn styled_label(label: impl Into<ArcStr>, decoration: TextDecoration) -> StyledLabel {
    StyledLabel {
        label: label.into(),
        text_alignment: TextAlign::default(),
        text_size: masonry::theme::TEXT_SIZE_NORMAL,
        letter_spacing: 0.0,
        font: FontFamily::Single(FontFamilyName::Generic(GenericFamily::SystemUi)),
        line_height: LineHeight::default(),
        decoration,
    }
}

pub(super) struct StyledLabel {
    label: ArcStr,
    text_alignment: TextAlign,
    text_size: f32,
    letter_spacing: f32,
    font: FontFamily<'static>,
    line_height: LineHeight,
    decoration: TextDecoration,
}

impl StyledLabel {
    pub(super) fn text_alignment(mut self, text_alignment: TextAlign) -> Self {
        self.text_alignment = text_alignment;
        self
    }

    pub(super) fn text_size(mut self, text_size: f32) -> Self {
        self.text_size = text_size;
        self
    }

    pub(super) fn letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = letter_spacing;
        self
    }

    pub(super) fn font(mut self, font: FontFamily<'static>) -> Self {
        self.font = font;
        self
    }

    pub(super) fn line_height(mut self, line_height: LineHeight) -> Self {
        self.line_height = line_height;
        self
    }
}

impl ViewMarker for StyledLabel {}

impl<State: 'static, Action> View<State, Action, ViewCtx> for StyledLabel {
    type Element = Pod<widgets::Label>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = widgets::Label::new(self.label.clone())
            .with_text_alignment(self.text_alignment)
            .with_style(StyleProperty::FontSize(self.text_size))
            .with_style(StyleProperty::FontWeight(FontWeight::NORMAL))
            .with_style(StyleProperty::LineHeight(self.line_height))
            .with_style(StyleProperty::FontFamily(self.font.clone()))
            .with_style(StyleProperty::WordSpacing(0.0))
            .with_style(StyleProperty::LetterSpacing(self.letter_spacing))
            .with_style(StyleProperty::Underline(matches!(
                self.decoration,
                TextDecoration::Underline
            )))
            .with_style(StyleProperty::Strikethrough(matches!(
                self.decoration,
                TextDecoration::Strikethrough
            )))
            .with_hint(true);
        (ctx.create_pod(widget), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if prev.label != self.label {
            widgets::Label::set_text(&mut element, self.label.clone());
        }
        if prev.text_alignment != self.text_alignment {
            widgets::Label::set_text_alignment(&mut element, self.text_alignment);
        }
        #[expect(
            clippy::float_cmp,
            reason = "skip the style insert only when the value is bit-identical; epsilon comparison would silently swallow caller-intended updates"
        )]
        if prev.text_size != self.text_size {
            widgets::Label::insert_style(&mut element, StyleProperty::FontSize(self.text_size));
        }
        if prev.line_height != self.line_height {
            widgets::Label::insert_style(&mut element, StyleProperty::LineHeight(self.line_height));
        }
        #[expect(
            clippy::float_cmp,
            reason = "skip the style insert only when the value is bit-identical; epsilon comparison would silently swallow caller-intended updates"
        )]
        if prev.letter_spacing != self.letter_spacing {
            widgets::Label::insert_style(
                &mut element,
                StyleProperty::LetterSpacing(self.letter_spacing),
            );
        }
        if prev.font != self.font {
            widgets::Label::insert_style(
                &mut element,
                StyleProperty::FontFamily(self.font.clone()),
            );
        }
        if prev.decoration != self.decoration {
            widgets::Label::insert_style(
                &mut element,
                StyleProperty::Underline(matches!(self.decoration, TextDecoration::Underline)),
            );
            widgets::Label::insert_style(
                &mut element,
                StyleProperty::Strikethrough(matches!(
                    self.decoration,
                    TextDecoration::Strikethrough
                )),
            );
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        tracing::error!(
            ?message,
            "Message arrived in StyledLabel::message, but StyledLabel doesn't consume any messages, this is a bug"
        );
        MessageResult::Stale
    }
}

#[cfg(test)]
mod tests {
    use masonry::testing::TestHarness;
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::styled_label;
    use crate::TextDecoration;
    use crate::test_support;

    struct AppState;

    #[test]
    fn build_and_rebuild_across_decorations_do_not_panic() {
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let none = styled_label("hello", TextDecoration::None);
        let (pod, mut view_state) =
            View::<AppState, (), ViewCtx>::build(&none, &mut ctx, &mut state);
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), pod.new_widget);

        let underline = styled_label("hello", TextDecoration::Underline);
        harness.edit_root_widget(|element| {
            View::<AppState, (), ViewCtx>::rebuild(
                &underline,
                &none,
                &mut view_state,
                &mut ctx,
                element,
                &mut state,
            );
        });

        let strikethrough = styled_label("hello", TextDecoration::Strikethrough);
        harness.edit_root_widget(|element| {
            View::<AppState, (), ViewCtx>::rebuild(
                &strikethrough,
                &underline,
                &mut view_state,
                &mut ctx,
                element,
                &mut state,
            );
        });
    }
}
