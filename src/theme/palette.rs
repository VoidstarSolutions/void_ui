//! Color palette for `void_ui` themes.
//!
//! Field names describe role and shade (`bg`, `surface_2`, `teal_soft`)
//! rather than referencing any external stylesheet.

use masonry::peniko::Color;

use super::color::{oklch, oklcha};

/// Surface / text / accent colors for one theme variant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    // Surfaces & lines
    pub bg: Color,
    pub bg_deep: Color,
    pub surface: Color,
    pub surface_2: Color,
    pub surface_hi: Color,
    pub border: Color,
    pub border_strong: Color,
    pub grid: Color,
    pub grid_major: Color,

    // Text
    pub text: Color,
    pub text_muted: Color,
    pub text_faint: Color,

    // Accents
    pub teal: Color,
    pub teal_soft: Color,
    pub teal_deep: Color,
    pub coral: Color,
    pub coral_soft: Color,
    pub coral_deep: Color,
    pub amber: Color,
    pub amber_soft: Color,
    pub amber_deep: Color,
    pub violet: Color,
    pub violet_soft: Color,
    pub violet_deep: Color,
    pub green: Color,
    pub green_soft: Color,
    pub green_deep: Color,
    pub blue: Color,
    pub blue_soft: Color,
    pub blue_deep: Color,

    // Semantic roles — what a color *means*, decoupled from which hue
    // carries it. Components consume these; the hue fields above stay as
    // the raw ramp (and as the defaults these are drawn from). A theme may
    // diverge a role from its hue without touching any component.
    /// Primary interactive accent (selection, primary buttons, carets).
    pub accent: Color,
    /// Translucent accent fill (rest-state primary background, text selection).
    pub accent_soft: Color,
    /// Pressed/active accent.
    pub accent_deep: Color,
    /// Destructive actions and errors.
    pub danger: Color,
    /// Translucent danger fill.
    pub danger_soft: Color,
    /// Pressed danger.
    pub danger_deep: Color,
    /// Caution states.
    pub warning: Color,
    /// Translucent warning fill.
    pub warning_soft: Color,
    /// Pressed warning.
    pub warning_deep: Color,
    /// Secondary accent, less prominent than `accent`.
    pub secondary: Color,
    /// Translucent secondary fill.
    pub secondary_soft: Color,
    /// Pressed secondary.
    pub secondary_deep: Color,
    /// Positive/confirmation states.
    pub success: Color,
    /// Translucent success fill.
    pub success_soft: Color,
    /// Pressed success.
    pub success_deep: Color,
    /// Neutral-informational states.
    pub info: Color,
    /// Translucent info fill.
    pub info_soft: Color,
    /// Pressed info.
    pub info_deep: Color,
    /// Focus-ring stroke color.
    pub focus: Color,
}

impl Palette {
    /// Dark variant — the default theme.
    #[must_use]
    pub fn dark() -> Self {
        let teal = oklch(0.74, 0.090, 195.0);
        let teal_soft = oklcha(0.74, 0.090, 195.0, 0.18);
        let teal_deep = oklch(0.66, 0.090, 195.0);
        let coral = oklch(0.74, 0.110, 30.0);
        let coral_soft = oklcha(0.74, 0.110, 30.0, 0.18);
        let coral_deep = oklch(0.66, 0.110, 30.0);
        let amber = oklch(0.82, 0.130, 80.0);
        let amber_soft = oklcha(0.82, 0.130, 80.0, 0.18);
        let amber_deep = oklch(0.74, 0.130, 80.0);
        let violet = oklch(0.72, 0.080, 295.0);
        let violet_soft = oklcha(0.72, 0.080, 295.0, 0.18);
        let violet_deep = oklch(0.64, 0.080, 295.0);
        let green = oklch(0.72, 0.130, 145.0);
        let green_soft = oklcha(0.72, 0.130, 145.0, 0.18);
        let green_deep = oklch(0.64, 0.130, 145.0);
        let blue = oklch(0.68, 0.090, 260.0);
        let blue_soft = oklcha(0.68, 0.090, 260.0, 0.18);
        let blue_deep = oklch(0.60, 0.090, 260.0);
        Self {
            bg: oklch(0.18, 0.008, 240.0),
            bg_deep: oklch(0.14, 0.008, 240.0),
            surface: oklch(0.22, 0.008, 240.0),
            surface_2: oklch(0.26, 0.008, 240.0),
            surface_hi: oklch(0.30, 0.008, 240.0),
            border: oklch(0.30, 0.008, 240.0),
            border_strong: oklch(0.40, 0.010, 240.0),
            grid: oklch(0.26, 0.006, 240.0),
            grid_major: oklch(0.32, 0.008, 240.0),

            text: oklch(0.94, 0.005, 240.0),
            text_muted: oklch(0.70, 0.006, 240.0),
            text_faint: oklch(0.52, 0.006, 240.0),

            teal,
            teal_soft,
            teal_deep,
            coral,
            coral_soft,
            coral_deep,
            amber,
            amber_soft,
            amber_deep,
            violet,
            violet_soft,
            violet_deep,
            green,
            green_soft,
            green_deep,
            blue,
            blue_soft,
            blue_deep,

            accent: teal,
            accent_soft: teal_soft,
            accent_deep: teal_deep,
            danger: coral,
            danger_soft: coral_soft,
            danger_deep: coral_deep,
            warning: amber,
            warning_soft: amber_soft,
            warning_deep: amber_deep,
            secondary: violet,
            secondary_soft: violet_soft,
            secondary_deep: violet_deep,
            success: green,
            success_soft: green_soft,
            success_deep: green_deep,
            info: blue,
            info_soft: blue_soft,
            info_deep: blue_deep,
            focus: teal,
        }
    }

