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
use xilem::view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, sized_box};
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
    error: Option<ArcStr>,
    orientation: Option<FormOrientation>,
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
        error: None,
        orientation: None,
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

    /// Set an error message directly. For errors the consumer already holds:
    /// server-side failures, cross-field checks, async validation results.
    ///
    /// Last write wins against [`Self::validate`].
    pub fn error(mut self, msg: impl Into<ArcStr>) -> Self {
        self.error = Some(msg.into());
        self
    }

    /// Run `rule` against `value` and store its result as this field's error
    /// (`Some(message)` = invalid, `None` = valid). Called at build time —
    /// i.e. every rebuild — so validation is live with no stored state.
    ///
    /// `T: ?Sized` so `&str` values work directly. Last write wins against
    /// [`Self::error`].
    pub fn validate<T: ?Sized>(
        mut self,
        value: &T,
        rule: impl FnOnce(&T) -> Option<ArcStr>,
    ) -> Self {
        self.error = rule(value);
        self
    }

    /// Override this field's orientation, ignoring the form's. Unset by
    /// default, in which case the field inherits the form's orientation.
    pub fn orientation(mut self, orientation: FormOrientation) -> Self {
        self.orientation = Some(orientation);
        self
    }

    /// Shorthand for `.orientation(FormOrientation::Horizontal)`.
    pub fn horizontal(mut self) -> Self {
        self.orientation = Some(FormOrientation::Horizontal);
        self
    }

    /// Shorthand for `.orientation(FormOrientation::Vertical)`.
    pub fn vertical(mut self) -> Self {
        self.orientation = Some(FormOrientation::Vertical);
        self
    }

    /// Materialize this field on its own. Uses the field's own orientation if
    /// set with [`Self::orientation`]/[`Self::horizontal`]/[`Self::vertical`],
    /// otherwise Vertical, at a theme-derived label width.
    #[must_use]
    pub fn render(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>> {
        let effective = resolve_orientation(self.orientation, FormOrientation::Vertical);
        render_field(self, effective, default_label_width(theme), theme)
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
    /// Set the form-wide orientation — used by every field that does not
    /// override it with its own [`FormField::orientation`].
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

    /// Fixed width of the label column. Applies to every field that resolves to
    /// [`FormOrientation::Horizontal`] — including a `.horizontal()` override
    /// inside a vertical form; ignored for fields resolving to `Vertical`.
    /// Defaults to a theme-derived width.
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
            .map(|field| {
                let effective = resolve_orientation(field.orientation, orientation);
                render_field(field, effective, width, theme)
            })
            .collect();
        Box::new(
            flex_col(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::px(f64::from(theme.density.gap_lg))),
        )
    }
}

/// Default label-column width for fields resolving to horizontal. A fixed
/// multiple of the base padding; not yet visually tuned against the gallery.
// TODO(#220): confirm or retune this multiple after the human gallery pass.
fn default_label_width(theme: &Theme) -> Length {
    Length::px(f64::from(theme.density.pad) * 10.0)
}

