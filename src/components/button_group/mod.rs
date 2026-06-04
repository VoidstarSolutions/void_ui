//! Button group component — multiple buttons rendered as a connected segmented control.
//!
//! Two builders are provided, both yielding the same [`ButtonGroup`] type:
//!
//! - [`button_group`] — plain group (horizontal by default, `.vertical()` to stack).
//! - [`toggle_button_group`] — exclusive selection group; host manages the
//!   selected index and passes it in; the callback fires with the new index.

pub mod demo;
mod view;

pub use view::{ButtonGroup, button_group, toggle_button_group};
