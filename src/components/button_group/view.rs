//! Button group and toggle button group — theme-driven segmented controls.
//!
//! Both builders produce a horizontal or vertical row of [`button`]s with
//! positional corner radii so only the outer edges of the group are rounded.
//! An index-based callback is used so the caller can handle any item with a
//! single closure.
//!
//! ```ignore
//! // Regular horizontal group
//! button_group(["Cut", "Copy", "Paste"], |s: &mut State, i| match i {
//!     0 => s.cut(),
//!     1 => s.copy(),
//!     _ => s.paste(),
//! })
//! .render(&theme)
//!
//! // Toggle group — host owns the selected index
//! toggle_button_group(["Day", "Week", "Month"], state.view, |s: &mut State, i| {
//!     s.view = i;
//! })
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::ArcStr;
use masonry::kurbo::RoundedRectRadii;
use masonry::widgets::Passthrough;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{AnyFlexChild, CrossAxisAlignment, FlexExt as _, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use crate::Theme;
use crate::components::button::{ButtonVariant, button};

/// Corner radius shared with `ThemedButton::CORNER_RADIUS`.
const GROUP_CORNER_RADIUS: f64 = 5.0;

/// Returns the appropriate `RoundedRectRadii` for a button at position `i`
/// in a group of `count` buttons.
///
/// Only the outer edges of the group are rounded; inner edges are square so
/// adjacent buttons form a seamless segmented control.
fn position_corners(i: usize, count: usize, vertical: bool) -> RoundedRectRadii {
    let first = i == 0;
    let last = i + 1 == count;
    let r = GROUP_CORNER_RADIUS;
    if vertical {
        RoundedRectRadii::new(
            if first { r } else { 0.0 }, // top_left
            if first { r } else { 0.0 }, // top_right
            if last { r } else { 0.0 },  // bottom_right
            if last { r } else { 0.0 },  // bottom_left
        )
    } else {
        RoundedRectRadii::new(
            if first { r } else { 0.0 }, // top_left
            if last { r } else { 0.0 },  // top_right
            if last { r } else { 0.0 },  // bottom_right
            if first { r } else { 0.0 }, // bottom_left
        )
    }
}

/// Builds the inner `flex_row` / `flex_col` of buttons, shared by both group types.
fn build_group_inner<S, A, F>(
    items: &[ArcStr],
    selected: Option<usize>,
    variant: ButtonVariant,
    disabled: bool,
    vertical: bool,
    callback: F,
    theme: &Theme,
) -> Box<AnyWidgetView<S, A>>
where
    F: Fn(&mut S, usize) -> A + Clone + Send + Sync + 'static,
    S: 'static,
    A: 'static,
{
    let count = items.len();
    let btn_children: Vec<AnyFlexChild<S, A>> = (0..count)
        .map(|i| {
            let cb = callback.clone();
            let corners = position_corners(i, count, vertical);
            button(move |s: &mut S| cb(s, i))
                .label(items[i].clone())
                .variant(variant)
                .disabled(disabled)
                .active(selected == Some(i))
                .corners(corners)
                .render(theme)
                .into_any_flex()
        })
        .collect();

    if vertical {
        Box::new(
            flex_col(btn_children)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(Length::ZERO),
        )
    } else {
        Box::new(
            flex_row(btn_children)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .gap(Length::ZERO),
        )
    }
}

// ---------------------------------------------------------------------------
// ButtonGroup
// ---------------------------------------------------------------------------

/// Builder for a regular (non-toggle) button group.
///
/// Created with [`button_group`]. All items share the same variant and fire a
/// single callback with the clicked item's index. Returns a xilem [`View`] via
/// [`Self::render`].
#[must_use = "ButtonGroup does nothing until rendered with .render(&theme)"]
pub struct ButtonGroup<F> {
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    vertical: bool,
    callback: F,
}

/// Create a horizontal button group.
///
/// `items` is any iterator of strings; `callback` receives the index of the
/// clicked button. The group renders as a connected segmented control.
pub fn button_group<F>(
    items: impl IntoIterator<Item = impl Into<ArcStr>>,
    callback: F,
) -> ButtonGroup<F> {
    ButtonGroup {
        items: items.into_iter().map(Into::into).collect(),
        variant: ButtonVariant::Default,
        disabled: false,
        vertical: false,
        callback,
    }
}

impl<F> ButtonGroup<F> {
    /// Render buttons in a vertical column instead of a horizontal row.
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    /// Set the visual style variant applied to every button in the group.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Disable all buttons in the group.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ButtonGroupView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, usize) -> Action + Clone + Send + Sync + 'static,
    {
        ButtonGroupView {
            items: self.items,
            variant: self.variant,
            disabled: self.disabled,
            vertical: self.vertical,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`ButtonGroup`].
///
/// Built only through [`ButtonGroup::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ButtonGroupView<F, State, Action> {
    items: Vec<ArcStr>,
    variant: ButtonVariant,
    disabled: bool,
    vertical: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Opaque view state for [`ButtonGroupView`].
#[doc(hidden)]
pub struct ButtonGroupViewState<S: 'static, A: 'static> {
    inner: Box<AnyWidgetView<S, A>>,
    inner_state: <Box<AnyWidgetView<S, A>> as View<S, A, ViewCtx>>::ViewState,
}

impl<F, State, Action> ViewMarker for ButtonGroupView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for ButtonGroupView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, usize) -> Action + Clone + Send + Sync + 'static,
{
    type Element = Pod<Passthrough>;
    type ViewState = ButtonGroupViewState<State, Action>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let inner = build_group_inner(
            &self.items,
            None,
            self.variant,
            self.disabled,
            self.vertical,
            self.callback.clone(),
            &self.theme,
        );
        let (element, inner_state) = ctx.with_id(ViewId::new(0), |ctx| inner.build(ctx, state));
        (element, ButtonGroupViewState { inner, inner_state })
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        let new_inner = build_group_inner(
            &self.items,
            None,
            self.variant,
            self.disabled,
            self.vertical,
            self.callback.clone(),
            &self.theme,
        );
        ctx.with_id(ViewId::new(0), |ctx| {
            new_inner.rebuild(&vs.inner, &mut vs.inner_state, ctx, element, state);
        });
        vs.inner = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.inner.teardown(&mut vs.inner_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_first().map(ViewId::routing_id) {
            Some(0) => vs
                .inner
                .message(&mut vs.inner_state, message, element, state),
            _ => MessageResult::Stale,
        }
    }
}

// ---------------------------------------------------------------------------
// ToggleButtonGroup
// ---------------------------------------------------------------------------

/// Builder for a toggle button group with exclusive single-item selection.
///
/// Created with [`toggle_button_group`]. The host owns the selected index and
/// passes it in; the callback is fired with the newly-selected index so the
/// host can update its state. Only one item can be active at a time.
#[must_use = "ToggleButtonGroup does nothing until rendered with .render(&theme)"]
pub struct ToggleButtonGroup<F> {
    items: Vec<ArcStr>,
    selected: usize,
    variant: ButtonVariant,
    disabled: bool,
    vertical: bool,
    callback: F,
}

/// Create a toggle button group with exclusive single-item selection.
///
/// `selected` is the index of the currently active item. `callback` is called
/// with the new index when the user clicks a different item.
pub fn toggle_button_group<F>(
    items: impl IntoIterator<Item = impl Into<ArcStr>>,
    selected: usize,
    callback: F,
) -> ToggleButtonGroup<F> {
    ToggleButtonGroup {
        items: items.into_iter().map(Into::into).collect(),
        selected,
        variant: ButtonVariant::Default,
        disabled: false,
        vertical: false,
        callback,
    }
}

impl<F> ToggleButtonGroup<F> {
    /// Render buttons in a vertical column instead of a horizontal row.
    pub fn vertical(mut self) -> Self {
        self.vertical = true;
        self
    }

    /// Set the visual style variant applied to every button in the group.
    pub fn variant(mut self, v: ButtonVariant) -> Self {
        self.variant = v;
        self
    }

    /// Disable all buttons in the group.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ToggleButtonGroupView<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, usize) -> Action + Clone + Send + Sync + 'static,
    {
        ToggleButtonGroupView {
            items: self.items,
            selected: self.selected,
            variant: self.variant,
            disabled: self.disabled,
            vertical: self.vertical,
            theme: *theme,
            callback: self.callback,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`ToggleButtonGroup`].
///
/// Built only through [`ToggleButtonGroup::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ToggleButtonGroupView<F, State, Action> {
    items: Vec<ArcStr>,
    selected: usize,
    variant: ButtonVariant,
    disabled: bool,
    vertical: bool,
    theme: Theme,
    callback: F,
    phantom: PhantomData<fn(State) -> Action>,
}

/// Opaque view state for [`ToggleButtonGroupView`].
#[doc(hidden)]
pub struct ToggleButtonGroupViewState<S: 'static, A: 'static> {
    inner: Box<AnyWidgetView<S, A>>,
    inner_state: <Box<AnyWidgetView<S, A>> as View<S, A, ViewCtx>>::ViewState,
}

impl<F, State, Action> ViewMarker for ToggleButtonGroupView<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for ToggleButtonGroupView<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, usize) -> Action + Clone + Send + Sync + 'static,
{
    type Element = Pod<Passthrough>;
    type ViewState = ToggleButtonGroupViewState<State, Action>;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let inner = build_group_inner(
            &self.items,
            Some(self.selected),
            self.variant,
            self.disabled,
            self.vertical,
            self.callback.clone(),
            &self.theme,
        );
        let (element, inner_state) = ctx.with_id(ViewId::new(0), |ctx| inner.build(ctx, state));
        (element, ToggleButtonGroupViewState { inner, inner_state })
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        let new_inner = build_group_inner(
            &self.items,
            Some(self.selected),
            self.variant,
            self.disabled,
            self.vertical,
            self.callback.clone(),
            &self.theme,
        );
        ctx.with_id(ViewId::new(0), |ctx| {
            new_inner.rebuild(&vs.inner, &mut vs.inner_state, ctx, element, state);
        });
        vs.inner = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            vs.inner.teardown(&mut vs.inner_state, ctx, element);
        });
    }

    fn message(
        &self,
        vs: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_first().map(ViewId::routing_id) {
            Some(0) => vs
                .inner
                .message(&mut vs.inner_state, message, element, state),
            _ => MessageResult::Stale,
        }
    }
}
