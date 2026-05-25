//! Clipboard icon button — copy a value to the system clipboard.
//!
//! Shows a copy icon at rest; switches to a checkmark for 1.5 s after the
//! user activates the button, then reverts. The caller supplies a callback
//! that receives the value and decides how to write it to the clipboard.
//!
//! ```ignore
//! use void_ui::components::clipboard;
//! clipboard("sk-proj-abc123", |s: &mut State, text: &str| {
//!     s.write_to_clipboard(text);
//! })
//! .render(&theme)
//! ```

pub mod demo;
mod view;
pub mod widget;

pub use view::{Clipboard, ClipboardView, clipboard};
