//! Tessera-styled widget components built on the xilem/masonry primitives.
//!
//! Each component is a small builder that resolves to a `WidgetView` at the
//! supplied [`Theme`](crate::Theme). The pattern is intentionally explicit:
//!
//! ```ignore
//! use void_ui::components::button;
//! button("Reset view").active(false).render(&theme)
//! ```
//!
//! Theme is passed at the render boundary rather than stored on each
//! component value so that swapping themes is a single state change in
//! the host, not a tree walk.

pub mod button;

pub use button::{Button, ButtonState, button};
