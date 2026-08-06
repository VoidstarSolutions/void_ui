//! Form demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{form, form_field};
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::checkbox::checkbox;
use crate::components::date_picker::date_picker;
use crate::components::group_box::group_box;
use crate::components::input::input;
use crate::components::radio::radio;
use crate::components::slider::slider;
use crate::components::toggle::toggle;
use crate::overlay_scope::overlay_scope;
use crate::{label, scroll_container, separator, with_source};

#[derive(Debug, Clone)]
struct FormDemo {
    name: String,
    full_name: String,
    email: String,
    subscribe: bool,
    notify: bool,
    nickname: String,
    bio: String,
    theme_choice: Option<usize>,
    volume: f64,
    start_date: Option<chrono::NaiveDate>,
}

impl FormDemo {
    fn new() -> Self {
        Self {
            name: String::new(),
            full_name: String::new(),
            email: String::new(),
            subscribe: false,
            notify: true,
            nickname: String::new(),
            bio: String::new(),
            theme_choice: Some(0),
            volume: 60.0,
            start_date: None,
        }
    }
}

type InnerView = Box<AnyWidgetView<FormDemo>>;
type InnerViewState = <InnerView as View<FormDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the form demo panel.
pub struct FormDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct FormDemoPanelState {
    demo: FormDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the `Form` demo panel. The returned view owns all interactive
/// state internally; callers do not lens into any form state.
#[must_use]
pub fn panel(theme: &Theme) -> FormDemoPanel {
    FormDemoPanel { theme: *theme }
}

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

/// Vertical form — labels stacked above controls, one required, one hinted.
fn vertical_section(theme: &Theme, state: &FormDemo) -> Box<AnyWidgetView<FormDemo>> {
    Box::new(with_source!(theme, {
        form(vec![
            form_field(
                "Name",
                input(state.name.clone(), |s: &mut FormDemo, t| s.name = t).render(theme),
            )
            .required(true),
            form_field(
                "Email",
                input(state.email.clone(), |s: &mut FormDemo, t| s.email = t).render(theme),
            )
            .hint("We'll never share it.")
            .validate(state.email.as_str(), |v: &str| {
                (!v.is_empty() && !v.contains('@')).then(|| "Enter a valid email address.".into())
            }),
            form_field(
                "Subscribe to the newsletter",
                checkbox(state.subscribe, |s: &mut FormDemo, c: bool| s.subscribe = c)
                    .render(theme),
            ),
        ])
        .render(theme)
    }))
}

/// Horizontal form — labels beside controls in a fixed-width column.
fn horizontal_section(theme: &Theme, state: &FormDemo) -> Box<AnyWidgetView<FormDemo>> {
    Box::new(with_source!(theme, {
        form(vec![
            form_field(
                "Full name",
                input(state.full_name.clone(), |s: &mut FormDemo, t| {
                    s.full_name = t;
                })
                .render(theme),
            )
            .required(true),
            form_field(
                "Notifications",
                toggle(state.notify, |s: &mut FormDemo, c: bool| s.notify = c).render(theme),
            )
            .hint("Email me about account activity."),
        ])
        .horizontal()
        .render(theme)
    }))
}

/// Mixed form — a horizontal form where one field overrides to vertical, so
/// per-field orientation is visible in one form.
fn mixed_section(theme: &Theme, state: &FormDemo) -> Box<AnyWidgetView<FormDemo>> {
    Box::new(with_source!(theme, {
        form(vec![
            // Inherits the form's horizontal orientation.
            form_field(
                "Nickname",
                input(state.nickname.clone(), |s: &mut FormDemo, t| s.nickname = t).render(theme),
            ),
            // Overrides to vertical — label above its own control.
            form_field(
                "Bio",
                input(state.bio.clone(), |s: &mut FormDemo, t| s.bio = t).render(theme),
            )
            .vertical(),
        ])
        .horizontal()
        .render(theme)
    }))
}

/// Grouped form — realistic fields wrapped in titled group-boxes, mixing an
/// input, a radio group, a slider, and a date picker as form-field controls.
fn grouped_section(theme: &Theme, state: &FormDemo) -> Box<AnyWidgetView<FormDemo>> {
    Box::new(with_source!(theme, {
        flex_col((
            group_box(
                form(vec![
                    form_field(
                        "Name",
                        input(state.name.clone(), |s: &mut FormDemo, t| s.name = t).render(theme),
                    )
                    .required(true),
                    form_field(
                        "Email",
                        input(state.email.clone(), |s: &mut FormDemo, t| s.email = t).render(theme),
                    )
                    .hint("We'll never share it.")
                    .validate(state.email.as_str(), |v: &str| {
                        (!v.is_empty() && !v.contains('@'))
                            .then(|| "Enter a valid email address.".into())
                    }),
                ])
                .horizontal()
                .render(theme),
            )
            .title("Account")
            .border()
            .render(theme),
            group_box(
                form(vec![
                    form_field(
                        "Theme",
                        flex_col((
                            radio("Light", |s: &mut FormDemo| s.theme_choice = Some(0))
                                .selected(state.theme_choice == Some(0))
                                .render(theme),
                            radio("Dark", |s: &mut FormDemo| s.theme_choice = Some(1))
                                .selected(state.theme_choice == Some(1))
                                .render(theme),
                            radio("System", |s: &mut FormDemo| s.theme_choice = Some(2))
                                .selected(state.theme_choice == Some(2))
                                .render(theme),
                        ))
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .gap(Length::px(f64::from(theme.density.gap))),
                    ),
                    form_field(
                        "Volume",
                        slider(state.volume, |s: &mut FormDemo, v| s.volume = v)
                            .range(0.0, 100.0)
                            .render(theme),
                    )
                    .hint("Notification sound level."),
                    form_field(
                        "Start date",
                        date_picker(state.start_date, |s: &mut FormDemo, d| s.start_date = d)
                            .placeholder("Pick a date")
                            .cleanable(true)
                            .render(theme),
                    ),
                ])
                .render(theme),
            )
            .title("Preferences")
            .border()
            .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(f64::from(theme.density.gap_lg)))
    }))
}

fn build_inner(theme: &Theme, state: &FormDemo) -> impl WidgetView<FormDemo> + use<> {
    let title_block = flex_col((
        label("Form")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Layout container pairing labels with controls.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(4.0));

    // Wrap the panel in its own overlay scope so the date picker's calendar
    // registers with a portal typed to this panel's local `FormDemo` state and
    // paints above the with_source code block, instead of falling back to an
    // in-tree overlay that later siblings obscure. Mirrors the other overlay
    // demo panels (date_picker, autocomplete, dropdown_button, popover).
    let inner = scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            section_header("Vertical", theme),
            vertical_section(theme, state),
            section_header("Horizontal", theme),
            horizontal_section(theme, state),
            section_header("Mixed", theme),
            mixed_section(theme, state),
            section_header("Grouped", theme),
            grouped_section(theme, state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme);

    overlay_scope(inner)
}

impl ViewMarker for FormDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for FormDemoPanel {
    type ViewState = FormDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = FormDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            FormDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut FormDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.demo));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.demo,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut FormDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut FormDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}
