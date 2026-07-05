//! Xilem view for the alert component.
//!
//! A themed message card with an optional leading icon, optional title,
//! body message, and optional close button. There is no custom masonry
//! widget — [`Alert::render`] composes [`crate::label`], [`crate::icon`],
//! and [`crate::button`] inside a [`sized_box`], following the
//! pure-composition pattern used by `button_group`.
//!
//! ```ignore
//! use void_ui::components::alert::{alert, AlertVariant};
//!
//! // Plain message
//! alert("This is a notification.").render(&theme)
//!
//! // Typed, with title and a close button
//! alert("There was a problem with your request.")
//!     .variant(AlertVariant::Error)
//!     .title("Uh oh! Something went wrong.")
//!     .on_close(|s: &mut State| s.dismiss())
//!     .render(&theme)
//! ```

use masonry::core::ArcStr;
use xilem::WidgetView;
use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, MainAxisAlignment, flex_col, flex_row, sized_box,
};

use crate::theme::Palette;
use crate::{ButtonVariant, IconName, Theme, button, icon, label};

/// Visual style and default icon for an [`Alert`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertVariant {
    /// Neutral message — no accent color, no default icon.
    #[default]
    Default,
    /// Informational — blue accent, info icon.
    Info,
    /// Positive confirmation — green accent, check icon.
    Success,
    /// Caution — amber accent, triangle-alert icon.
    Warning,
    /// Error — coral accent, circle-x icon.
    Error,
}

impl AlertVariant {
    /// Foreground / background / border colors for this variant.
    ///
    /// `pub(crate)` so other components (e.g. `badge`) can share the same
    /// semantic color mapping instead of duplicating it.
    pub(crate) fn colors(self, palette: &Palette) -> (Color, Color, Color) {
        match self {
            Self::Default => (palette.text, palette.surface, palette.border),
            Self::Info => (palette.info, palette.info_soft, palette.info),
            Self::Success => (palette.success, palette.success_soft, palette.success),
            Self::Warning => (palette.warning, palette.warning_soft, palette.warning),
            Self::Error => (palette.danger, palette.danger_soft, palette.danger),
        }
    }

    /// Default leading icon for this variant, if any.
    fn default_icon(self) -> Option<IconName> {
        match self {
            Self::Default => None,
            Self::Info => Some(IconName::Info),
            Self::Success => Some(IconName::CheckCircle),
            Self::Warning => Some(IconName::AlertTriangle),
            Self::Error => Some(IconName::CircleX),
        }
    }
}

/// Implemented by `()` (no close button) and by `Fn(&mut State) -> Action`
/// closures, so [`Alert::on_close`] is optional without boxing the callback.
pub trait CloseCallback<State, Action>: Send + Sync + 'static {
    /// Whether this callback should produce a close button.
    #[must_use]
    fn enabled() -> bool {
        false
    }

    /// Invoke the callback. Only called when [`Self::enabled`] is `true`.
    fn call(&self, _state: &mut State) -> Action {
        unreachable!("CloseCallback::call on a disabled callback")
    }
}

impl<State: 'static, Action: 'static> CloseCallback<State, Action> for () {}

impl<State, Action, F> CloseCallback<State, Action> for F
where
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    fn enabled() -> bool {
        true
    }

    fn call(&self, state: &mut State) -> Action {
        self(state)
    }
}

/// Builder for an alert message card.
///
/// Created with [`alert`]. Configure with builder methods; materialize as a
/// xilem view via [`Self::render`].
#[must_use = "Alert does nothing until rendered with .render(&theme)"]
pub struct Alert<C = ()> {
    message: ArcStr,
    title: Option<ArcStr>,
    variant: AlertVariant,
    icon: Option<IconName>,
    show_icon: bool,
    banner: bool,
    on_close: C,
}

/// Create an alert with the given message.
///
/// Defaults to [`AlertVariant::Default`] (no accent color, no icon).
pub fn alert(message: impl Into<ArcStr>) -> Alert {
    Alert {
        message: message.into(),
        title: None,
        variant: AlertVariant::Default,
        icon: None,
        show_icon: true,
        banner: false,
        on_close: (),
    }
}

