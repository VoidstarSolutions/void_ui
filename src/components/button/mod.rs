//! Tessera `.tb-btn` button component.
//!
//! The xilem [`Button`] builder lives in [`view`]; the masonry widget that
//! owns the pointer state machine lives in [`widget`]. The widget is
//! exposed publicly so the [`ButtonView`]'s public `Element` associated
//! type can name it without leaking a private type through the public
//! API.

mod view;
pub mod widget;

pub use view::{Button, ButtonView, button};
