//! Xilem view for the form layout component.
//!
//! A layout container pairing a themed [`label`] with a control, and stacking
//! such pairs into a form. There is no custom masonry widget and no view
//! state: [`Form::render`] and [`FormField::render`] compose the existing
//! `label` with xilem's built-in `flex_row`/`flex_col`/`sized_box` and return
//! a type-erased view directly. Presentation only — the required marker is
//! cosmetic and carries no validation.

use masonry::core::ArcStr;
use xilem::masonry::layout::Length;
use xilem::{AnyWidgetView, WidgetView};

// `CrossAxisAlignment`, `flex_col`, `flex_row`, `sized_box`, `crate::Theme`,
// and `crate::label` are needed by `Form::render`/`FormField::render`, added
// in a later task. Re-add them then; importing now would be unused-import
// dead weight until that code exists.

/// Orientation of a field's label relative to its control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FormOrientation {
    /// Label above the control. Default.
    #[default]
    Vertical,
    /// Label beside the control, in a fixed-width column.
    Horizontal,
}

/// One label/control pair. Created with [`form_field`].
#[must_use = "FormField does nothing until added to a form or rendered with .render(&theme)"]
pub struct FormField<State, Action = ()> {
    #[expect(dead_code, reason = "read by FormField::render, added in a later task")]
    label: ArcStr,
    #[expect(dead_code, reason = "read by FormField::render, added in a later task")]
    control: Box<AnyWidgetView<State, Action>>,
    required: bool,
    hint: Option<ArcStr>,
}

/// Pair a label with a control.
///
/// Defaults: not required, no hint. The control is type-erased so a
/// [`Vec<FormField>`] can hold controls of different types.
pub fn form_field<State, Action>(
    label: impl Into<ArcStr>,
    control: impl WidgetView<State, Action> + 'static,
) -> FormField<State, Action>
where
    State: 'static,
    Action: 'static,
{
    FormField {
        label: label.into(),
        control: Box::new(control),
        required: false,
        hint: None,
    }
}

impl<State: 'static, Action: 'static> FormField<State, Action> {
    /// Mark the field required — appends a cosmetic asterisk in
    /// `palette.danger` after the label. No validation is attached.
    pub fn required(mut self, on: bool) -> Self {
        self.required = on;
        self
    }

    /// Add a muted caption under the control.
    pub fn hint(mut self, text: impl Into<ArcStr>) -> Self {
        self.hint = Some(text.into());
        self
    }
}

/// Builder for a form: a vertical stack of [`FormField`]s. Created with
/// [`form`].
#[must_use = "Form does nothing until rendered with .render(&theme)"]
pub struct Form<State, Action = ()> {
    #[expect(dead_code, reason = "read by Form::render, added in a later task")]
    fields: Vec<FormField<State, Action>>,
    orientation: FormOrientation,
    label_width: Option<Length>,
}

/// Stack `fields` into a form.
///
/// Defaults to [`FormOrientation::Vertical`] and a theme-derived label-column
/// width.
pub fn form<State, Action>(fields: Vec<FormField<State, Action>>) -> Form<State, Action> {
    Form {
        fields,
        orientation: FormOrientation::Vertical,
        label_width: None,
    }
}

impl<State: 'static, Action: 'static> Form<State, Action> {
    /// Set the orientation of every field.
    pub fn orientation(mut self, orientation: FormOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Shorthand for [`FormOrientation::Vertical`] (the default).
    pub fn vertical(mut self) -> Self {
        self.orientation = FormOrientation::Vertical;
        self
    }

    /// Shorthand for [`FormOrientation::Horizontal`].
    pub fn horizontal(mut self) -> Self {
        self.orientation = FormOrientation::Horizontal;
        self
    }

    /// Fixed width of the label column. Horizontal orientation only; ignored
    /// when vertical. Defaults to a theme-derived width.
    pub fn label_width(mut self, width: Length) -> Self {
        self.label_width = Some(width);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{FormOrientation, form, form_field};
    use crate::Theme;
    use crate::label;
    use xilem::masonry::layout::Length;

    #[test]
    fn orientation_defaults_to_vertical() {
        assert_eq!(FormOrientation::default(), FormOrientation::Vertical);
    }

    #[test]
    fn form_field_defaults_are_not_required_and_have_no_hint() {
        let theme = Theme::default();
        let f = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme));
        assert!(!f.required);
        assert!(f.hint.is_none());
    }

    #[test]
    fn required_and_hint_setters_apply() {
        let theme = Theme::default();
        let f = form_field::<(), ()>("Email", label("x").render::<(), ()>(&theme))
            .required(true)
            .hint("we won't share it");
        assert!(f.required);
        assert_eq!(f.hint.as_deref(), Some("we won't share it"));
    }

    #[test]
    fn form_builders_set_orientation_and_label_width() {
        let built = form::<(), ()>(vec![])
            .horizontal()
            .label_width(Length::px(120.0));
        assert_eq!(built.orientation, FormOrientation::Horizontal);
        assert!(built.label_width.is_some());

        let reset = form::<(), ()>(vec![]).horizontal().vertical();
        assert_eq!(reset.orientation, FormOrientation::Vertical);
    }
}
