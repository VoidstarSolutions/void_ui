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
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::label;

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
    label: ArcStr,
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

    /// Materialize this field on its own, always vertical, at a theme-derived
    /// label width.
    #[must_use]
    pub fn render(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>> {
        render_field(
            self,
            FormOrientation::Vertical,
            default_label_width(theme),
            theme,
        )
    }
}

/// Builder for a form: a vertical stack of [`FormField`]s. Created with
/// [`form`].
#[must_use = "Form does nothing until rendered with .render(&theme)"]
pub struct Form<State, Action = ()> {
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

    /// Materialize the form at the supplied theme.
    #[must_use]
    pub fn render(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>> {
        let width = self
            .label_width
            .unwrap_or_else(|| default_label_width(theme));
        let orientation = self.orientation;
        let rows: Vec<Box<AnyWidgetView<State, Action>>> = self
            .fields
            .into_iter()
            .map(|field| render_field(field, orientation, width, theme))
            .collect();
        Box::new(
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::px(f64::from(theme.density.gap_lg))),
        )
    }
}

/// Default label-column width for horizontal forms. A fixed multiple of the
/// base padding; not yet visually tuned against the gallery.
// TODO(#220): confirm or retune this multiple after the human gallery pass.
fn default_label_width(theme: &Theme) -> Length {
    Length::px(f64::from(theme.density.pad) * 10.0)
}

/// Render one field into a vertical or horizontal label/control layout.
fn render_field<State: 'static, Action: 'static>(
    field: FormField<State, Action>,
    orientation: FormOrientation,
    label_width: Length,
    theme: &Theme,
) -> Box<AnyWidgetView<State, Action>> {
    let FormField {
        label: text,
        control,
        required,
        hint,
    } = field;

    // Label, plus a cosmetic danger asterisk when required.
    let label_view = label(text).render(theme);
    let label_row: Box<AnyWidgetView<State, Action>> = if required {
        let asterisk = label("*").color(theme.palette.danger).render(theme);
        Box::new(
            flex_row((label_view, asterisk))
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .gap(Length::px(f64::from(theme.density.gap))),
        )
    } else {
        Box::new(label_view)
    };

    // Control, plus a muted hint caption beneath it when set.
    let control_cell: Box<AnyWidgetView<State, Action>> = match hint {
        Some(hint_text) => {
            let hint_view = label(hint_text)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_muted)
                .multiline(true)
                .render(theme);
            Box::new(
                flex_col((control, hint_view))
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(Length::px(f64::from(theme.density.gap))),
            )
        }
        None => control,
    };

    match orientation {
        FormOrientation::Vertical => Box::new(
            flex_col((label_row, control_cell))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::px(f64::from(theme.density.pad))),
        ),
        FormOrientation::Horizontal => Box::new(
            flex_row((sized_box(label_row).fixed_width(label_width), control_cell))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Length::px(f64::from(theme.density.gap_lg))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{FormOrientation, form, form_field};
    use crate::Theme;
    use crate::label;
    use crate::test_support;
    use xilem::ViewCtx;
    use xilem::core::View;
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

    #[test]
    fn fields_and_forms_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = ();

        // Standalone field.
        let _ = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme))
            .render(&theme)
            .build(&mut ctx, &mut state);

        // Vertical form: a required field and a field with a hint.
        let _ = form::<(), ()>(vec![
            form_field("Name", label("x").render::<(), ()>(&theme)).required(true),
            form_field("Email", label("y").render::<(), ()>(&theme)).hint("optional"),
        ])
        .render(&theme)
        .build(&mut ctx, &mut state);

        // Horizontal form: required + hint on one field, explicit label width.
        let _ = form::<(), ()>(vec![
            form_field("Name", label("x").render::<(), ()>(&theme)),
            form_field("Bio", label("y").render::<(), ()>(&theme))
                .hint("about you")
                .required(true),
        ])
        .horizontal()
        .label_width(Length::px(100.0))
        .render(&theme)
        .build(&mut ctx, &mut state);
    }
}
