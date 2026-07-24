//! Xilem view for the breadcrumb component.
//!
//! An ordered list of segments joined by a themed chevron separator: a
//! segment with an [`BreadcrumbSegment::on_select`] callback renders as a
//! quiet inline button (an ancestor to navigate to); a segment without one
//! renders as a plain, full-color label (the current location) — so callers
//! get "last segment styled as current by default" for free by simply not
//! attaching a callback to it, no separate "is current" flag needed. There is
//! no custom masonry widget: [`Breadcrumb::render`] composes
//! [`crate::button`], [`crate::icon`], and [`crate::label`] in a `flex_row`,
//! marking the whole trail as a navigation landmark and the current segment
//! as `AriaCurrent::Page` via [`access_wrap::annotate`].
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State;
//! # impl State { fn navigate_home(&mut self) {} }
//! use void_ui::{breadcrumb, segment};
//!
//! breadcrumb()
//!     .segment(segment("Trade Dashboard").on_select(|s: &mut State| s.navigate_home()))
//!     .segment(segment("Trade dashboard"))
//!     .render(&theme)
//! # ;
//! ```

use masonry::core::ArcStr;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row};
use xilem::{AnyWidgetView, WidgetView};

use crate::components::access_wrap::{self, AccessAnnotation};
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
    ///
    /// The returned view is built from owned values (segment labels, colors
    /// read out of `theme`), so it does not borrow `theme`. `use<State,
    /// Action>` precises the opaque type's captures to just the generic
    /// parameters — without it, edition-2024 RPIT rules would capture the
    /// `&Theme` lifetime too, barring callers that store the trail in a
    /// `+ use<>` (lifetime-free) view, e.g. an app top bar.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render(self, theme: &Theme) -> impl WidgetView<State, Action> + use<State, Action>
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
                        .decorative()
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
                None => Box::new(access_wrap::annotate(
                    label(seg.label).color(theme.palette.text).render(theme),
                    AccessAnnotation::CurrentPage,
                )),
            };
            children.push(view);
        }
        let trail = flex_row(children)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.pad) / 3.0));
        access_wrap::annotate(trail, AccessAnnotation::Navigation)
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{breadcrumb, segment};
    use crate::{Theme, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn empty_trail_builds_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = breadcrumb::<AppState, ()>()
            .render(&theme)
            .build(&mut ctx, &mut state);
    }

    #[test]
    fn trail_with_ancestor_and_current_segments_builds_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let trail = breadcrumb()
            .segment(segment("Home").on_select(|_: &mut AppState| ()))
            .segment(segment("Section").on_select(|_: &mut AppState| ()))
            .segment(segment("Current"));
        let _ = trail.render(&theme).build(&mut ctx, &mut state);
    }

    #[test]
    fn single_segment_trail_builds_without_a_leading_chevron() {
        // Only segments after the first get a chevron separator; a lone
        // segment must build cleanly with none.
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let trail = breadcrumb().segment(segment::<AppState, ()>("Only"));
        let _ = trail.render(&theme).build(&mut ctx, &mut state);
    }
}
