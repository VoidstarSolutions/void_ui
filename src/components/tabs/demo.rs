//! Tabs demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{TabItem, TabsVariant, tabs};
use crate::components::IconName;
use crate::components::ScrollBarVisibility;
use crate::label;
use crate::scroll_container;
use crate::with_source;
use crate::{Theme, separator};

// --- MARK: LOCAL STATE

struct TabsDemo {
    default: usize,
    underline: usize,
    pill: usize,
    outline: usize,
    segmented: usize,
    segmented_fill: usize,
}

impl TabsDemo {
    fn new() -> Self {
        Self {
            default: 0,
            underline: 0,
            pill: 1,
            outline: 0,
            segmented: 1,
            segmented_fill: 0,
        }
    }
}

type InnerView = Box<AnyWidgetView<TabsDemo>>;
type InnerViewState = <InnerView as View<TabsDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL VIEW STATE

pub struct TabsDemoPanelState {
    demo: TabsDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

pub struct TabsDemoPanel {
    theme: Theme,
}

// --- MARK: ITEM SETS

fn label_items() -> Vec<TabItem> {
    vec![
        TabItem::label("Account"),
        TabItem::label("Profile"),
        TabItem::label("Documents"),
        TabItem::label("Mail"),
    ]
}

fn icon_items() -> Vec<TabItem> {
    vec![
        TabItem::label("Appearance").with_icon(IconName::Settings),
        TabItem::label("Settings").with_icon(IconName::User),
        TabItem::label("About").with_icon(IconName::Info),
    ]
}

/// Body content shown for the selected tab in the "Default" example, one
/// entry per item in [`label_items`].
fn tab_body(theme: &Theme, selected: usize) -> Box<AnyWidgetView<TabsDemo>> {
    let text = match selected {
        0 => "Manage your account details, password, and connected services.",
        1 => "Update your display name, avatar, and public profile information.",
        2 => "Browse, upload, and organize files shared with your team.",
        _ => "Configure email notifications and message filtering rules.",
    };
    Box::new(
        label(text)
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    )
}

// --- MARK: INNER VIEW BUILDER

fn build_inner(theme: &Theme, state: &TabsDemo) -> impl WidgetView<TabsDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Tabs")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("A row of switchable items. Selection is host-managed: the host owns the selected index and is notified when the user picks a different tab.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let default_tabs = with_source!(theme, {
        flex_col((
            tabs(label_items(), state.default, |s: &mut TabsDemo, i| {
                s.default = i;
            })
            .variant(TabsVariant::Default)
            .render(theme),
            tab_body(theme, state.default),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(8.0))
    });

    let underline_tabs = with_source!(theme, {
        tabs(label_items(), state.underline, |s: &mut TabsDemo, i| {
            s.underline = i;
        })
        .variant(TabsVariant::Underline)
        .render(theme)
    });

    let pill_tabs = with_source!(theme, {
        tabs(label_items(), state.pill, |s: &mut TabsDemo, i| {
            s.pill = i;
        })
        .variant(TabsVariant::Pill)
        .render(theme)
    });

    let outline_tabs = with_source!(theme, {
        tabs(label_items(), state.outline, |s: &mut TabsDemo, i| {
            s.outline = i;
        })
        .variant(TabsVariant::Outline)
        .render(theme)
    });

    let segmented_tabs = with_source!(theme, {
        tabs(icon_items(), state.segmented, |s: &mut TabsDemo, i| {
            s.segmented = i;
        })
        .variant(TabsVariant::Segmented)
        .render(theme)
    });

    let segmented_fill_tabs = with_source!(theme, {
        tabs(icon_items(), state.segmented_fill, |s: &mut TabsDemo, i| {
            s.segmented_fill = i;
        })
        .variant(TabsVariant::SegmentedFill)
        .render(theme)
    });

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            header("Default"),
            default_tabs,
            header("Underline"),
            underline_tabs,
            header("Pill"),
            pill_tabs,
            header("Outline"),
            outline_tabs,
            header("Segmented"),
            segmented_tabs,
            header("Segmented (fill)"),
            segmented_fill_tabs,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// --- MARK: VIEW IMPL

impl ViewMarker for TabsDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for TabsDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = TabsDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = TabsDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            TabsDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut TabsDemoPanelState,
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
        vs: &mut TabsDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut TabsDemoPanelState,
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
pub fn panel(theme: &Theme) -> TabsDemoPanel {
    TabsDemoPanel { theme: *theme }
}
