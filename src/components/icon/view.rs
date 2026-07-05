//! Xilem view for the icon component.
//!
//! ```ignore
//! use void_ui::components::icon::{IconName, icon};
//!
//! icon(IconName::ChevronLeft).render(&theme)
//! icon(IconName::Plus).color(theme.palette.accent).size(20.0).render(&theme)
//! ```
//!
//! The host application must register [`crate::LUCIDE_FONT_BYTES`] once before
//! displaying icons (e.g. `Xilem::new_simple(...).with_font(LUCIDE_FONT_BYTES.to_vec())`).

use std::borrow::Cow;

use masonry::parley::{FontFamily, FontFamilyName};
use masonry::peniko::Color;
use xilem::WidgetView;

use crate::Theme;
use crate::label;
use lucide_icons::Icon as IconName;

/// Builder for a themed icon.
///
/// Created with [`icon`]. Returns a xilem view via [`Self::render`].
#[must_use = "Icon does nothing until rendered with .render(&theme)"]
pub struct Icon {
    pub(super) name: IconName,
    pub(super) color: Option<Color>,
    pub(super) size: Option<f32>,
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Icon {
            name,
            color: None,
            size: None,
        }
    }
}

/// Create a themed icon from a [`IconName`] variant.
///
/// The host must have registered [`crate::LUCIDE_FONT_BYTES`] before the
/// icon is rendered or it will appear as a missing-glyph box.
pub fn icon(name: impl Into<Icon>) -> Icon {
    name.into()
}

impl Icon {
    /// Override the icon color. Defaults to `palette.text`.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Override the rendered size in pixels. Defaults to `density.ui_font_size`.
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<State, Action>
    where
        State: 'static,
        Action: 'static,
    {
        let ch = char::from(self.name);
        let color = self.color.unwrap_or(theme.palette.text);
        let size = self.size.unwrap_or(theme.density.ui_font_size);
        label(String::from(ch))
            .font(FontFamily::Single(FontFamilyName::Named(Cow::Borrowed(
                "lucide",
            ))))
            .text_size(size)
            .color(color)
            .line_height(1.0)
            .render(theme)
    }
}
