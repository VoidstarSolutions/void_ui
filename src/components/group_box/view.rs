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
//! group_box(content).title("Section").outline().render(&theme)
//! ```

use masonry::core::ArcStr;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, sized_box};
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::label;

/// Visual treatment of the content area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GroupBoxVariant {
    /// No background or border — just spacing around the content.
    #[default]
    Normal,
    /// Solid `surface` background with rounded corners.
    Fill,
    /// Bordered outline with rounded corners.
    Outline,
}

/// Builder for a titled grouping container.
///
/// Created with [`group_box`]. Returns a view via [`Self::render`].
#[must_use = "GroupBox does nothing until rendered with .render(&theme)"]
pub struct GroupBox<V> {
    title: Option<ArcStr>,
    variant: GroupBoxVariant,
    child: V,
}

/// Wrap `child` in a group box.
///
/// Defaults to [`GroupBoxVariant::Normal`] with no title.
pub fn group_box<V>(child: V) -> GroupBox<V> {
    GroupBox {
        title: None,
        variant: GroupBoxVariant::default(),
        child,
    }
}

impl<V> GroupBox<V> {
    /// Add a title above the content area.
    pub fn title(mut self, text: impl Into<ArcStr>) -> Self {
        self.title = Some(text.into());
        self
    }

    /// Set the visual treatment of the content area.
    pub fn variant(mut self, variant: GroupBoxVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Shorthand for `.variant(GroupBoxVariant::Fill)`.
    pub fn fill(mut self) -> Self {
        self.variant = GroupBoxVariant::Fill;
        self
    }

    /// Shorthand for `.variant(GroupBoxVariant::Outline)`.
    pub fn outline(mut self) -> Self {
        self.variant = GroupBoxVariant::Outline;
        self
    }

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

        let content: Box<AnyWidgetView<State, Action>> = match self.variant {
            GroupBoxVariant::Normal => Box::new(sized_box(self.child).padding(pad)),
            GroupBoxVariant::Fill => Box::new(
                sized_box(self.child)
                    .padding(pad)
                    .background_color(theme.palette.surface)
                    .corner_radius(radius),
            ),
            GroupBoxVariant::Outline => Box::new(
                sized_box(self.child)
                    .padding(pad)
                    .border(theme.palette.border, Length::px(1.0))
                    .corner_radius(radius),
            ),
        };

        match self.title {
            Some(title) => {
                let title_label = label(title)
                    .text_size(theme.typography.size_caption)
                    .color(theme.palette.text_muted)
                    .render(theme);
                Box::new(
                    flex_col((title_label, content))
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .gap(Length::px(8.0)),
                )
            }
            None => content,
        }
    }
}
