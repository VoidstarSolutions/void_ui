//! Clipboard icon button — copy a value to the system clipboard.
//!
//! Shows a copy icon at rest; switches to a checkmark for 1.5 s after the
//! user activates the button, then reverts. The value is written to the
//! system clipboard by the widget itself; the caller-supplied callback fires
//! afterward so the host can react (e.g. update UI state, show a toast) —
//! it must **not** write the clipboard again.
//!
//! ```ignore
//! use void_ui::components::clipboard;
//! clipboard("sk-proj-abc123", |s: &mut State, text: &str| {
//!     s.last_copied = Some(text.to_owned());
//! })
//! .render(&theme)
//! ```

pub mod demo;
mod view;
pub mod widget;

pub use view::{Clipboard, ClipboardView, clipboard};
