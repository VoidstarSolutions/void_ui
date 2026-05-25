//! Button group component — multiple buttons rendered as a connected segmented control.
//!
//! Three builders are provided:
//!
//! - [`button_group`] — horizontal group (default).
//! - [`button_group`] with `.vertical()` — vertically stacked group.
//! - [`toggle_button_group`] — exclusive selection group; host manages the
//!   selected index and passes it in; widget fires the callback with the new index.

pub mod demo;
mod view;

pub use view::{
    ButtonGroup, ButtonGroupView, ToggleButtonGroup, ToggleButtonGroupView, button_group,
    toggle_button_group,
};
