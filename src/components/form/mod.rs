//! Form component — a layout container pairing labels with controls.
//!
//! There is no custom masonry widget and no view state: [`Form::render`] and
//! [`FormField::render`] compose the themed [`label`](crate::label) with
//! xilem's built-in flex containers. Presentation only — no validation.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Form, FormField, FormOrientation, form, form_field};
