//! `overlap_column` — stack two children vertically while decoupling their
//! *visual* order from their *paint* order.
//!
//! Two pieces:
//!
//! - [`widget::OverlapColumn`] is the masonry widget that owns the layout
//!   math and the (reversed) registration order that makes paint order win.
//! - [`view::OverlapColumn`] is the xilem `View` that wraps it; the
//!   public-facing API uses [`view::overlap_column`] as the entry point.
//!
//! The widget module is `pub` because the xilem View references the widget
//! as its `Element` type — the type-system reachability rule forces the
//! path to be public.

mod view;
pub mod widget;

pub use view::{OverlapColumn, overlap_column};
