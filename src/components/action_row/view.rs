//! Xilem view for the `action_row` component.
//!
//! A themed list-row layout: an optional leading status dot, a primary label
//! flexed to fill (with an optional muted inline summary), an optional trailing
//! status badge, and zero or more trailing action controls. Pure composition —
//! there is no custom masonry widget; [`ActionRow::render`] assembles existing
//! primitives (`status_dot`, `label`, `badge`, and caller-supplied action
//! views) into a padded `flex_row`, following the same layout as `alert`.
//!
//! ```ignore
//! use void_ui::{action_row, button, AlertVariant};
//!
//! action_row("EURUSD")
//!     .leading_dot(theme.palette.success)
//!     .secondary("spot")
//!     .badge("LIVE", AlertVariant::Success)
//!     .action(button(|s: &mut S| s.edit()).icon(IconName::Pencil)
//!         .variant(ButtonVariant::Text).accessible_name("Edit").render(&theme))
//!     .render(&theme)
//! ```

use masonry::core::ArcStr;
use masonry::peniko::Color;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{AnyFlexChild, CrossAxisAlignment, flex_item, flex_row, sized_box};
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::components::alert::AlertVariant;
use crate::components::badge::badge;
use crate::components::label::label;
use crate::components::status_dot::status_dot;

/// Builder for a themed action row. Created with [`action_row`]; materialize as
/// a xilem view with [`Self::render`]. Trailing [`action`](Self::action)
/// controls are supplied already rendered, so the caller styles them however
/// they like while `action_row` owns the row's alignment, gaps, and padding.
#[must_use = "ActionRow does nothing until rendered with .render(&theme)"]
pub struct ActionRow<State, Action> {
    label: ArcStr,
    leading_dot: Option<Color>,
    secondary: Option<ArcStr>,
    badge: Option<(ArcStr, AlertVariant)>,
    actions: Vec<Box<AnyWidgetView<State, Action>>>,
}

/// Starts an action row with its primary `label`. Add a leading dot, a summary,
/// a badge, and trailing actions with the chained setters, then
/// [`render`](ActionRow::render).
pub fn action_row<State, Action>(label: impl Into<ArcStr>) -> ActionRow<State, Action> {
    ActionRow {
        label: label.into(),
        leading_dot: None,
        secondary: None,
        badge: None,
        actions: Vec::new(),
    }
}

impl<State: 'static, Action: 'static> ActionRow<State, Action> {
    /// Adds a leading [`status_dot`] of `color` — the row's status indicator.
    pub fn leading_dot(mut self, color: Color) -> Self {
        self.leading_dot = Some(color);
        self
    }

    /// Adds a muted inline summary after the primary label (via the label's own
    /// secondary text).
    pub fn secondary(mut self, text: impl Into<ArcStr>) -> Self {
        self.secondary = Some(text.into());
        self
    }

    /// Adds a trailing status [`badge`] with the given `variant`.
    pub fn badge(mut self, text: impl Into<ArcStr>, variant: AlertVariant) -> Self {
        self.badge = Some((text.into(), variant));
        self
    }

    /// Appends a trailing action control (already rendered). Call repeatedly for
    /// multiple actions; they sit at the row's trailing edge in order.
    pub fn action(mut self, view: impl WidgetView<State, Action> + 'static) -> Self {
        self.actions.push(view.boxed());
        self
    }

    /// Materializes the row at the supplied theme.
    #[must_use]
    pub fn render(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>> {
        let mut children: Vec<AnyFlexChild<State, Action>> = Vec::new();

        // Leading status dot (fixed).
        if let Some(color) = self.leading_dot {
            children.push(flex_item(status_dot(color).render(theme), 0.0).into());
        }

        // Primary label + optional muted inline summary, flexed to fill so the
        // trailing content pins to the right.
        let mut primary = label(self.label).color(theme.palette.text);
        if let Some(summary) = self.secondary {
            primary = primary.secondary(summary);
        }
        children.push(flex_item(primary.render(theme), 1.0).into());

        // Optional trailing status badge (fixed).
        if let Some((text, variant)) = self.badge {
            children.push(flex_item(badge(text).variant(variant).render(theme), 0.0).into());
        }

        // Trailing action controls, grouped in a tighter inner row so they sit
        // closer to each other than to the main sections.
        if !self.actions.is_empty() {
            let actions: Vec<AnyFlexChild<State, Action>> = self
                .actions
                .into_iter()
                .map(|a| flex_item(a, 0.0).into())
                .collect();
            let actions_row = flex_row(actions)
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .gap(Length::px(f64::from(theme.density.gap)));
            children.push(flex_item(actions_row, 0.0).into());
        }

        let row = flex_row(children)
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.gap_lg)));

        Box::new(
            sized_box(row).padding(masonry::properties::Padding::from_vh(
                Length::px(f64::from(theme.density.pad_v)),
                Length::px(f64::from(theme.density.pad_h)),
            )),
        )
    }
}

#[cfg(test)]
mod tests {
    use xilem::WidgetView;

    use super::action_row;
    use crate::Theme;
    use crate::components::alert::AlertVariant;
    use crate::components::button::button;

    #[derive(Default)]
    struct S;

    fn assert_widget_view<V: WidgetView<S, ()>>(_: &V) {}

    fn edit(theme: &Theme) -> impl WidgetView<S, ()> + use<> {
        button(|_: &mut S| {}).label("Edit").render(theme)
    }

    /// The builder composes a full row — leading dot, flexed label + summary,
    /// badge, and multiple trailing actions — into a `WidgetView` without
    /// panicking.
    #[test]
    fn builds_a_full_row() {
        let theme = Theme::default();
        let view = action_row::<S, ()>("EURUSD")
            .leading_dot(theme.palette.success)
            .secondary("spot")
            .badge("LIVE", AlertVariant::Success)
            .action(edit(&theme))
            .action(button(|_: &mut S| {}).label("Delete").render(&theme))
            .render(&theme);
        assert_widget_view(&view);
    }

    /// The minimal row — just a label — also builds.
    #[test]
    fn builds_a_bare_label_row() {
        let theme = Theme::default();
        let view = action_row::<S, ()>("Only a label").render(&theme);
        assert_widget_view(&view);
    }

    /// Actions are optional and independent of the other slots.
    #[test]
    fn builds_with_actions_but_no_dot_or_badge() {
        let theme = Theme::default();
        let view = action_row::<S, ()>("Row")
            .action(edit(&theme))
            .render(&theme);
        assert_widget_view(&view);
    }
}