/// Effective orientation for a field: the field's own override if set,
/// otherwise the surrounding form's orientation.
fn resolve_orientation(field: Option<FormOrientation>, form: FormOrientation) -> FormOrientation {
    field.unwrap_or(form)
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
        error,
        orientation: _,
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

    // Caption beneath the control: the error (danger) takes precedence over the
    // hint (muted). At most one shows; a valid field with a hint shows the hint.
    let caption: Option<Box<AnyWidgetView<State, Action>>> = match (error, hint) {
        (Some(err), _) => Some(Box::new(
            label(err)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.danger)
                .multiline(true)
                .render(theme),
        )),
        (None, Some(hint_text)) => Some(Box::new(
            label(hint_text)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_muted)
                .multiline(true)
                .render(theme),
        )),
        (None, None) => None,
    };
    // Always wrap the control in the same `flex_col`, with the caption as an
    // *optional sibling* (`None` = zero children, `Some` = one), rather than
    // switching `control_cell` between a bare control and a wrapping `flex_col`.
    // That switch changes the concrete view type at this slot, and xilem's
    // rebuild reacts to a type change by tearing down and recreating the whole
    // subtree — including the focused `TextInput`, which loses focus on the
    // first keystroke that toggles a validation error. Keeping the wrapper
    // unconditional holds the control's identity stable across valid/invalid
    // rebuilds. The gap only materializes when a caption is actually present.
    let control_cell: Box<AnyWidgetView<State, Action>> = Box::new(
        flex_col((control, caption))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(f64::from(theme.density.gap))),
    );

    match orientation {
        FormOrientation::Vertical => Box::new(
            flex_col((label_row, control_cell))
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::px(f64::from(theme.density.pad))),
        ),
        FormOrientation::Horizontal => Box::new(
            // Control fills the row's remaining width beside the fixed label
            // column, so a text input isn't pinned to its tiny intrinsic size.
            flex_row((
                sized_box(label_row).fixed_width(label_width),
                control_cell.flex(1.0),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(f64::from(theme.density.gap_lg))),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{FormOrientation, form, form_field, resolve_orientation};
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
        assert!(f.error.is_none());
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
    fn error_setter_sets_message() {
        let theme = Theme::default();
        let f = form_field::<(), ()>("Email", label("x").render::<(), ()>(&theme)).error("boom");
        assert_eq!(f.error.as_deref(), Some("boom"));
    }

    #[test]
    fn validate_stores_rule_result() {
        let theme = Theme::default();
        let rule = |v: &str| (v.is_empty()).then(|| "required".into());

        let failing =
            form_field::<(), ()>("Email", label("x").render::<(), ()>(&theme)).validate("", rule);
        assert_eq!(failing.error.as_deref(), Some("required"));

        let passing =
            form_field::<(), ()>("Email", label("x").render::<(), ()>(&theme)).validate("ok", rule);
        assert!(passing.error.is_none());
    }

    #[test]
    fn error_and_validate_are_last_write_wins() {
        let theme = Theme::default();
        let clear = |_: &str| None;

        // validate after error clears it
        let a = form_field::<(), ()>("Email", label("x").render::<(), ()>(&theme))
            .error("first")
            .validate("ok", clear);
        assert!(a.error.is_none());

        // error after validate overrides it
        let b = form_field::<(), ()>("Email", label("x").render::<(), ()>(&theme))
            .validate("", |v: &str| (v.is_empty()).then(|| "computed".into()))
            .error("forced");
        assert_eq!(b.error.as_deref(), Some("forced"));
    }

    #[test]
    fn resolve_orientation_matrix() {
        use FormOrientation::{Horizontal, Vertical};
        // Default field inherits the form's orientation.
        assert_eq!(resolve_orientation(None, Vertical), Vertical);
        assert_eq!(resolve_orientation(None, Horizontal), Horizontal);
        // A field override wins over the opposite form orientation.
        assert_eq!(resolve_orientation(Some(Horizontal), Vertical), Horizontal);
        assert_eq!(resolve_orientation(Some(Vertical), Horizontal), Vertical);
        // Standalone default (form defaulted to Vertical, no override).
        assert_eq!(resolve_orientation(None, Vertical), Vertical);
    }

    #[test]
    fn form_field_orientation_defaults_to_none() {
        let theme = Theme::default();
        let f = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme));
        assert!(f.orientation.is_none());
    }

    #[test]
    fn form_field_orientation_setters_apply() {
        let theme = Theme::default();
        let h = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme)).horizontal();
        assert_eq!(h.orientation, Some(FormOrientation::Horizontal));
        let v = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme)).vertical();
        assert_eq!(v.orientation, Some(FormOrientation::Vertical));
        let o = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme))
            .orientation(FormOrientation::Horizontal);
        assert_eq!(o.orientation, Some(FormOrientation::Horizontal));
    }

    #[test]
    fn mixed_orientation_forms_and_standalone_override_build() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = ();

        // Vertical form: one field overrides to horizontal, one inherits.
        let _ = form::<(), ()>(vec![
            form_field("Name", label("x").render::<(), ()>(&theme)).horizontal(),
            form_field("Email", label("y").render::<(), ()>(&theme)),
        ])
        .render(&theme)
        .build(&mut ctx, &mut state);

        // Horizontal form: one field overrides to vertical, one inherits.
        let _ = form::<(), ()>(vec![
            form_field("Name", label("x").render::<(), ()>(&theme)).vertical(),
            form_field("Email", label("y").render::<(), ()>(&theme)),
        ])
        .horizontal()
        .render(&theme)
        .build(&mut ctx, &mut state);

        // Standalone field honoring its own override.
        let _ = form_field::<(), ()>("Name", label("x").render::<(), ()>(&theme))
            .horizontal()
            .render(&theme)
            .build(&mut ctx, &mut state);
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

    #[test]
    fn errored_fields_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = ();

        // Vertical: an errored field, a field carrying BOTH hint and error
        // (exercises the error-beats-hint precedence branch), and a clean
        // field with neither hint nor error (mixed errored/clean form).
        let _ = form::<(), ()>(vec![
            form_field("Name", label("x").render::<(), ()>(&theme)).error("required"),
            form_field("Email", label("y").render::<(), ()>(&theme))
                .hint("optional")
                .error("bad email"),
            form_field("Phone", label("z").render::<(), ()>(&theme)),
        ])
        .render(&theme)
        .build(&mut ctx, &mut state);

        // Horizontal: same coverage in the other orientation.
        let _ = form::<(), ()>(vec![
            form_field("Name", label("x").render::<(), ()>(&theme)).error("required"),
            form_field("Email", label("y").render::<(), ()>(&theme))
                .hint("optional")
                .error("bad email"),
        ])
        .horizontal()
        .render(&theme)
        .build(&mut ctx, &mut state);
    }
}
