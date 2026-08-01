//! Xilem view for the badge/pill component.
//!
//! A small inline chip: themed background, optional semantic accent, an
//! optional leading icon, an optional trailing dismiss (X) button, and either
//! a slightly rounded or fully capsule-shaped outline. There is no custom
//! masonry widget — [`Badge::render`] composes [`crate::label`] (with an
//! optional [`crate::icon`] and dismiss [`crate::button`]) inside a
//! `flex_row` within a `sized_box`, reusing [`AlertVariant`]'s color mapping
//! so a badge's semantics match an alert's.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::{badge, pill};
//! use void_ui::components::AlertVariant;
//!
//! badge("Draft").render::<(), ()>(&theme);
//! pill("Active").variant(AlertVariant::Success).render::<(), ()>(&theme)
//! # ;
//! ```

use masonry::core::ArcStr;
use masonry::layout::Length;
use masonry::properties::Padding;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row, sized_box};

use crate::{AlertVariant, ButtonVariant, IconName, Theme, button, icon, label};

/// Shape of a [`Badge`]'s outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Shape {
    /// `theme.radius.small` corners.
    #[default]
    Rounded,
    /// Radius large enough that kurbo clamps it to a full capsule.
    Pill,
}

/// Optional dismiss callback for a [`Badge`].
///
/// Implemented for `()` (no dismiss button) and for any
/// `Fn(&mut State) -> Action`, so [`Badge::on_dismiss`] is optional without
/// boxing the callback. Mirrors `alert`'s `CloseCallback`.
pub trait DismissCallback<State, Action>: Send + Sync + 'static {
    /// Whether this callback should produce a dismiss button.
    #[must_use]
    fn enabled() -> bool {
        false
    }

    /// Invoke the callback. Only called when [`Self::enabled`] is `true`.
    fn call(&self, _state: &mut State) -> Action {
        unreachable!("DismissCallback::call on a disabled callback")
    }
}

impl<State: 'static, Action: 'static> DismissCallback<State, Action> for () {}

impl<State, Action, F> DismissCallback<State, Action> for F
where
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    fn enabled() -> bool {
        true
    }

    fn call(&self, state: &mut State) -> Action {
        self(state)
    }
}

/// Builder for an inline chip.
///
/// Created with [`badge`] or [`pill`]. Materialize as a xilem view via
/// [`Self::render`].
#[must_use = "Badge does nothing until rendered with .render(&theme)"]
pub struct Badge<C = ()> {
    text: ArcStr,
    variant: AlertVariant,
    shape: Shape,
    icon: Option<IconName>,
    on_dismiss: C,
}

/// Create a badge with slightly rounded corners.
///
/// Defaults to [`AlertVariant::Default`] (neutral tint, no accent color),
/// no leading icon, and no dismiss button.
pub fn badge(text: impl Into<ArcStr>) -> Badge {
    Badge {
        text: text.into(),
        variant: AlertVariant::Default,
        shape: Shape::Rounded,
        icon: None,
        on_dismiss: (),
    }
}

/// Create a badge with fully capsule-shaped (pill) corners.
///
/// Defaults to [`AlertVariant::Default`] (neutral tint, no accent color),
/// no leading icon, and no dismiss button.
pub fn pill(text: impl Into<ArcStr>) -> Badge {
    Badge {
        text: text.into(),
        variant: AlertVariant::Default,
        shape: Shape::Pill,
        icon: None,
        on_dismiss: (),
    }
}

impl<C> Badge<C> {
    /// Set the semantic color variant. Defaults to [`AlertVariant::Default`]
    /// (a neutral tint, not an accent color).
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set an optional leading icon, tinted to the variant foreground.
    pub fn icon(mut self, name: IconName) -> Self {
        self.icon = Some(name);
        self
    }

    /// Show a trailing dismiss (X) button that invokes `on_dismiss` when
    /// clicked.
    pub fn on_dismiss<F>(self, on_dismiss: F) -> Badge<F> {
        Badge {
            text: self.text,
            variant: self.variant,
            shape: self.shape,
            icon: self.icon,
            on_dismiss,
        }
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<C, State, Action>
    where
        State: 'static,
        Action: 'static,
        C: DismissCallback<State, Action>,
    {
        let (fg, bg, border) = self.variant.colors(&theme.palette);
        let radius = match self.shape {
            Shape::Rounded => Length::px(f64::from(theme.radius.small)),
            Shape::Pill => Length::px(999.0),
        };

        let icon_view = self.icon.map(|name| {
            icon(name)
                .color(fg)
                .size(theme.typography.size_body)
                .decorative()
                .render(theme)
        });

        // Latin text under `CrossAxisAlignment::Center` sits slightly low
        // because parley leaves line-leading above the glyph, whereas the
        // leading/dismiss icon glyphs are em-centered. A small bottom padding
        // on the label raises its glyph to sit level with the icons.
        let text = sized_box(
            label(self.text)
                .text_size(theme.typography.size_body)
                .color(fg)
                .render(theme),
        )
        .padding(Padding::bottom(Length::px(1.0)));

        let on_dismiss = self.on_dismiss;
        let dismiss_view = C::enabled().then(|| {
            button(move |state: &mut State| on_dismiss.call(state))
                .icon(IconName::X)
                .variant(ButtonVariant::Text)
                .tint(fg)
                .accessible_name("Dismiss")
                .compact(true)
                .render(theme)
        });

        let row = flex_row((icon_view, text, dismiss_view))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.pad) * 0.5));

        sized_box(row)
            .padding(Padding::from_vh(
                Length::px(f64::from(theme.density.button_pad_v)),
                Length::px(f64::from(theme.density.button_pad_h)),
            ))
            .background_color(bg)
            .border(border, Length::px(1.0))
            .corner_radius(radius)
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{Shape, badge, pill};
    use crate::{AlertVariant, IconName, Theme, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn badge_defaults_to_default_variant_and_rounded_shape() {
        let b = badge("Draft");
        assert_eq!(b.variant, AlertVariant::Default);
        assert_eq!(b.shape, Shape::Rounded);
    }

    #[test]
    fn pill_defaults_to_pill_shape() {
        let p = pill("Active");
        assert_eq!(p.variant, AlertVariant::Default);
        assert_eq!(p.shape, Shape::Pill);
    }

    #[test]
    fn variant_overrides_the_default() {
        let b = badge("x").variant(AlertVariant::Success);
        assert_eq!(b.variant, AlertVariant::Success);
    }

    #[test]
    fn icon_defaults_to_none_and_icon_sets_it() {
        let b = badge("x");
        assert!(b.icon.is_none());
        // IconName (lucide's Icon) is not PartialEq — match the variant instead.
        let b = badge("x").icon(IconName::Info);
        assert!(matches!(b.icon, Some(IconName::Info)));
    }

    #[test]
    fn badge_and_pill_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = badge("Draft")
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = pill("Active")
            .variant(AlertVariant::Success)
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
    }

    #[test]
    fn badge_with_icon_and_dismiss_builds() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = badge("Tag")
            .variant(AlertVariant::Info)
            .icon(IconName::Info)
            .on_dismiss(|_: &mut AppState| {})
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = pill("Done")
            .variant(AlertVariant::Success)
            .icon(IconName::CheckCircle)
            .on_dismiss(|_: &mut AppState| {})
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
    }
}
