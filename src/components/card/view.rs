//! Xilem view for the card component.
//!
//! A themed rounded, padded surface for floating panels (tooltips, popover
//! bodies) — a lighter-weight sibling of [`crate::group_box`] with no title
//! bar. There is no custom masonry widget: [`Card::render`] wraps the child
//! in `sized_box` styling, following the same pure-composition pattern as
//! `group_box`.
//!
//! ```ignore
//! use void_ui::card;
//!
//! card(kv_rows).render(&theme)
//! card(content).border().background(theme.palette.surface_2).render(&theme)
//! ```

use masonry::peniko::Color;
use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::sized_box;
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;

/// Builder for a themed floating surface.
///
/// Created with [`card`]. Materialize as a xilem view via [`Self::render`].
#[must_use = "Card does nothing until rendered with .render(&theme)"]
pub struct Card<V> {
    child: V,
    background: Option<Color>,
    border: bool,
}

/// Wrap `child` in a themed rounded, padded surface.
///
/// Defaults to `palette.surface` background, no border.
pub fn card<V>(child: V) -> Card<V> {
    Card {
        child,
        background: None,
        border: false,
    }
}

impl<V> Card<V> {
    /// Override the surface's background color. Defaults to `palette.surface`.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Add a `palette.border` outline.
    pub fn border(mut self) -> Self {
        self.border = true;
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
        let base = sized_box(self.child)
            .padding(Length::px(f64::from(theme.density.pad)))
            .background_color(self.background.unwrap_or(theme.palette.surface))
            .corner_radius(Length::px(f64::from(theme.radius.small)));

        if self.border {
            Box::new(base.border(theme.palette.border, Length::px(1.0)))
        } else {
            Box::new(base)
        }
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::card;
    use crate::{Theme, label, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn defaults_to_no_border_and_no_background_override() {
        let c = card(());
        assert!(!c.border);
        assert!(c.background.is_none());
    }

    #[test]
    fn border_and_background_set_the_expected_fields() {
        let theme = Theme::default();
        let c = card(()).border().background(theme.palette.accent);
        assert!(c.border);
        assert_eq!(c.background, Some(theme.palette.accent));
    }

    #[test]
    fn bordered_and_borderless_cards_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let content = label("content").render::<AppState, ()>(&theme);
        let _ = card(content)
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);

        let content = label("content").render::<AppState, ()>(&theme);
        let _ = card(content)
            .border()
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
    }
}
