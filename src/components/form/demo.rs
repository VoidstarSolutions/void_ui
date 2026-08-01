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
use crate::components::input::input;
use crate::components::toggle::toggle;
use crate::{label, scroll_container, separator, with_source};

#[derive(Debug, Clone)]
struct FormDemo {
    name: String,
    full_name: String,
    email: String,
    subscribe: bool,
    notify: bool,
}

impl FormDemo {
    fn new() -> Self {
        Self {
            name: String::new(),
            full_name: String::new(),
            email: String::new(),
            subscribe: false,
            notify: true,
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
            .hint("We'll never share it."),
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

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            section_header("Vertical", theme),
            vertical_section(theme, state),
            section_header("Horizontal", theme),
            horizontal_section(theme, state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
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
