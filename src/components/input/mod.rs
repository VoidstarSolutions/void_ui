//! Input component — a single-line, themed text field.
//!
//! Two-layer pattern: `view` holds the [`input`] builder and the xilem view
//! that themes and diffs the field on rebuild; the masonry widget is the
//! upstream `TextInput` (which owns its own `TextArea` for editing), themed
//! per-render from the [`Theme`](crate::Theme) rather than reimplemented.
//!
//! This is the foundation for the wider input family (prefix/suffix, numeric,
//! currency, masked); those build on this base rather than re-wrapping the
//! masonry widget themselves.

mod currency;
#[cfg(feature = "gallery")]
pub mod demo;
mod number;
mod view;
pub mod widget;

pub use currency::{CurrencyFormat, CurrencyInput, currency_input};
pub use number::{NumberInput, number_input};
pub use view::{Input, input};
