//! Icon component for void-ui.
//!
//! Renders any [`IconName`] (= `lucide_icons::Icon`) as a character in the
//! bundled Lucide font. The host application must register the font once at
//! startup:
//!
//! ```ignore
//! use void_ui::LUCIDE_FONT_BYTES;
//!
//! let app = Xilem::new_simple(state, logic, options)
//!     .with_font(LUCIDE_FONT_BYTES.to_vec());
//! ```
//!
//! Then render icons anywhere in the view tree:
//!
//! ```ignore
//! icon(IconName::ChevronLeft).render(&theme)
//! icon(IconName::Plus).color(theme.palette.teal).size(20.0).render(&theme)
//! ```

pub mod demo;
mod names;
mod view;

pub use names::IconName;
pub use view::{Icon, icon};
