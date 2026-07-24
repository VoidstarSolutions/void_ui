//! Breadcrumb component — an ordered trail of segments joined by a themed
//! chevron separator, for app-chrome navigation (`Trade Dashboard › Trade dashboard`).
//!
//! [`Breadcrumb::render`] composes [`crate::button()`], [`crate::icon()`], and
//! [`crate::label()`] in a `flex_row`, marking the whole trail as a navigation
//! landmark and the current segment as `AriaCurrent::Page` via
//! `access_wrap::annotate` — pure composition alone can't override
//! its own accessibility node.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Breadcrumb, BreadcrumbSegment, breadcrumb, segment};
