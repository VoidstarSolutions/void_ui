//! Masonry widget helpers for the icon component.

use std::borrow::Cow;

use masonry::core::{ArcStr, NewWidget, StyleProperty, Widget as _};
use masonry::parley::{FontFamily, FontFamilyName, LineHeight};
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
            // Match `Icon::render`'s `.line_height(1.0)`: the default line
            // height floats the glyph above a row's centerline (visible when an
            // icon is laid out next to text, e.g. a context-menu row). A
            // font-size-relative line height of 1.0 hugs the glyph so it
            // centers true. Keeps the view and widget build paths consistent.
            .with_style(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.0)))
            .prepare();
        lbl.properties.insert(ContentColor::new(color));
        lbl
    }
}

#[cfg(test)]
mod tests {
    use lucide_icons::Icon as IconName;

    use super::Icon;
    use crate::Theme;

    #[test]
    fn build_widget_uses_the_icons_glyph_char() {
        let theme = Theme::default();
        let nw = Icon::from(IconName::Bell).build_widget(&theme);
        assert_eq!(
            nw.widget.text().as_ref(),
            &String::from(char::from(IconName::Bell))
        );
    }

    #[test]
    fn distinct_icons_produce_distinct_glyphs() {
        let theme = Theme::default();
        let bell = Icon::from(IconName::Bell).build_widget(&theme);
        let x = Icon::from(IconName::X).build_widget(&theme);
        assert_ne!(bell.widget.text(), x.widget.text());
    }

    #[test]
    fn build_widget_does_not_panic_with_overrides() {
        let theme = Theme::default();
        let _ = Icon::from(IconName::Bell)
            .color(theme.palette.accent)
            .size(24.0)
            .build_widget(&theme);
    }
}
