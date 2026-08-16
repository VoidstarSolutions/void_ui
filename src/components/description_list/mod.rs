//! Description list component for void-ui.
//!
//! An ordered set of label/value pairs — a themed `<dl>` analog. In the default
//! horizontal layout, values align in a shared column sized to the widest label;
//! in stacked layout each value sits below its label.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::description_list;
//!
//! description_list::<(), ()>()
//!     .item("Name", void_ui::label("Ada Lovelace").render(&theme))
//!     .item("Role", void_ui::label("Mathematician").render(&theme))
//!     .render(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use view::{
    DescriptionList, DescriptionListOrientation, DescriptionListView, description_list,
};
