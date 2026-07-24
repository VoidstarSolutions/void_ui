//! Skeleton loading placeholder for void-ui.
//!
//! A themed shape that stands in for content while it loads, animating
//! gently to signal activity. Size and shape it to match the content it
//! replaces — text lines, image blocks, or avatar circles.
//!
//! ## Accessibility
//!
//! Each skeleton hides itself from assistive tech (`node.set_hidden()`) — a
//! placeholder shape carries no announceable content, so exposing an empty
//! generic container would only add noise. The consequence is that skeletons
//! are **silent**: unlike [`crate::spinner()`], which reports
//! `Role::ProgressIndicator`, a screen full of skeletons announces nothing,
//! so a screen-reader user is told neither that content is loading nor when
//! it arrives. The loading cue is therefore the **host's** responsibility:
//! mark the region busy (e.g. an `aria-busy` equivalent) or announce state
//! transitions through a live region while the skeletons are mounted, and
//! clear it once the real content replaces them.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::skeleton;
//!
//! // Full-width single-line placeholder (default).
//! skeleton().render(&theme);
//!
//! // A fixed image block in the secondary tone, animation off.
//! skeleton().rectangle().width(240.0).height(120.0).secondary().animated(false).render(&theme);
//!
//! // A 40px avatar circle with a shimmer sweep.
//! skeleton().circle(40.0).wave().render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{Skeleton, SkeletonAnimation, SkeletonShape, SkeletonView, skeleton};
