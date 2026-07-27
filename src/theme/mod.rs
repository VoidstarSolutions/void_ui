//! Design tokens for `void_ui` components.
//!
//! A [`Theme`] bundles a [`Palette`], a [`Density`], a [`Typography`], and
//! [`Radii`]. Components read the theme they need at render time; the host
//! application owns the live `Theme` value and swaps it (dark/light,
//! density step) by replacing it in state.

mod code_palette;
pub mod color;
mod density;
mod motion;
mod palette;
mod typography;

pub use code_palette::CodePalette;
pub use color::{oklch, oklcha};
pub use density::Density;
pub use motion::Motion;
pub use palette::Palette;
pub use typography::{FontStack, Typography};

/// Corner radii — small and large surface radii, plus a `tiny` step
/// for compact form controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// Compact form controls — checkbox boxes. 3px.
    pub tiny: f32,
    /// Cards, pills, buttons. 6px.
    pub small: f32,
    /// Large surfaces, dialogs. 10px.
    pub large: f32,
}

impl Radii {
    #[must_use]
    pub const fn default_stack() -> Self {
        Self {
            tiny: 3.0,
            small: 6.0,
            large: 10.0,
        }
    }
}

impl Default for Radii {
    fn default() -> Self {
        Self::default_stack()
    }
}

/// Which palette variant a [`Theme`] is built from.
///
/// Stored as a discriminator so callers can ask "are we currently dark?"
/// without comparing palette structs. The variant carries no behavior of
/// its own — the palette field is the source of truth for colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeVariant {
    /// The dark theme variant.
    #[default]
    Dark,
    /// The light theme variant.
    Light,
}

/// All tokens for one theme variant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub variant: ThemeVariant,
    pub palette: Palette,
    pub density: Density,
    pub typography: Typography,
    pub radius: Radii,
    pub code: CodePalette,
    pub motion: Motion,
}

impl Theme {
    /// The dark variant at balanced density.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            variant: ThemeVariant::Dark,
            palette: Palette::dark(),
            density: Density::balanced(),
            typography: Typography::default_stack(),
            radius: Radii::default_stack(),
            code: CodePalette::dark(),
            motion: Motion::default(),
        }
    }

    /// The light variant at balanced density.
    #[must_use]
    pub fn light() -> Self {
        Self {
            variant: ThemeVariant::Light,
            palette: Palette::light(),
            density: Density::balanced(),
            typography: Typography::default_stack(),
            radius: Radii::default_stack(),
            code: CodePalette::light(),
            motion: Motion::default(),
        }
    }

    /// Replace the density step, keeping palette / typography / radii.
    #[must_use]
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// Replace the motion preference, keeping palette / density /
    /// typography / radii.
    #[must_use]
    pub fn with_motion(mut self, motion: Motion) -> Self {
        self.motion = motion;
        self
    }

    /// True when this theme is the dark variant.
    #[must_use]
    pub const fn is_dark(&self) -> bool {
        matches!(self.variant, ThemeVariant::Dark)
    }

    /// True when this theme is the light variant.
    #[must_use]
    pub const fn is_light(&self) -> bool {
        matches!(self.variant, ThemeVariant::Light)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::{Motion, Radii, Theme};

    /// Both theme variants share the default radius stack, including the
    /// `tiny` token used by compact form controls (checkbox box).
    #[test]
    fn radii_default_stack_has_tiny_token() {
        let r = Radii::default_stack();
        assert!((r.tiny - 3.0).abs() < f32::EPSILON);
        assert!((r.small - 6.0).abs() < f32::EPSILON);
        assert!((r.large - 10.0).abs() < f32::EPSILON);
        assert_eq!(Theme::dark().radius, r);
        assert_eq!(Theme::light().radius, r);
    }

    /// Both theme variants default to full motion — `Motion::full()` — so
    /// existing consumers who never touch `.with_motion(..)` keep playing
    /// decorative animation exactly as before this token existed.
    #[test]
    fn theme_defaults_to_full_motion() {
        assert_eq!(Theme::dark().motion, Motion::full());
        assert_eq!(Theme::light().motion, Motion::full());
    }
}
