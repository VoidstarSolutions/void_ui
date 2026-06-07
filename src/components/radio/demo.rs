//! Radio button demo panel used by the void-ui gallery.
//!
//! The interactive group section uses a custom [`View`] implementation whose
//! [`ViewState`] holds the selected index. No gallery state is involved;
//! clicking a radio returns [`MessageResult::RequestRebuild`], which causes
//! xilem to rebuild the view tree with the updated selection from `ViewState`.

use std::marker::PhantomData;

use masonry::widgets::Passthrough;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row};

use crate::label;
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::components::radio::radio;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

// ---------------------------------------------------------------------------
// Public panel
// ---------------------------------------------------------------------------

/// Renders the Radio demo panel.
///
/// Covers unselected, selected, disabled, and a live interactive group whose
/// state is entirely self-contained (no gallery state required).
#[must_use]
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let standalone_example = SingleToggleDemo::new(*theme);

    let disabled_example = with_source!(theme, {
        flex_row((
            radio("Disabled unselected", |_: &mut S| {})
                .active(false)
                .disabled(true)
                .render(theme),
            radio("Disabled selected", |_: &mut S| {})
                .active(true)
                .disabled(true)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0))
    });

    let title_block = flex_col((
        label("Radio")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Single-selection button. Group mutual-exclusion is managed by the host.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            header("Standalone"),
            standalone_example,
            header("Disabled"),
            disabled_example,
            header("Group — click to change selection"),
            RadioGroupDemo::new(*theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

// ---------------------------------------------------------------------------
// Self-contained toggleable single radio
// ---------------------------------------------------------------------------

type ToggleChildView<S> = Box<AnyWidgetView<S, ()>>;
type ToggleChildState<S> = <ToggleChildView<S> as View<S, (), ViewCtx>>::ViewState;

#[doc(hidden)]
pub struct SingleToggleDemoState<S: 'static> {
    selected: bool,
    prev_child: ToggleChildView<S>,
    child_state: ToggleChildState<S>,
}

/// A single standalone radio whose selected state is self-contained.
///
/// Clicking it flips its `selected` state and returns
/// [`MessageResult::RequestRebuild`] — no host app state involved.
pub struct SingleToggleDemo<S: 'static> {
    theme: Theme,
    phantom: PhantomData<fn() -> S>,
}

impl<S: 'static> SingleToggleDemo<S> {
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            phantom: PhantomData,
        }
    }
}

fn build_toggle_child<S: 'static>(selected: bool, theme: &Theme) -> ToggleChildView<S> {
    let t = *theme;
    Box::new(
        radio("Radio Button", |_: &mut S| {})
            .active(selected)
            .render(&t),
    )
}

impl<S: 'static> ViewMarker for SingleToggleDemo<S> {}

