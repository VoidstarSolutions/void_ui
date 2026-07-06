//! Breadcrumb demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{breadcrumb, segment};
use crate::components::ScrollBarVisibility;
use crate::{ButtonVariant, Theme, button, label, scroll_container, separator, with_source};

// --- MARK: LOCAL STATE

#[derive(Clone, Copy, PartialEq, Eq)]
enum Location {
    Home,
    TradeDashboard,
    Settings,
    SettingsQqq,
}

struct BreadcrumbDemo {
    location: Location,
}

impl BreadcrumbDemo {
    fn new() -> Self {
        Self {
            location: Location::TradeDashboard,
        }
    }
}

type InnerView = Box<AnyWidgetView<BreadcrumbDemo>>;
type InnerViewState = <InnerView as View<BreadcrumbDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL VIEW STATE

pub struct BreadcrumbDemoPanelState {
    demo: BreadcrumbDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

pub struct BreadcrumbDemoPanel {
    theme: Theme,
}

// --- MARK: TRAIL BUILDER

/// The live trail for the current `location` — clicking an ancestor segment
/// navigates up by setting `location` to that ancestor.
fn trail(theme: &Theme, location: Location) -> Box<AnyWidgetView<BreadcrumbDemo>> {
    let home = || {
        segment("Trade Dashboard").on_select(|s: &mut BreadcrumbDemo| s.location = Location::Home)
    };
    Box::new(match location {
        Location::Home => breadcrumb()
            .segment(segment("Trade Dashboard"))
            .render(theme),
        Location::TradeDashboard => breadcrumb()
            .segment(home())
            .segment(segment("Trade dashboard"))
            .render(theme),
        Location::Settings => breadcrumb()
            .segment(home())
            .segment(segment("Settings"))
            .render(theme),
        Location::SettingsQqq => breadcrumb()
            .segment(home())
            .segment(
                segment("Settings")
                    .on_select(|s: &mut BreadcrumbDemo| s.location = Location::Settings),
            )
            .segment(segment("QQQ"))
            .render(theme),
    })
}

/// Buttons to jump straight to a page, standing in for whatever in-app
/// navigation (sidebar, search) would normally set the current location.
fn jump_controls(theme: &Theme) -> impl WidgetView<BreadcrumbDemo> + use<> {
    let jump = |label_text: &'static str, to: Location| {
        button(move |s: &mut BreadcrumbDemo| s.location = to)
            .label(label_text)
            .variant(ButtonVariant::Ghost)
            .render(theme)
    };
    flex_row((
        jump("Trade dashboard", Location::TradeDashboard),
        jump("Settings", Location::Settings),
        jump("Settings · QQQ", Location::SettingsQqq),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0))
}

// --- MARK: INNER VIEW BUILDER

fn build_inner<'a>(
    theme: &'a Theme,
    state: &'a BreadcrumbDemo,
) -> impl WidgetView<BreadcrumbDemo> + use<'a> {
    let title_block = flex_col((
        label("Breadcrumb")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "An ordered trail of segments joined by a themed chevron. A segment with \
             an on_select callback is an interactive ancestor link; one without renders \
             as the plain current-location label.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let location = state.location;
    let live_section = with_source!(theme, {
        flex_col((trail(theme, location), jump_controls(theme)))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .gap(Length::px(12.0))
    });

    let static_section = with_source!(theme, {
        breadcrumb()
            .segment(
                segment("Trade Dashboard")
                    .on_select(|s: &mut BreadcrumbDemo| s.location = Location::Home),
            )
            .segment(segment("Trade dashboard"))
            .render(theme)
    });

    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            header("Interactive — click a crumb, or jump to a page"),
            live_section,
            header("Static"),
            static_section,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// --- MARK: VIEW IMPL

impl ViewMarker for BreadcrumbDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for BreadcrumbDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = BreadcrumbDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = BreadcrumbDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            BreadcrumbDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut BreadcrumbDemoPanelState,
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
        vs: &mut BreadcrumbDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut BreadcrumbDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}

// --- MARK: PUBLIC ENTRY POINT

#[must_use]
pub fn panel(theme: &Theme) -> BreadcrumbDemoPanel {
    BreadcrumbDemoPanel { theme: *theme }
}
