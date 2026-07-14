//! Xilem view for the group box component.
//!
//! A titled container that visually groups related content. There is no
//! custom masonry widget and no view state: [`GroupBox::render`] composes
//! the existing themed [`label`] with `sized_box` styling around the child
//! and returns the type-erased view directly.
//!
//! ```ignore
//! use void_ui::group_box;
//!
//! group_box(checkbox("Enable feature", |s: &mut State| s.enabled = !s.enabled)
//!     .checked(state.enabled)
//!     .render(&theme))
//!     .title("Settings")
//!     .render(&theme)
//!
//! group_box(content).title("Section").border().render(&theme)
//! ```

use masonry::core::ArcStr;
use masonry::peniko::Color;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, sized_box};
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::label;

/// Visual treatment of the content area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum GroupBoxVariant {
    /// No background or border — just spacing around the content.
    #[default]
    Normal,
    /// Solid `surface` background with rounded corners.
    Fill,
    /// Bordered outline with rounded corners.
    Outline,
}

/// Title state of a [`GroupBox`] that has no title.
///
/// In this state [`GroupBox::title_color`] is not available, since there is
/// no title for it to color.
#[derive(Debug)]
pub struct NoTitle;

/// Title state of a [`GroupBox`] that has a title, with an optional color
/// override.
#[derive(Debug)]
pub struct WithTitle {
    text: ArcStr,
    color: Option<Color>,
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::NoTitle {}
    impl Sealed for super::WithTitle {}
}

/// The title state of a [`GroupBox`].
///
/// Implemented by [`NoTitle`] and [`WithTitle`]. This trait is sealed and has
/// no public members; it exists only to let [`GroupBox::render`] be written
/// once for both states.
pub trait TitleState: sealed::Sealed {
    #[doc(hidden)]
    fn into_label(self) -> Option<(ArcStr, Option<Color>)>;
}

impl TitleState for NoTitle {
    fn into_label(self) -> Option<(ArcStr, Option<Color>)> {
        None
    }
}

impl TitleState for WithTitle {
    fn into_label(self) -> Option<(ArcStr, Option<Color>)> {
        Some((self.text, self.color))
    }
}

/// Builder for a titled grouping container.
///
/// Created with [`group_box`]. Returns a view via [`Self::render`].
#[must_use = "GroupBox does nothing until rendered with .render(&theme)"]
pub struct GroupBox<V, T = NoTitle> {
    title: T,
    background: Option<Color>,
    variant: GroupBoxVariant,
    child: V,
}

/// Wrap `child` in a group box.
///
/// Defaults to no background/border and no title.
pub fn group_box<V>(child: V) -> GroupBox<V> {
    GroupBox {
        title: NoTitle,
        background: None,
        variant: GroupBoxVariant::default(),
        child,
    }
}

impl<V> GroupBox<V, NoTitle> {
    /// Add a title above the content area.
    pub fn title(self, text: impl Into<ArcStr>) -> GroupBox<V, WithTitle> {
        GroupBox {
            title: WithTitle {
                text: text.into(),
                color: None,
            },
            background: self.background,
            variant: self.variant,
            child: self.child,
        }
    }
}

impl<V> GroupBox<V, WithTitle> {
    /// Override the title's text color. Defaults to `palette.text_muted`.
    pub fn title_color(mut self, color: Color) -> Self {
        self.title.color = Some(color);
        self
    }
}

impl<V, T> GroupBox<V, T> {
    /// Use a solid `surface` background with rounded corners.
    pub fn fill(mut self) -> Self {
        self.variant = GroupBoxVariant::Fill;
        self
    }

    /// Use a bordered outline with rounded corners.
    pub fn border(mut self) -> Self {
        self.variant = GroupBoxVariant::Outline;
        self
    }

    /// Deprecated alias of [`Self::border`].
    #[deprecated(
        since = "0.1.0",
        note = "renamed to `border` to match card's vocabulary"
    )]
    pub fn outline(self) -> Self {
        self.border()
    }

    /// Override the content area's background color.
    ///
    /// Applies regardless of variant: it fills behind the default
    /// (no-background) variant, replaces the default `surface` fill set by
    /// [`Self::fill`], and adds a fill behind the border drawn by
    /// [`Self::border`].
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
}

