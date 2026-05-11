//! `flex_wrap` — pack items left-to-right and wrap to a new row when they
//! overflow the container's width.
//!
//! The masonry-side widget that owns the layout math is
//! [`widget::FlexWrap`] — exposed publicly because the xilem `View`
//! (added in a follow-up commit) will reference it as its `Element`.

pub mod widget;

pub use widget::FlexWrap;
