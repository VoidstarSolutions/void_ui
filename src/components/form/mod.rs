//! Form component — a layout container pairing labels with controls.
//!
//! There is no custom masonry widget and no view state: [`Form::render`] and
//! [`FormField::render`] compose the themed [`label`](crate::label) with
//! xilem's built-in flex containers. Presentation only — no validation.

// `demo` is added once a later task creates `src/components/form/demo.rs`.
// `gallery` is a default-on feature, so declaring `pub mod demo;` here before
// that file exists would break `cargo build`/`cargo test --lib`.
mod view;

pub use view::{Form, FormField, FormOrientation, form, form_field};