impl<S: 'static> View<S, (), ViewCtx> for SingleToggleDemo<S> {
    type Element = Pod<Passthrough>;
    type ViewState = SingleToggleDemoState<S>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut S) -> (Self::Element, Self::ViewState) {
        let selected = false;
        let child = build_toggle_child(selected, &self.theme);
        let (element, child_state) = ctx.with_id(ViewId::new(0), |ctx| child.build(ctx, state));
        (
            element,
            SingleToggleDemoState {
                selected,
                prev_child: child,
                child_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) {
        let new_child = build_toggle_child(vs.selected, &self.theme);
        ctx.with_id(ViewId::new(0), |ctx| {
            new_child.rebuild(&vs.prev_child, &mut vs.child_state, ctx, element, state);
        });
        vs.prev_child = new_child;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.prev_child.teardown(&mut vs.child_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) -> MessageResult<()> {
        match message.take_first().map(ViewId::routing_id) {
            Some(0) => {
                let result = vs
                    .prev_child
                    .message(&mut vs.child_state, message, element, state);
                match result {
                    MessageResult::Action(()) => {
                        vs.selected = !vs.selected;
                        MessageResult::RequestRebuild
                    }
                    other => other,
                }
            }
            _ => MessageResult::Stale,
        }
    }
}

// ---------------------------------------------------------------------------
// Self-contained interactive radio group
// ---------------------------------------------------------------------------

/// The child view type for the group: a boxed, type-erased flex row of radios
/// that produce `usize` (the selected index) as their action.
type GroupChildView<S> = Box<AnyWidgetView<S, usize>>;

/// The opaque xilem `ViewState` carried by the boxed child view.
type GroupChildState<S> = <GroupChildView<S> as View<S, usize, ViewCtx>>::ViewState;

/// State persisted across rebuilds by xilem for [`RadioGroupDemo`].
///
/// Stores the current selection index alongside the child view and its
/// xilem `ViewState` so the view tree can be diff-updated on each rebuild.
#[doc(hidden)]
pub struct RadioGroupDemoState<S: 'static> {
    selected: usize,
    /// The view that was last built/rebuilt — passed as `prev` to the next
    /// `rebuild` so xilem can diff-update the radio row correctly.
    prev_child: GroupChildView<S>,
    child_state: GroupChildState<S>,
}

/// A self-contained radio group view.
///
/// Its `ViewState` holds the selected index and the inner flex-row child view.
/// When any radio is clicked, [`message`](View::message) updates the index and
/// returns [`MessageResult::RequestRebuild`], triggering a re-render without
/// touching the host app's state.
pub struct RadioGroupDemo<S: 'static> {
    theme: Theme,
    phantom: PhantomData<fn() -> S>,
}

impl<S: 'static> RadioGroupDemo<S> {
    #[must_use]
    pub fn new(theme: Theme) -> Self {
        Self {
            theme,
            phantom: PhantomData,
        }
    }
}

/// Build the flex-row child view for the given selection index.
///
/// Each radio produces a `usize` (its index) as its action, which
/// `RadioGroupDemo::message` intercepts to update `ViewState`.
fn build_group_child<S: 'static>(selected: usize, theme: &Theme) -> GroupChildView<S> {
    let t = *theme;
    Box::new(
        flex_row((
            radio("Option A", move |_: &mut S| 0usize)
                .active(selected == 0)
                .render(&t),
            radio("Option B", move |_: &mut S| 1usize)
                .active(selected == 1)
                .render(&t),
            radio("Option C", move |_: &mut S| 2usize)
                .active(selected == 2)
                .render(&t),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(16.0)),
    )
}

impl<S: 'static> ViewMarker for RadioGroupDemo<S> {}

impl<S: 'static> View<S, (), ViewCtx> for RadioGroupDemo<S> {
    /// The element type matches `Box<AnyWidgetView<S, usize>>` — a
    /// `Passthrough` widget that wraps the inner flex row.
    type Element = Pod<Passthrough>;
    type ViewState = RadioGroupDemoState<S>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut S) -> (Self::Element, Self::ViewState) {
        let selected = 0;
        let child = build_group_child(selected, &self.theme);
        let (element, child_state) = ctx.with_id(ViewId::new(0), |ctx| child.build(ctx, state));
        (
            element,
            RadioGroupDemoState {
                selected,
                prev_child: child,
                child_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) {
        let new_child = build_group_child(vs.selected, &self.theme);
        ctx.with_id(ViewId::new(0), |ctx| {
            new_child.rebuild(&vs.prev_child, &mut vs.child_state, ctx, element, state);
        });
        vs.prev_child = new_child;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.prev_child.teardown(&mut vs.child_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut S,
    ) -> MessageResult<()> {
        // Route through the id we established in build/rebuild.
        match message.take_first().map(ViewId::routing_id) {
            Some(0) => {
                let result = vs
                    .prev_child
                    .message(&mut vs.child_state, message, element, state);
                match result {
                    MessageResult::Action(idx) => {
                        vs.selected = idx;
                        MessageResult::RequestRebuild
                    }
                    MessageResult::RequestRebuild => MessageResult::RequestRebuild,
                    MessageResult::Nop => MessageResult::Nop,
                    MessageResult::Stale => MessageResult::Stale,
                }
            }
            _ => MessageResult::Stale,
        }
    }
}
