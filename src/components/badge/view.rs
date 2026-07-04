//! Xilem view for the badge/pill component.
//!
//! A small inline chip: themed background, optional semantic accent, and
//! either a slightly rounded or fully capsule-shaped outline. There is no
//! custom masonry widget — [`Badge::render`] composes [`crate::label`]
//! inside a `sized_box`, reusing [`AlertVariant`]'s color mapping so a
//! badge's semantics match an alert's.
//!
//! ```ignore
//! use void_ui::{badge, pill};
//! use void_ui::components::AlertVariant;
//!
//! badge("Draft").render(&theme)
//! pill("Active").variant(AlertVariant::Success).render(&theme)
//! ```

use masonry::core::ArcStr;
use masonry::layout::Length;
use masonry::properties::Padding;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::sized_box;

use crate::{AlertVariant, Theme, label};

/// Shape of a [`Badge`]'s outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Shape {
    /// `theme.radius.small` corners.
    #[default]
    Rounded,
    /// Radius large enough that kurbo clamps it to a full capsule.
    Pill,
}

/// Builder for an inline chip.
///
/// Created with [`badge`] or [`pill`]. Materialize as a xilem view via
/// [`Self::render`].
#[must_use = "Badge does nothing until rendered with .render(&theme)"]
pub struct Badge {
    text: ArcStr,
    variant: AlertVariant,
    shape: Shape,
}

/// Create a badge with slightly rounded corners.
///
/// Defaults to [`AlertVariant::Default`] (neutral tint, no accent color).
pub fn badge(text: impl Into<ArcStr>) -> Badge {
    Badge {
        text: text.into(),
        variant: AlertVariant::Default,
        shape: Shape::Rounded,
    }
}

/// Create a badge with fully capsule-shaped (pill) corners.
///
/// Defaults to [`AlertVariant::Default`] (neutral tint, no accent color).
pub fn pill(text: impl Into<ArcStr>) -> Badge {
    Badge {
        text: text.into(),
        variant: AlertVariant::Default,
        shape: Shape::Pill,
    }
}

impl Badge {
    /// Set the semantic color variant. Defaults to [`AlertVariant::Default`]
    /// (a neutral tint, not an accent color).
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<State, Action>
    where
        State: 'static,
        Action: 'static,
    {
        let (fg, bg, border) = self.variant.colors(&theme.palette);
        let radius = match self.shape {
            Shape::Rounded => Length::px(f64::from(theme.radius.small)),
            Shape::Pill => Length::px(999.0),
        };

        let text = label(self.text)
            .text_size(theme.typography.size_caption)
            .color(fg)
            .line_height(1.0)
            .render(theme);

        sized_box(text)
            .padding(Padding::from_vh(
                Length::px(f64::from(theme.density.button_pad_v)),
                Length::px(f64::from(theme.density.button_pad_h)),
            ))
            .background_color(bg)
            .border(border, Length::px(1.0))
            .corner_radius(radius)
    }
}
