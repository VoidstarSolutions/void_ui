//! Layout density — three predefined steps mirroring Tessera's
//! `data-density` attribute (`compact` / `balanced` / `airy`).
//!
//! Density carries the sizes that scale together when a user picks a
//! density step: surface padding (`pad`), control font size
//! (`ui_font_size`), button padding (`button_pad_h`/`button_pad_v`),
//! row/header padding (`pad_h`/`pad_v`), intra-component gaps
//! (`gap`/`gap_lg`), the base control glyph size (`control`), and the
//! data-row baseline (`row_height`). Components derive their spacing from
//! these tokens — component-specific values are documented ratios of a
//! token, not free-standing constants. Chart-domain pitches (P&F box
//! sizes) are deliberately *not* part of this type; chart products own
//! their own pitch constants.

/// Sizes that should scale together when a user picks a density step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Density {
    /// Default inner padding for surfaces (cards, panels), in px.
    pub pad: f32,
    /// Base font size for UI controls (topbar buttons, inspector chips,
    /// segmented controls), in px.
    pub ui_font_size: f32,
    /// Vertical padding inside a button (label gap above/below), in px.
    /// Spread is deliberate — Compact 3 / Balanced 5 / Airy 8 — so the
    /// density swap is visibly meaningful, not just a 1-px font change.
    pub button_pad_v: f32,
    /// Horizontal padding inside a button, in px.
    pub button_pad_h: f32,
    /// Small intra-component gap: control glyph ↔ its text label
    /// (checkbox, toggle, radio), in px.
    pub gap: f32,
    /// Inline gap between siblings in a composed content row (input affixes,
    /// label + secondary text, tab icon + label), in px.
    pub gap_lg: f32,
    /// Horizontal padding of interactive rows and headers (sidebar items,
    /// collapsible headers, menu gutters), in px.
    pub pad_h: f32,
    /// Vertical padding of interactive rows and headers, in px.
    pub pad_v: f32,
    /// Base control glyph size — radio diameter and slider thumb diameter
    /// read it directly; derived marks scale from it, in px.
    pub control: f32,
    /// Baseline data-row height: the data grid's default row height; the
    /// list's default item height is `row_height * 4 / 3` (rounded) and the
    /// grid's filter row and list's loading footer are both
    /// `row_height + pad_v`, in px.
    pub row_height: f32,
}

impl Density {
    /// `data-density="compact"`.
    #[must_use]
    pub const fn compact() -> Self {
        Self {
            pad: 10.0,
            ui_font_size: 11.0,
            button_pad_v: 3.0,
            button_pad_h: 7.0,
            gap: 4.0,
            gap_lg: 12.0,
            pad_h: 6.0,
            pad_v: 4.0,
            control: 12.0,
            row_height: 20.0,
        }
    }

    /// `data-density="balanced"` — Tessera's default.
    #[must_use]
    pub const fn balanced() -> Self {
        Self {
            pad: 12.0,
            ui_font_size: 12.0,
            button_pad_v: 5.0,
            button_pad_h: 9.0,
            gap: 6.0,
            gap_lg: 15.0,
            pad_h: 8.0,
            pad_v: 6.0,
            control: 14.0,
            row_height: 24.0,
        }
    }

    /// `data-density="airy"`.
    #[must_use]
    pub const fn airy() -> Self {
        Self {
            pad: 16.0,
            ui_font_size: 13.0,
            button_pad_v: 8.0,
            button_pad_h: 14.0,
            gap: 8.0,
            gap_lg: 19.0,
            pad_h: 11.0,
            pad_v: 8.0,
            control: 16.0,
            row_height: 30.0,
        }
    }
}

impl Default for Density {
    fn default() -> Self {
        Self::balanced()
    }
}

#[cfg(test)]
mod tests {
    use super::Density;

    /// Asserts two f32 token values are equal within `f32::EPSILON`, with a
    /// meaningful failure message naming the token.
    fn assert_token_eq(name: &str, actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < f32::EPSILON,
            "{name}: expected {expected}, got {actual}"
        );
    }

    /// Balanced is the default step: its token values must reproduce the
    /// hardcoded per-component constants they replace, pixel for pixel.
    #[test]
    fn balanced_tokens_match_pre_token_constants() {
        let d = Density::balanced();
        // checkbox/toggle LABEL_GAP, radio RADIO_GAP
        assert_token_eq("gap", d.gap, 6.0);
        // former `col` inline gap
        assert_token_eq("gap_lg", d.gap_lg, 15.0);
        // sidebar/collapsible PAD_H, menu ICON_GAP
        assert_token_eq("pad_h", d.pad_h, 8.0);
        // sidebar/collapsible PAD_V
        assert_token_eq("pad_v", d.pad_v, 6.0);
        // RADIO_DIAMETER, slider THUMB_DIAMETER
        assert_token_eq("control", d.control, 14.0);
        // data_grid DEFAULT_ROW_HEIGHT
        assert_token_eq("row_height", d.row_height, 24.0);
    }

    /// `gap_lg` inherits the exact former `col` values so the five inline-gap
    /// call sites migrated off `col` stay pixel-identical at every step.
    #[test]
    fn gap_lg_inherits_col_values() {
        assert_token_eq("compact gap_lg", Density::compact().gap_lg, 12.0);
        assert_token_eq("balanced gap_lg", Density::balanced().gap_lg, 15.0);
        assert_token_eq("airy gap_lg", Density::airy().gap_lg, 19.0);
    }

    /// The list footer spinner derives its height from `row_height + pad_v`
    /// — the same formula the grid's filter row uses — so it scales with
    /// density instead of carrying a fixed offset.
    #[test]
    fn footer_arithmetic_preserves_balanced_pixels() {
        let d = Density::balanced();
        let height = f64::from(d.row_height) + f64::from(d.pad_v);
        assert!(
            (height - 30.0).abs() < f64::EPSILON,
            "expected 30.0, got {height}"
        );
    }

    #[test]
    fn every_token_is_monotonic_across_steps() {
        let (c, b, a) = (Density::compact(), Density::balanced(), Density::airy());
        for (name, cv, bv, av) in [
            ("gap", c.gap, b.gap, a.gap),
            ("gap_lg", c.gap_lg, b.gap_lg, a.gap_lg),
            ("pad_h", c.pad_h, b.pad_h, a.pad_h),
            ("pad_v", c.pad_v, b.pad_v, a.pad_v),
            ("control", c.control, b.control, a.control),
            ("row_height", c.row_height, b.row_height, a.row_height),
            ("pad", c.pad, b.pad, a.pad),
            (
                "ui_font_size",
                c.ui_font_size,
                b.ui_font_size,
                a.ui_font_size,
            ),
            (
                "button_pad_v",
                c.button_pad_v,
                b.button_pad_v,
                a.button_pad_v,
            ),
            (
                "button_pad_h",
                c.button_pad_h,
                b.button_pad_h,
                a.button_pad_h,
            ),
        ] {
            assert!(cv < bv, "{name}: compact ({cv}) must be < balanced ({bv})");
            assert!(bv < av, "{name}: balanced ({bv}) must be < airy ({av})");
        }
    }
}
