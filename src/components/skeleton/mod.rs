//! Skeleton loading placeholder for void-ui.
//!
//! A themed shape that stands in for content while it loads, animating
//! gently to signal activity. Size and shape it to match the content it
//! replaces — text lines, image blocks, or avatar circles.
//!
//! ```ignore
//! use void_ui::skeleton;
//!
//! // Full-width single-line placeholder (default).
//! skeleton().render(&theme)
//!
//! // A fixed image block in the secondary tone, animation off.
//! skeleton().rectangle().width(240.0).height(120.0).secondary().animated(false).render(&theme)
//!
//! // A 40px avatar circle with a shimmer sweep.
//! skeleton().circle(40.0).wave().render(&theme)
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
pub(super) mod widget;

pub use view::{Skeleton, SkeletonAnimation, SkeletonShape, SkeletonView, skeleton};
