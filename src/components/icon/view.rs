//! Xilem view for the icon component.
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
//!
//! The host application must register [`crate::LUCIDE_FONT_BYTES`] once before
//! displaying icons (e.g. `Xilem::new_simple(...).with_font(LUCIDE_FONT_BYTES.to_vec())`).

use std::borrow::Cow;

use masonry::parley::{FontFamily, FontFamilyName};
use masonry::peniko::Color;
use xilem::{AnyWidgetView, WidgetView};

use crate::Theme;
use crate::components::access_wrap::{self, AccessAnnotation};
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
    pub(super) decorative: bool,
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Icon {
            name,
            color: None,
            size: None,
            decorative: false,
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

/// The chevron glyph for a disclosure/expansion state: a downward chevron
/// when `open` (expanded), a rightward chevron when closed (collapsed).
///
/// The single source of truth for the open → glyph mapping shared by every
/// disclosure affordance — [`collapsible`](crate::collapsible) headers,
/// expandable [`data_grid`](crate::data_grid) rows, tree nodes — so the
/// convention can't drift between the view layer ([`disclosure_chevron`])
/// and widgets that build the chevron directly via
/// [`Icon::build_widget`](crate::Icon::build_widget).
#[must_use]
pub fn disclosure_icon(open: bool) -> IconName {
    if open {
        IconName::ChevronDown
    } else {
        IconName::ChevronRight
    }
}

/// A themed disclosure chevron view: a rotating [`icon`] that points down
/// when `open` (expanded) and right when closed (collapsed), drawn in
/// `palette.text_muted`.
///
/// Marked [`Icon::decorative`] because a disclosure chevron carries no
/// standalone semantic content of its own: the expanded/collapsed state is
/// exposed to assistive technology by the interactive control that *owns*
/// the chevron (a collapsible header, an expandable row) via
/// `aria-expanded`, not by the glyph — whose raw Lucide codepoint has no
/// meaningful pronunciation.
#[must_use = "View values do nothing unless provided to Xilem."]
pub fn disclosure_chevron<State, Action>(
    open: bool,
    theme: &Theme,
) -> impl WidgetView<State, Action> + use<State, Action>
where
    State: 'static,
    Action: 'static,
{
    icon(disclosure_icon(open))
        .color(theme.palette.text_muted)
        .decorative()
        .render(theme)
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

    /// Hide this icon from assistive tech entirely.
    ///
    /// Use for purely decorative glyphs with no semantic content of their
    /// own (e.g. a chevron used only as a visual separator) — without this,
    /// a screen reader will try to announce the icon's raw Lucide glyph
    /// codepoint, which has no meaningful pronunciation.
    pub fn decorative(mut self) -> Self {
        self.decorative = true;
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
        let glyph = label(String::from(ch))
            .font(FontFamily::Single(FontFamilyName::Named(Cow::Borrowed(
                "lucide",
            ))))
            .text_size(size)
            .color(color)
            .line_height(1.0)
            .render(theme);
        if self.decorative {
            Box::new(access_wrap::annotate(glyph, AccessAnnotation::Hidden))
                as Box<AnyWidgetView<State, Action>>
        } else {
            Box::new(glyph) as Box<AnyWidgetView<State, Action>>
        }
    }
}

#[cfg(test)]
mod tests {
    use xilem::WidgetView;

    use super::{IconName, disclosure_chevron, disclosure_icon, icon};
    use crate::Theme;

    /// The disclosure mapping is the single source of truth shared by the
    /// collapsible header (widget layer) and expandable data-grid rows (view
    /// layer): open ⇒ downward chevron, closed ⇒ rightward chevron.
    #[test]
    fn disclosure_icon_points_down_when_open_right_when_closed() {
        // `IconName` (lucide's `Icon`) isn't `PartialEq`; its glyph char is
        // its observable identity, so compare those.
        assert_eq!(
            char::from(disclosure_icon(true)),
            char::from(IconName::ChevronDown)
        );
        assert_eq!(
            char::from(disclosure_icon(false)),
            char::from(IconName::ChevronRight)
        );
    }

    /// Smoke-test that the view helper materializes a `WidgetView` for both
    /// states without panicking (exercises the `icon(..).decorative().render`
    /// wiring the tree-column cell relies on).
    #[test]
    fn disclosure_chevron_builds_a_view() {
        fn assert_widget_view<V: WidgetView<(), ()>>(_: &V) {}
        let theme = Theme::default();
        assert_widget_view(&disclosure_chevron::<(), ()>(true, &theme));
        assert_widget_view(&disclosure_chevron::<(), ()>(false, &theme));
    }

    #[test]
    fn defaults_to_no_color_or_size_override_and_not_decorative() {
        let i = icon(IconName::Bell);
        assert!(i.color.is_none());
        assert!(i.size.is_none());
        assert!(!i.decorative);
    }

    #[test]
    fn builder_methods_set_the_expected_fields() {
        let theme = Theme::default();
        let i = icon(IconName::Bell)
            .color(theme.palette.accent)
            .size(20.0)
            .decorative();
        assert_eq!(i.color, Some(theme.palette.accent));
        assert_eq!(i.size, Some(20.0));
        assert!(i.decorative);
    }

    #[test]
    fn plain_and_decorative_icons_build_a_view() {
        fn assert_widget_view<V: WidgetView<(), ()>>(_: &V) {}
        let theme = Theme::default();
        assert_widget_view(&icon(IconName::Bell).render::<(), ()>(&theme));
        assert_widget_view(&icon(IconName::Bell).decorative().render::<(), ()>(&theme));
    }
}
