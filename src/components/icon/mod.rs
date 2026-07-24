//! Icon component for void-ui.
//!
//! Renders any [`IconName`] (= `lucide_icons::Icon`) as a character in the
//! bundled Lucide font. The host application must register the font once at
//! startup:
//!
//! ```text
//! use void_ui::LUCIDE_FONT_BYTES;
//!
//! let app = Xilem::new_simple(state, logic, options)
//!     .with_font(LUCIDE_FONT_BYTES.to_vec());
//! ```
//!
//! Then render icons anywhere in the view tree:
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::components::icon::{IconName, icon};
//!
//! icon(IconName::ChevronLeft).render::<(), ()>(&theme);
//! icon(IconName::Plus).color(theme.palette.accent).size(20.0).render::<(), ()>(&theme)
//! # ;
//! ```

#[cfg(feature = "gallery")]
pub mod demo;
mod view;
mod widget;

pub use lucide_icons::Icon as IconName;
pub use view::{Icon, disclosure_chevron, disclosure_icon, icon};
