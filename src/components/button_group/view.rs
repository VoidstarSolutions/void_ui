//! Button group and toggle button group — theme-driven segmented controls.
//!
//! Both builders produce a horizontal or vertical row of [`button`]s with
//! positional corner radii so only the outer edges of the group are rounded.
//! An index-based callback is used so the caller can handle any item with a
//! single closure.
//!
//! There is no custom masonry widget and no view state: [`ButtonGroup::render`]
//! composes the existing themed buttons inside a flex row/column and returns
//! the type-erased view directly. A toggle group is the same component with a
//! host-owned selected index.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! # struct State { view: usize }
//! # impl State { fn cut(&mut self) {} fn copy(&mut self) {} fn paste(&mut self) {} }
//! # let state = State { view: 0 };
//! use void_ui::components::{button_group, toggle_button_group};
//!
//! // Regular horizontal group
//! button_group(["Cut", "Copy", "Paste"], |s: &mut State, i| match i {
//!     0 => s.cut(),
//!     1 => s.copy(),
//!     _ => s.paste(),
//! })
//! .render(&theme);
//!
//! // Toggle group — host owns the selected index
//! toggle_button_group(["Day", "Week", "Month"], state.view, |s: &mut State, i| {
//!     s.view = i;
//! })
//! .render(&theme)
//! # ;
//! ```

use masonry::core::ArcStr;
use masonry::kurbo::RoundedRectRadii;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{AnyFlexChild, CrossAxisAlignment, FlexExt as _, flex_col, flex_row};
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::components::button::{ButtonVariant, button};

/// Returns the appropriate `RoundedRectRadii` for a button at position `i`
/// in a group of `count` buttons.
///
/// Only the outer edges of the group are rounded; inner edges are square so
/// adjacent buttons form a seamless segmented control.
fn position_corners(i: usize, count: usize, vertical: bool, r: f64) -> RoundedRectRadii {
    let first = i == 0;
    let last = i + 1 == count;
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

/// Builder for a button group — a connected segmented control.
///
/// Created with [`button_group`] (no selection) or [`toggle_button_group`]
/// (exclusive host-owned selection). All items share the same variant and
/// fire a single callback with the clicked item's index. Returns a xilem
/// view via [`Self::render`].
#[must_use = "ButtonGroup does nothing until rendered with .render(&theme)"]
pub struct ButtonGroup<F> {
    items: Vec<ArcStr>,
    /// `Some` marks the active item of a toggle group; `None` for a plain group.
    selected: Option<usize>,
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
        selected: None,
        variant: ButtonVariant::Default,
        disabled: false,
        vertical: false,
        callback,
    }
}

/// Create a toggle button group with exclusive single-item selection.
///
/// The host owns the selected index and passes it in; `callback` is called
/// with the new index when the user clicks a different item.
pub fn toggle_button_group<F>(
    items: impl IntoIterator<Item = impl Into<ArcStr>>,
    selected: usize,
    callback: F,
) -> ButtonGroup<F> {
    ButtonGroup {
        selected: Some(selected),
        ..button_group(items, callback)
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
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<F, State, Action>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, usize) -> Action + Clone + Send + Sync + 'static,
    {
        let count = self.items.len();
        let btn_children: Vec<AnyFlexChild<State, Action>> = (0..count)
            .map(|i| {
                let cb = self.callback.clone();
                let corners =
                    position_corners(i, count, self.vertical, f64::from(theme.radius.small));
                button(move |s: &mut State| cb(s, i))
                    .label(self.items[i].clone())
                    .variant(self.variant)
                    .disabled(self.disabled)
                    .selected(self.selected == Some(i))
                    .corners(corners)
                    .render(theme)
                    .into_any_flex()
            })
            .collect();

        let view: Box<AnyWidgetView<State, Action>> = if self.vertical {
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
        };
        view
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{button_group, position_corners, toggle_button_group};
    use crate::{Theme, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn only_the_outer_edges_of_a_horizontal_group_are_rounded() {
        let r = 4.0;
        let first = position_corners(0, 3, false, r);
        assert_eq!((first.top_left, first.bottom_left), (r, r));
        assert_eq!((first.top_right, first.bottom_right), (0.0, 0.0));

        let middle = position_corners(1, 3, false, r);
        assert_eq!(
            (
                middle.top_left,
                middle.top_right,
                middle.bottom_right,
                middle.bottom_left
            ),
            (0.0, 0.0, 0.0, 0.0)
        );

        let last = position_corners(2, 3, false, r);
        assert_eq!((last.top_right, last.bottom_right), (r, r));
        assert_eq!((last.top_left, last.bottom_left), (0.0, 0.0));
    }

    #[test]
    fn only_the_outer_edges_of_a_vertical_group_are_rounded() {
        let r = 4.0;
        let first = position_corners(0, 2, true, r);
        assert_eq!((first.top_left, first.top_right), (r, r));
        assert_eq!((first.bottom_left, first.bottom_right), (0.0, 0.0));

        let last = position_corners(1, 2, true, r);
        assert_eq!((last.bottom_left, last.bottom_right), (r, r));
        assert_eq!((last.top_left, last.top_right), (0.0, 0.0));
    }

    #[test]
    fn a_single_item_group_rounds_all_four_corners() {
        let r = 4.0;
        let only = position_corners(0, 1, false, r);
        assert_eq!(
            (
                only.top_left,
                only.top_right,
                only.bottom_right,
                only.bottom_left
            ),
            (r, r, r, r)
        );
    }

    #[test]
    fn toggle_button_group_marks_the_given_index_selected() {
        let group = toggle_button_group(["A", "B"], 1, |_: &mut AppState, _i: usize| ());
        assert_eq!(group.selected, Some(1));
    }

    #[test]
    fn button_group_has_no_selection() {
        let group = button_group(["A", "B"], |_: &mut AppState, _i: usize| ());
        assert_eq!(group.selected, None);
    }

    #[test]
    fn horizontal_and_vertical_groups_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = button_group(["Cut", "Copy", "Paste"], |_: &mut AppState, _i| ())
            .render(&theme)
            .build(&mut ctx, &mut state);
        let _ = toggle_button_group(["Day", "Week"], 0, |_: &mut AppState, _i| ())
            .vertical()
            .disabled(true)
            .render(&theme)
            .build(&mut ctx, &mut state);
    }
}
