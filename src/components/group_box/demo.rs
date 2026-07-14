//! Group box demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::group_box;
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::button::button;
use crate::components::checkbox::checkbox;
use crate::components::radio::radio;
use crate::components::toggle::toggle;
use crate::{label, scroll_container, separator};

#[derive(Debug, Clone)]
struct GroupBoxDemo {
    subscriptions: [bool; 3],
    private_profile: bool,
    include_private_contributions: bool,
    appearance: Option<usize>,
    hide_activity: bool,
}

impl GroupBoxDemo {
    fn new() -> Self {
        Self {
            subscriptions: [false, false, false],
            private_profile: true,
            include_private_contributions: false,
            appearance: None,
            hide_activity: true,
        }
    }
}

type InnerView = Box<AnyWidgetView<GroupBoxDemo>>;
type InnerViewState = <InnerView as View<GroupBoxDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the group box demo panel.
pub struct GroupBoxDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct GroupBoxDemoPanelState {
    demo: GroupBoxDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the `GroupBox` demo panel.
///
/// The returned view owns all interactive state internally; callers do not
/// need to store or lens into any group box state.
#[must_use]
pub fn panel(theme: &Theme) -> GroupBoxDemoPanel {
    GroupBoxDemoPanel { theme: *theme }
}

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

/// A row pairing a label with a control, the control pushed to the trailing edge.
fn labeled_row<S: 'static, C: WidgetView<S> + 'static>(
    text: &'static str,
    control: C,
    theme: &Theme,
) -> impl WidgetView<S> + use<S, C> {
    flex_row((label(text).render(theme), control))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_alignment(MainAxisAlignment::SpaceBetween)
}

/// Default style — checkbox group with a trailing action button.
fn default_section(theme: &Theme, state: &GroupBoxDemo) -> Box<AnyWidgetView<GroupBoxDemo>> {
    let [sub_all, sub_news, sub_activity] = state.subscriptions;
    group_box(
        flex_col((
            checkbox(sub_all, |s: &mut GroupBoxDemo, checked: bool| {
                s.subscriptions[0] = checked;
            })
            .label("All")
            .render(theme),
            checkbox(sub_news, |s: &mut GroupBoxDemo, checked: bool| {
                s.subscriptions[1] = checked;
            })
            .label("Newsletter")
            .render(theme),
            checkbox(sub_activity, |s: &mut GroupBoxDemo, checked: bool| {
                s.subscriptions[2] = checked;
            })
            .label("Account Activity")
            .render(theme),
            button(|_: &mut GroupBoxDemo| {})
                .label("Update Subscriptions")
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0)),
    )
    .title("Subscriptions")
    .render(theme)
}

/// Fill style — toggle rows with a primary action button.
fn fill_section(theme: &Theme, state: &GroupBoxDemo) -> Box<AnyWidgetView<GroupBoxDemo>> {
    group_box(
        flex_col((
            labeled_row(
                "Make profile private and hide activity",
                toggle(
                    state.private_profile,
                    |s: &mut GroupBoxDemo, checked: bool| {
                        s.private_profile = checked;
                    },
                )
                .render(theme),
                theme,
            ),
            labeled_row(
                "Include private contributions on my profile",
                toggle(
                    state.include_private_contributions,
                    |s: &mut GroupBoxDemo, checked: bool| {
                        s.include_private_contributions = checked;
                    },
                )
                .render(theme),
                theme,
            ),
            button(|_: &mut GroupBoxDemo| {})
                .label("Save")
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(12.0)),
    )
    .title("Contributions & activity")
    .fill()
    .render(theme)
}

/// Outline style — a radio group.
fn outline_section(theme: &Theme, state: &GroupBoxDemo) -> Box<AnyWidgetView<GroupBoxDemo>> {
    group_box(
        flex_col((
            radio("Light", |s: &mut GroupBoxDemo| s.appearance = Some(0))
                .selected(state.appearance == Some(0))
                .render(theme),
            radio("Dark", |s: &mut GroupBoxDemo| s.appearance = Some(1))
                .selected(state.appearance == Some(1))
                .render(theme),
            radio("System", |s: &mut GroupBoxDemo| s.appearance = Some(2))
                .selected(state.appearance == Some(2))
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(6.0)),
    )
    .title("Appearance")
    .border()
    .render(theme)
}

/// Without a title — a single row, outlined.
fn untitled_section(theme: &Theme, state: &GroupBoxDemo) -> Box<AnyWidgetView<GroupBoxDemo>> {
    group_box(labeled_row(
        "Make profile private and hide activity",
        toggle(
            state.hide_activity,
            |s: &mut GroupBoxDemo, checked: bool| {
                s.hide_activity = checked;
            },
        )
        .render(theme),
        theme,
    ))
    .border()
    .render(theme)
}

/// Custom style — overridden title color and a custom-background content area.
fn custom_section(theme: &Theme) -> Box<AnyWidgetView<GroupBoxDemo>> {
    group_box(
        flex_col((label(
            "You can use .title_color() to recolor the title, and \
                 .background() to override the content container's fill.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(8.0)),
    )
    .title("This is a custom style")
    .title_color(theme.palette.text_faint)
    .background(theme.palette.surface_2)
    .border()
    .render(theme)
}

fn build_inner(theme: &Theme, state: &GroupBoxDemo) -> impl WidgetView<GroupBoxDemo> + use<> {
    let title_block = flex_col((
        label("Group Box")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Titled container that visually groups related content.")
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
            section_header("Default Style", theme),
            default_section(theme, state),
            section_header("Fill Style", theme),
            fill_section(theme, state),
            section_header("Outline Style", theme),
            outline_section(theme, state),
            section_header("Without Title", theme),
            untitled_section(theme, state),
            section_header("Custom Style", theme),
            custom_section(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for GroupBoxDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for GroupBoxDemoPanel {
    type ViewState = GroupBoxDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = GroupBoxDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            GroupBoxDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut GroupBoxDemoPanelState,
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
        vs: &mut GroupBoxDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut GroupBoxDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}
