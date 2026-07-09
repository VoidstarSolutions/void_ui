//! Action row component — a themed list-row layout.
//!
//! A leading status dot, a primary label flexed to fill (with an optional
//! muted inline summary), an optional trailing status badge, and zero or more
//! trailing action controls. Pure composition of existing primitives
//! (`status_dot`, `label`, `badge`, caller-supplied action views) — there is no
//! custom masonry widget; see [`ActionRow::render`].
//!
//! Fills the gap `sidebar_item` leaves: it covers a clickable nav row, but not
//! this dot + label + badge + multiple-trailing-actions shape.

mod view;

pub use view::{ActionRow, action_row};
