//! Xilem view for the status dot component.
//!
//! A small filled circle used to show per-row/per-item status (connection
//! state, online/offline, etc.). There is no custom masonry widget:
//! [`StatusDot::render`] wraps an empty [`crate::label`] in `sized_box`
//! styling — fixed square size, solid background, and a corner radius equal
//! to half the size, which kurbo clamps into a perfect circle regardless of
//! rounding.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::status_dot;
//!
//! status_dot(theme.palette.green).render::<(), ()>(&theme);
//! status_dot(theme.palette.coral).size(10.0).render::<(), ()>(&theme)
//! # ;
//! ```

use masonry::layout::Length;
use masonry::peniko::Color;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::sized_box;

use crate::{Density, Theme, label};

/// Diameter in px when no explicit [`StatusDot::size`] override is given.
///
/// Derived from `density.control` — the same token `radio` and `slider`
/// use for their glyph diameter — using a `4/7` ratio chosen so Balanced
/// density reproduces the library's original fixed 8px default exactly
/// (`14.0 * 4.0 / 7.0 == 8.0`), matching how every other token in
/// `density.rs` was calibrated against its pre-token constant.
fn default_size(density: &Density) -> f32 {
    density.control * 4.0 / 7.0
}

/// Builder for a small themed status indicator dot.
///
/// Created with [`status_dot`]. Materialize as a xilem view via
/// [`Self::render`].
#[must_use = "StatusDot does nothing until rendered with .render(&theme)"]
pub struct StatusDot {
    color: Color,
    size: Option<f32>,
}

/// Create a status dot filled with `color`.
///
/// Defaults to a density-driven diameter (8px at Balanced density);
/// override with [`StatusDot::size`].
pub fn status_dot(color: Color) -> StatusDot {
    StatusDot { color, size: None }
}

impl StatusDot {
    /// Override the dot's diameter in px. Defaults to a density-driven
    /// value (8px at Balanced density).
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
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
        let size_px = f64::from(self.size.unwrap_or_else(|| default_size(&theme.density)));
        let size = Length::px(size_px);
        sized_box(label("").render(theme))
            .fixed_width(size)
            .fixed_height(size)
            .background_color(self.color)
            .corner_radius(Length::px(size_px / 2.0))
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{Density, default_size, status_dot};
    use crate::{Theme, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn defaults_to_no_size_override() {
        let d = status_dot(Theme::default().palette.accent);
        assert!(d.size.is_none());
    }

    #[test]
    fn size_overrides_the_default() {
        let d = status_dot(Theme::default().palette.accent).size(10.0);
        assert_eq!(d.size, Some(10.0));
    }

    #[test]
    fn default_and_sized_dots_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = status_dot(theme.palette.green)
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = status_dot(theme.palette.coral)
            .size(10.0)
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
    }

    /// Balanced density must reproduce the library's original hardcoded
    /// 8px default exactly, matching how every other token in
    /// `density.rs` was calibrated against its pre-token constant.
    #[test]
    fn balanced_density_matches_legacy_8px_default() {
        let size = default_size(&Density::balanced());
        assert!(
            (size - 8.0).abs() < f32::EPSILON,
            "expected 8.0, got {size}"
        );
    }

    /// The default scales with density like every other density-driven
    /// size in the library.
    #[test]
    fn default_size_is_monotonic_across_density_steps() {
        let compact = default_size(&Density::compact());
        let balanced = default_size(&Density::balanced());
        let airy = default_size(&Density::airy());
        assert!(
            compact < balanced,
            "compact ({compact}) must be < balanced ({balanced})"
        );
        assert!(
            balanced < airy,
            "balanced ({balanced}) must be < airy ({airy})"
        );
    }
}
