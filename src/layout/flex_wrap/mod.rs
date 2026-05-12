//! `flex_wrap` — pack items left-to-right and wrap to a new row when they
//! overflow the container's width.
//!
//! Two pieces:
//!
//! - [`widget::FlexWrap`] is the masonry widget that owns the layout
//!   math (row partitioning, alignment within rows).
//! - [`view::FlexWrap`] is the xilem `View` that wraps it; the
//!   public-facing API uses [`view::flex_wrap`] as the entry point.
//!
//! The widget module is `pub` because the xilem View references the
//! widget as its `Element` type — the type-system reachability rule
//! forces the path to be public.

mod view;
pub mod widget;

pub use view::{FlexWrap, FlexWrapElement, FlexWrapSequence, flex_wrap};
