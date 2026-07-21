//! Hover-driven tooltip component.
//!
//! Wraps any child view with a hover-idle delay that pops themed tooltip
//! content — any view, not just text — anchored near the cursor (or, for
//! keyboard users, the child's bottom-left corner). Popup content is
//! mounted through the outermost `overlay_scope`'s portal, the same
//! mechanism `dialog` uses — an `overlay_scope` ancestor is required.
//!
//! ```ignore
//! use void_ui::components::{button, label, tooltip};
//! use void_ui::overlay_scope;
//!
//! // tooltip requires an `overlay_scope` ancestor; wrap the app root once.
//! overlay_scope(
//!     tooltip(
//!         label("Reset the chart to defaults").render(&theme),
//!         button(|_: &mut State| {}).label("Reset").render(&theme),
//!     )
//!     .render(&theme),
//! )
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{DEFAULT_DELAY_MS, Tooltip, TooltipView, tooltip};
