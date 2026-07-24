//! Shared overlay infrastructure: placement vocabulary, surface chrome, and
//! portal open/close plumbing used by every overlay-flavored
//! component (`popover`, `dropdown_button`, `autocomplete`, `dialog`,
//! `notification`) and by the root overlay machinery
//! ([`crate::overlay_scope()`], [`crate::overlay_portal`],
//! [`crate::anchored_overlay`]).
//!
//! Nothing here is component-specific — if a type only makes sense for one
//! component, it belongs in that component's module instead.

mod anchor;
pub(crate) mod binding;
mod surface;

pub use anchor::OverlayAnchor;
pub(crate) use surface::{OverlaySurface, SurfaceStyle};
