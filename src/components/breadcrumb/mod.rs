//! Breadcrumb component — an ordered trail of segments joined by a themed
//! chevron separator, for app-chrome navigation (`Trade Dashboard › Trade dashboard`).
//!
//! [`Breadcrumb::render`] composes [`crate::button`], [`crate::icon`], and
//! [`crate::label`] in a `flex_row`, wrapped in a minimal masonry widget
//! ([`widget::BreadcrumbNav`]) whose only job is to report
//! `Role::Navigation` to assistive tech — pure composition alone can't
//! override its own accessibility role.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Breadcrumb, BreadcrumbSegment, breadcrumb, segment};
