//! Masonry widget helpers for the icon component.

use std::borrow::Cow;

use masonry::core::{ArcStr, NewWidget, StyleProperty, Widget as _};
use masonry::parley::{FontFamily, FontFamilyName};
use masonry::properties::ContentColor;
use masonry::widgets::Label;

use super::view::Icon;
use crate::Theme;

impl Icon {
    /// Produce a masonry [`Label`] widget for embedding as a child in another widget.
    ///
    /// Use this when building widgets that host an icon child directly (buttons,
    /// sidebar strips, etc.) instead of composing at the xilem view layer.
    #[must_use]
    pub fn build_widget(self, theme: &Theme) -> NewWidget<Label> {
        let ch = char::from(self.name);
        let color = self.color.unwrap_or(theme.palette.text);
        let size = self.size.unwrap_or(theme.density.ui_font_size);
        let mut lbl = Label::new(ArcStr::from(String::from(ch)))
            .with_style(StyleProperty::FontSize(size))
            .with_style(StyleProperty::FontFamily(FontFamily::Single(
                FontFamilyName::Named(Cow::Borrowed("lucide")),
            )))
            .prepare();
        lbl.properties.insert(ContentColor::new(color));
        lbl
    }
}