impl<V, T: TitleState> GroupBox<V, T> {
    /// Materialize a view at the supplied theme.
    #[must_use]
    pub fn render<State, Action>(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>>
    where
        State: 'static,
        Action: 'static,
        V: WidgetView<State, Action>,
    {
        let pad = Length::px(f64::from(theme.density.pad));
        let radius = Length::px(f64::from(theme.radius.small));

        let (background, border) =
            resolve_style(self.variant, self.background, theme.palette.surface);

        // Background and corner radius are applied unconditionally: a
        // transparent fill and a radius with nothing to clip are both
        // visual no-ops, so the only branch left is the border.
        let base = sized_box(self.child)
            .padding(pad)
            .background_color(background.unwrap_or(Color::TRANSPARENT))
            .corner_radius(radius);

        let content: Box<AnyWidgetView<State, Action>> = if border {
            Box::new(base.border(theme.palette.border, Length::px(1.0)))
        } else {
            Box::new(base)
        };

        match self.title.into_label() {
            Some((title, title_color)) => {
                let title_label = label(title)
                    .text_size(theme.typography.size_caption)
                    .color(title_color.unwrap_or(theme.palette.text_muted))
                    .render(theme);
                Box::new(
                    flex_col((title_label, content))
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .gap(Length::px(f64::from(theme.density.pad) / 2.0)),
                )
            }
            None => content,
        }
    }
}

/// Resolve the content area's background fill and whether it gets a border,
/// given the variant and an optional explicit background override.
fn resolve_style(
    variant: GroupBoxVariant,
    background: Option<Color>,
    surface: Color,
) -> (Option<Color>, bool) {
    let background = match (variant, background) {
        (GroupBoxVariant::Fill, None) => Some(surface),
        (_, Some(c)) => Some(c),
        _ => None,
    };
    let border = matches!(variant, GroupBoxVariant::Outline);
    (background, border)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SURFACE: Color = Color::from_rgb8(0x10, 0x20, 0x30);
    const CUSTOM: Color = Color::from_rgb8(0xAA, 0xBB, 0xCC);

    #[test]
    fn normal_without_background_has_no_fill_or_border() {
        assert_eq!(
            resolve_style(GroupBoxVariant::Normal, None, SURFACE),
            (None, false)
        );
    }

    #[test]
    fn normal_with_background_fills_without_border() {
        assert_eq!(
            resolve_style(GroupBoxVariant::Normal, Some(CUSTOM), SURFACE),
            (Some(CUSTOM), false)
        );
    }

    #[test]
    fn fill_without_background_uses_theme_surface() {
        assert_eq!(
            resolve_style(GroupBoxVariant::Fill, None, SURFACE),
            (Some(SURFACE), false)
        );
    }

    #[test]
    fn fill_with_background_overrides_theme_surface() {
        assert_eq!(
            resolve_style(GroupBoxVariant::Fill, Some(CUSTOM), SURFACE),
            (Some(CUSTOM), false)
        );
    }

    #[test]
    fn outline_without_background_has_border_and_no_fill() {
        assert_eq!(
            resolve_style(GroupBoxVariant::Outline, None, SURFACE),
            (None, true)
        );
    }

    #[test]
    fn outline_with_background_fills_and_borders() {
        assert_eq!(
            resolve_style(GroupBoxVariant::Outline, Some(CUSTOM), SURFACE),
            (Some(CUSTOM), true)
        );
    }

    /// The deprecated `.outline` shim must still compile and delegate to
    /// `.border` — a host mid-migration off the old name must see identical
    /// behavior.
    #[test]
    #[allow(deprecated)]
    fn deprecated_outline_delegates_to_border() {
        use crate::Theme;
        use crate::label;

        let theme = Theme::default();
        let _ = group_box(label("content").render::<(), ()>(&theme))
            .outline()
            .render::<(), ()>(&theme);
    }
}
