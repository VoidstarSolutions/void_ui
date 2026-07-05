//! Xilem view for the breadcrumb component.
//!
//! An ordered list of segments joined by a themed chevron separator: a
//! segment with an [`BreadcrumbSegment::on_select`] callback renders as a
//! quiet inline button (an ancestor to navigate to); a segment without one
//! renders as a plain, full-color label (the current location) — so callers
//! get "last segment styled as current by default" for free by simply not
//! attaching a callback to it, no separate "is current" flag needed. There is
//! no custom masonry widget: [`Breadcrumb::render`] composes
//! [`crate::button`], [`crate::icon`], and [`crate::label`] in a `flex_row`.
//!
//! ```ignore
//! use void_ui::{breadcrumb, segment};
//!
//! breadcrumb()
//!     .segment(segment("Trade Dashboard").on_select(|s: &mut State| s.navigate_home()))
//!     .segment(segment("Trade dashboard"))
//!     .render(&theme)
//! ```

use masonry::core::ArcStr;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::widget::BreadcrumbNav;
use crate::{ButtonVariant, IconName, Theme, button, icon, label};

type SelectCallback<State, Action> = Box<dyn Fn(&mut State) -> Action + Send + Sync>;

/// One entry in a [`Breadcrumb`], before it is added via [`Breadcrumb::segment`].
///
/// Build one with [`segment`]; chain [`Self::on_select`] to make it an
/// interactive ancestor link instead of a plain current-location label.
#[must_use = "BreadcrumbSegment does nothing until added to a breadcrumb() with .segment(...)"]
pub struct BreadcrumbSegment<State, Action> {
    label: ArcStr,
    on_select: Option<SelectCallback<State, Action>>,
}

/// Start building a breadcrumb segment with the given label.
pub fn segment<State, Action>(label: impl Into<ArcStr>) -> BreadcrumbSegment<State, Action> {
    BreadcrumbSegment {
        label: label.into(),
        on_select: None,
    }
}

impl<State, Action> BreadcrumbSegment<State, Action> {
    /// Make this segment interactive: it renders as a quiet inline button
    /// that invokes `callback` on click, instead of a plain current-location
    /// label.
    pub fn on_select<F>(mut self, callback: F) -> Self
    where
        F: Fn(&mut State) -> Action + Send + Sync + 'static,
    {
        self.on_select = Some(Box::new(callback));
        self
    }
}

/// Builder for a breadcrumb trail.
///
/// Created with [`breadcrumb`]. Add segments in order with [`Self::segment`];
/// materialize as a xilem view via [`Self::render`].
#[must_use = "Breadcrumb does nothing until rendered with .render(&theme)"]
pub struct Breadcrumb<State, Action> {
    segments: Vec<BreadcrumbSegment<State, Action>>,
}

/// Start building a breadcrumb trail with no segments.
pub fn breadcrumb<State, Action>() -> Breadcrumb<State, Action> {
    Breadcrumb {
        segments: Vec::new(),
    }
}

impl<State, Action> Breadcrumb<State, Action> {
    /// Append a segment to the trail.
    pub fn segment(mut self, segment: BreadcrumbSegment<State, Action>) -> Self {
        self.segments.push(segment);
        self
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render(self, theme: &Theme) -> impl WidgetView<State, Action>
    where
        State: 'static,
        Action: 'static,
    {
        let mut children: Vec<Box<AnyWidgetView<State, Action>>> =
            Vec::with_capacity(self.segments.len() * 2);
        for (i, seg) in self.segments.into_iter().enumerate() {
            if i > 0 {
                children.push(Box::new(
                    icon(IconName::ChevronRight)
                        .color(theme.palette.text_faint)
                        .size(theme.typography.size_caption)
                        .render(theme),
                ));
            }
            let view: Box<AnyWidgetView<State, Action>> = match seg.on_select {
                Some(on_select) => Box::new(
                    button(move |s: &mut State| on_select(s))
                        .label(seg.label)
                        .variant(ButtonVariant::Text)
                        .tint(theme.palette.text_muted)
                        .render(theme),
                ),
                None => Box::new(label(seg.label).color(theme.palette.text).render(theme)),
            };
            children.push(view);
        }
        let trail = flex_row(children)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.pad) / 3.0));
        BreadcrumbNavView { inner: trail }
    }
}

/// Wraps a breadcrumb trail view in [`BreadcrumbNav`] so it's announced as a
/// navigation landmark. A thin hand-written [`View`], mirroring the
/// transparent single-child pattern `xilem_masonry`'s own `sized_box` uses,
/// since `flex_row` alone has no accessibility-role override to reach for.
struct BreadcrumbNavView<V> {
    inner: V,
}

impl<V> ViewMarker for BreadcrumbNavView<V> {}

impl<State, Action, V> View<State, Action, ViewCtx> for BreadcrumbNavView<V>
where
    State: 'static,
    Action: 'static,
    V: WidgetView<State, Action>,
{
    type Element = Pod<BreadcrumbNav>;
    type ViewState = V::ViewState;

    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.inner.build(ctx, state);
        let widget = BreadcrumbNav::new(child.new_widget);
        (ctx.create_pod(widget), child_state)
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) {
        let mut child = BreadcrumbNav::child_mut(&mut element);
        self.inner
            .rebuild(&prev.inner, view_state, ctx, child.downcast(), state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = BreadcrumbNav::child_mut(&mut element);
        self.inner.teardown(view_state, ctx, child.downcast());
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<Action> {
        let mut child = BreadcrumbNav::child_mut(&mut element);
        self.inner
            .message(view_state, message, child.downcast(), state)
    }
}
