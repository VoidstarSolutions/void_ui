//! `kbd`: inline keycap chip (HTML `<kbd>` analog).
//!
//! Renders a key combo as a single raised, bordered, monospace pill with
//! platform-aware symbol mapping (⌘/Ctrl, ⌥/Alt, …). Presentation only:
//! takes a typed key spec, renders it — no key capture or binding.

pub mod view;
pub mod widget;

#[cfg(feature = "gallery")]
pub mod demo;

pub use view::{Kbd, Modifier, kbd};
