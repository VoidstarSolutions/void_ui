//! Tessera-derived design tokens for `void_ui` components.
//!
//! A [`Theme`] bundles a [`Palette`], a [`Density`], a [`Typography`], and
//! [`Radii`]. Components read the theme they need at render time; the host
//! application owns the live `Theme` value and swaps it (dark/light,
//! density step) by replacing it in state.
//!
//! The token names mirror the CSS custom properties in the Tessera source
//! one-to-one. When the design source moves a token, the rename is
//! mechanical here.

pub mod color;
mod density;
mod palette;
mod typography;

pub use color::{oklch, oklcha};
pub use density::Density;
pub use palette::Palette;
pub use typography::{FontStack, Typography};

/// Corner radii — Tessera ships two sizes (`--radius`, `--radius-lg`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// `--radius` — cards, pills, buttons. 6px.
    pub small: f32,
    /// `--radius-lg` — large surfaces, dialogs. 10px.
    pub large: f32,
}

impl Radii {
    #[must_use]
    pub const fn default_stack() -> Self {
        Self {
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

/// All tokens for one theme variant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub palette: Palette,
    pub density: Density,
    pub typography: Typography,
    pub radius: Radii,
}

impl Theme {
    /// Tessera's dark default at balanced density.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            palette: Palette::dark(),
            density: Density::balanced(),
            typography: Typography::default_stack(),
            radius: Radii::default_stack(),
        }
    }

    /// Tessera's light variant at balanced density.
    #[must_use]
    pub fn light() -> Self {
        Self {
            palette: Palette::light(),
            density: Density::balanced(),
            typography: Typography::default_stack(),
            radius: Radii::default_stack(),
        }
    }

    /// Replace the density step, keeping palette / typography / radii.
    #[must_use]
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
