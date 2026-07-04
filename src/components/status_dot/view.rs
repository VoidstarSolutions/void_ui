//! Xilem view for the status dot component.
//!
//! A small filled circle used to show per-row/per-item status (connection
//! state, online/offline, etc.). There is no custom masonry widget:
//! [`StatusDot::render`] wraps an empty [`crate::label`] in `sized_box`
//! styling — fixed square size, solid background, and a corner radius equal
//! to half the size, which kurbo clamps into a perfect circle regardless of
//! rounding.
//!
//! ```ignore
//! use void_ui::status_dot;
//!
//! status_dot(theme.palette.green).render(&theme)
//! status_dot(theme.palette.coral).size(10.0).render(&theme)
//! ```

use masonry::layout::Length;
use masonry::peniko::Color;
use xilem::WidgetView;
use xilem::style::Style as _;
use xilem::view::sized_box;

use crate::{Theme, label};

/// Default diameter in px, matching the size `citadel-ui`'s hand-rolled
/// version used at every call site.
const DEFAULT_SIZE: f32 = 8.0;

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
/// Defaults to an 8px diameter; override with [`StatusDot::size`].
pub fn status_dot(color: Color) -> StatusDot {
    StatusDot { color, size: None }
}

impl StatusDot {
    /// Override the dot's diameter in px. Defaults to `8.0`.
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
        let size_px = f64::from(self.size.unwrap_or(DEFAULT_SIZE));
        let size = Length::px(size_px);
        sized_box(label("").render(theme))
            .fixed_width(size)
            .fixed_height(size)
            .background_color(self.color)
            .corner_radius(Length::px(size_px / 2.0))
    }
}
