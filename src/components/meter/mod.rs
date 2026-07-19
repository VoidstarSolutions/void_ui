//! Meter component — a themed track with a proportional, optionally
//! heat-tinted fill.
//!
//! Presentation only — no pointer/keyboard interaction, no emitted actions.
//! Renders a `0.0..=1.0` fraction as a horizontal bar: a theme-driven track
//! color and either a solid or two-stop gradient fill. No built-in label —
//! compose one alongside it (e.g. in a `flex_row`) if a value readout is
//! wanted.
//!
//! ```ignore
//! use void_ui::meter;
//!
//! meter(0.72).render(&theme)
//! meter(0.42)
//!     .fill_gradient(theme.palette.green, theme.palette.coral)
//!     .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub(super) mod widget;

pub use view::{Meter, MeterFill, MeterView, meter};
