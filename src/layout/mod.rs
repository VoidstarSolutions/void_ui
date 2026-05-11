//! Composition primitives that arrange child widgets.
//!
//! Layout widgets in this module are *not* user-facing components — they
//! exist so that the visible components in [`crate::components`] (and
//! downstream apps) can arrange their children with patterns that aren't
//! covered by masonry's stock `Flex` and `Grid`. Each layout widget
//! follows the same folder shape as a component:
//!
//! ```text
//! layout/
//!   flex_wrap/
//!     mod.rs    re-exports public API
//!     view.rs   xilem View
//!     widget.rs masonry widget
//! ```
//!
//! ## Layout vs component
//!
//! - A *component* is something a user sees and interacts with — a
//!   `Button`, `Chip`, `Card`. Each is listed in
//!   [`crate::components::ComponentKind`] and gets a sidebar entry in
//!   the gallery.
//! - A *layout* is a composition primitive — `flex_wrap`, `auto_grid`,
//!   `split`. Layouts have no "selected state", no callbacks of their
//!   own; they just position their children. They don't appear in
//!   `ComponentKind`. They can still have demos in the gallery, but
//!   under a separate "Layouts" section (added when we have enough to
//!   justify it).
//!
//! Both kinds can use each other: a `Card` component may compose
//! `flex_wrap` internally; a `flex_wrap` demo may use `Button` as its
//! sample children.