    /// Light variant.
    #[must_use]
    pub fn light() -> Self {
        let teal = oklch(0.55, 0.085, 195.0);
        let teal_soft = oklcha(0.55, 0.085, 195.0, 0.14);
        let teal_deep = oklch(0.51, 0.085, 195.0);
        let coral = oklch(0.63, 0.115, 30.0);
        let coral_soft = oklcha(0.63, 0.115, 30.0, 0.14);
        let coral_deep = oklch(0.59, 0.115, 30.0);
        let amber = oklch(0.67, 0.130, 65.0);
        let amber_soft = oklcha(0.67, 0.130, 65.0, 0.14);
        let amber_deep = oklch(0.63, 0.130, 65.0);
        let violet = oklch(0.55, 0.085, 295.0);
        let violet_soft = oklcha(0.55, 0.085, 295.0, 0.14);
        let violet_deep = oklch(0.51, 0.085, 295.0);
        let green = oklch(0.53, 0.120, 145.0);
        let green_soft = oklcha(0.53, 0.120, 145.0, 0.14);
        let green_deep = oklch(0.49, 0.120, 145.0);
        let blue = oklch(0.51, 0.090, 260.0);
        let blue_soft = oklcha(0.51, 0.090, 260.0, 0.14);
        let blue_deep = oklch(0.47, 0.090, 260.0);
        Self {
            bg: oklch(0.985, 0.004, 85.0),
            bg_deep: oklch(0.965, 0.004, 85.0),
            surface: oklch(0.995, 0.003, 85.0),
            surface_2: oklch(0.970, 0.004, 85.0),
            surface_hi: oklch(0.940, 0.005, 85.0),
            border: oklch(0.900, 0.005, 85.0),
            border_strong: oklch(0.820, 0.006, 85.0),
            grid: oklch(0.930, 0.004, 85.0),
            grid_major: oklch(0.860, 0.005, 85.0),

            text: oklch(0.22, 0.010, 240.0),
            text_muted: oklch(0.45, 0.008, 240.0),
            text_faint: oklch(0.62, 0.006, 240.0),

            teal,
            teal_soft,
            teal_deep,
            coral,
            coral_soft,
            coral_deep,
            amber,
            amber_soft,
            amber_deep,
            violet,
            violet_soft,
            violet_deep,
            green,
            green_soft,
            green_deep,
            blue,
            blue_soft,
            blue_deep,

            accent: teal,
            accent_soft: teal_soft,
            accent_deep: teal_deep,
            danger: coral,
            danger_soft: coral_soft,
            danger_deep: coral_deep,
            warning: amber,
            warning_soft: amber_soft,
            warning_deep: amber_deep,
            secondary: violet,
            secondary_soft: violet_soft,
            secondary_deep: violet_deep,
            success: green,
            success_soft: green_soft,
            success_deep: green_deep,
            info: blue,
            info_soft: blue_soft,
            info_deep: blue_deep,
            focus: teal,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::Palette;

    /// The semantic roles default to the hue values they were extracted
    /// from, in both variants. A future theme may diverge them; the default
    /// stack must not.
    #[test]
    fn semantic_roles_default_to_hue_values() {
        for p in [Palette::dark(), Palette::light()] {
            assert_eq!(p.accent, p.teal);
            assert_eq!(p.accent_soft, p.teal_soft);
            assert_eq!(p.accent_deep, p.teal_deep);
            assert_eq!(p.danger, p.coral);
            assert_eq!(p.danger_soft, p.coral_soft);
            assert_eq!(p.danger_deep, p.coral_deep);
            assert_eq!(p.warning, p.amber);
            assert_eq!(p.warning_soft, p.amber_soft);
            assert_eq!(p.warning_deep, p.amber_deep);
            assert_eq!(p.secondary, p.violet);
            assert_eq!(p.secondary_soft, p.violet_soft);
            assert_eq!(p.secondary_deep, p.violet_deep);
            assert_eq!(p.success, p.green);
            assert_eq!(p.success_soft, p.green_soft);
            assert_eq!(p.success_deep, p.green_deep);
            assert_eq!(p.info, p.blue);
            assert_eq!(p.info_soft, p.blue_soft);
            assert_eq!(p.info_deep, p.blue_deep);
            assert_eq!(p.focus, p.teal);
        }
    }
}