impl<C> Alert<C> {
    /// Set the visual style variant. Also selects the default leading icon
    /// for that variant (override with [`Self::icon`] or suppress with
    /// [`Self::no_icon`]).
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Set a title shown above the message in the variant's accent color.
    /// Ignored in [`Self::banner`] style.
    pub fn title(mut self, title: impl Into<ArcStr>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Override the leading icon. Defaults to the variant's icon, if any.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Suppress the leading icon entirely, even if the variant has a default.
    pub fn no_icon(mut self) -> Self {
        self.show_icon = false;
        self
    }

    /// Render as a full-width banner: no border, no corner radius, no title,
    /// content vertically centered.
    pub fn banner(mut self) -> Self {
        self.banner = true;
        self
    }

    /// Show a close (X) button that invokes `on_close` when clicked.
    pub fn on_close<F>(self, on_close: F) -> Alert<F> {
        Alert {
            message: self.message,
            title: self.title,
            variant: self.variant,
            icon: self.icon,
            show_icon: self.show_icon,
            banner: self.banner,
            on_close,
        }
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<C, State, Action>
    where
        State: 'static,
        Action: 'static,
        C: CloseCallback<State, Action>,
    {
        let (fg, bg, border) = self.variant.colors(&theme.palette);
        let icon_size = theme.density.ui_font_size;

        let icon_view = self
            .show_icon
            .then(|| self.icon.or_else(|| self.variant.default_icon()))
            .flatten()
            .map(|name| icon(name).color(fg).size(icon_size).render(theme));

        let title_view = (!self.banner)
            .then_some(self.title)
            .flatten()
            .map(|title| label(title).color(fg).line_height(1.0).render(theme));

        let message_color = if self.variant == AlertVariant::Default {
            theme.palette.text
        } else {
            fg
        };
        let message_view = label(self.message)
            .color(message_color)
            .multiline(true)
            .line_height(1.0)
            .render(theme);

        let content = flex_col((title_view, message_view))
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_alignment(MainAxisAlignment::Center)
            .gap(Length::px(2.0));

        let on_close = self.on_close;
        let close_view = C::enabled().then(|| {
            button(move |state: &mut State| on_close.call(state))
                .icon(IconName::X)
                .variant(ButtonVariant::Text)
                .tint(fg)
                .accessible_name("Dismiss")
                .render(theme)
        });

        let row = flex_row((icon_view, content.flex(1.0), close_view))
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .gap(Length::px(f64::from(theme.density.pad) * 0.66));

        let (border_color, border_width, corner_radius) = if self.banner {
            (bg, Length::ZERO, Length::ZERO)
        } else {
            (
                border,
                Length::px(1.0),
                Length::px(f64::from(theme.radius.small)),
            )
        };

        sized_box(row)
            .padding(Length::px(f64::from(theme.density.pad)))
            .background_color(bg)
            .border(border_color, border_width)
            .corner_radius(corner_radius)
    }
}

#[cfg(test)]
mod tests {
    use super::{AlertVariant, IconName};
    use crate::Theme;

    /// [`IconName`] (= `lucide_icons::Icon`) doesn't implement `PartialEq`,
    /// but does convert to a unique `char`, so compare icons via that.
    fn icon_char(icon: Option<IconName>) -> Option<char> {
        icon.map(char::from)
    }

    #[test]
    fn default_variant_uses_text_surface_border_and_no_icon() {
        let theme = Theme::dark();
        let p = &theme.palette;
        assert_eq!(
            AlertVariant::Default.colors(p),
            (p.text, p.surface, p.border)
        );
        assert_eq!(icon_char(AlertVariant::Default.default_icon()), None);
    }

    #[test]
    fn typed_variants_use_their_accent_color_and_default_icon() {
        for theme in [Theme::dark(), Theme::light()] {
            let p = &theme.palette;
            assert_eq!(AlertVariant::Info.colors(p), (p.info, p.info_soft, p.info));
            assert_eq!(
                icon_char(AlertVariant::Info.default_icon()),
                icon_char(Some(IconName::Info))
            );

            assert_eq!(
                AlertVariant::Success.colors(p),
                (p.success, p.success_soft, p.success)
            );
            assert_eq!(
                icon_char(AlertVariant::Success.default_icon()),
                icon_char(Some(IconName::CheckCircle))
            );

            assert_eq!(
                AlertVariant::Warning.colors(p),
                (p.warning, p.warning_soft, p.warning)
            );
            assert_eq!(
                icon_char(AlertVariant::Warning.default_icon()),
                icon_char(Some(IconName::AlertTriangle))
            );

            assert_eq!(
                AlertVariant::Error.colors(p),
                (p.danger, p.danger_soft, p.danger)
            );
            assert_eq!(
                icon_char(AlertVariant::Error.default_icon()),
                icon_char(Some(IconName::CircleX))
            );
        }
    }
}
