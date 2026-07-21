//! Autocomplete component — text input with a filtered suggestion list.
//!
//! The list opens below the field when the user's text matches at least one
//! candidate (case-insensitive prefix match). Arrow keys navigate, Enter or a
//! click selects, and Escape or focus-loss closes the overlay.
//!
//! ```ignore
//! use void_ui::components::autocomplete::autocomplete;
//!
//! autocomplete(state.city.clone(), |s: &mut State, text| s.city = text)
//!     .suggestions(["New York", "Los Angeles", "Chicago"])
//!     .placeholder("Enter city…")
//!     .render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub(crate) mod widget;

pub use view::{Autocomplete, AutocompleteView, autocomplete};
pub use widget::AutocompleteAction;
