//! Layout density — three predefined steps mirroring Tessera's
//! `data-density` attribute (`compact` / `balanced` / `airy`).
//!
//! Density carries the small handful of sizes that scale together for the
//! whole UI: chart cell pitch, surface padding, and the UI control font
//! size. Component-specific values (a chip's vertical padding, an
//! inspector row's gap) compose from these.

/// Sizes that should scale together when a user picks a density step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Density {
    /// Chart row height (P&F box vertical pitch), in px.
    pub row: f32,
    /// Chart column width (P&F box horizontal pitch), in px.
    pub col: f32,
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
    /// grid's filter row is `row_height + pad_v`, in px.
    pub row_height: f32,
}

impl Density {
    /// `data-density="compact"`.
    #[must_use]
    pub const fn compact() -> Self {
        Self {
            row: 14.0,
            col: 12.0,
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
            row: 17.0,
            col: 15.0,
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
            row: 22.0,
            col: 19.0,
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
#[allow(clippy::float_cmp)]
mod tests {
    use super::Density;

    /// Balanced is the default step: its token values must reproduce the
    /// hardcoded per-component constants they replace, pixel for pixel.
    #[test]
    fn balanced_tokens_match_pre_token_constants() {
        let d = Density::balanced();
        assert_eq!(d.gap, 6.0); // checkbox/toggle LABEL_GAP, radio RADIO_GAP
        assert_eq!(d.gap_lg, 15.0); // former `col` inline gap
        assert_eq!(d.pad_h, 8.0); // sidebar/collapsible PAD_H, menu ICON_GAP
        assert_eq!(d.pad_v, 6.0); // sidebar/collapsible PAD_V
        assert_eq!(d.control, 14.0); // RADIO_DIAMETER, slider THUMB_DIAMETER
        assert_eq!(d.row_height, 24.0); // data_grid DEFAULT_ROW_HEIGHT
    }

    /// `gap_lg` inherits the exact former `col` values so the five inline-gap
    /// call sites migrated off `col` stay pixel-identical at every step.
    #[test]
    fn gap_lg_inherits_col_values() {
        assert_eq!(Density::compact().gap_lg, 12.0);
        assert_eq!(Density::balanced().gap_lg, 15.0);
        assert_eq!(Density::airy().gap_lg, 19.0);
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
