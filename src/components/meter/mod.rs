//! Meter component — a themed track with a proportional, optionally
//! heat-tinted fill.
//!
//! Presentation only — no pointer/keyboard interaction, no emitted actions.
//! Renders a `0.0..=1.0` fraction as a horizontal bar: a theme-driven track
//! color and either a solid or two-stop gradient fill. `.percent_label()`
//! composes a trailing "NN%" readout derived from the fraction; any other
//! label is composed by hand alongside it (e.g. in a `flex_row`).
//!
//! ```ignore
//! use void_ui::meter;
//!
//! meter(0.72).render(&theme)
//! meter(0.42)
//!     .fill_gradient(theme.palette.green, theme.palette.coral)
//!     .percent_label()
//!     .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub(super) mod widget;

pub use view::{Meter, MeterFill, meter};
