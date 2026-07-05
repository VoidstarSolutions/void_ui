//! Breadcrumb component — an ordered trail of segments joined by a themed
//! chevron separator, for app-chrome navigation (`Trade Dashboard › Trade dashboard`).
//!
//! There is no custom masonry widget and no view state: [`Breadcrumb::render`]
//! composes [`crate::button`], [`crate::icon`], and [`crate::label`] in a
//! `flex_row`.

#[cfg(feature = "gallery")]
pub mod demo;
mod view;

pub use view::{Breadcrumb, BreadcrumbSegment, breadcrumb, segment};
