//! Built-in named icon set — re-exports `lucide_icons::Icon` as [`IconName`].
//!
//! Every variant in [`IconName`] maps to a character in the bundled Lucide
//! font. Use [`LUCIDE_FONT_BYTES`](crate::LUCIDE_FONT_BYTES) to register the
//! font with the host application before rendering icons.

pub use lucide_icons::Icon as IconName;
