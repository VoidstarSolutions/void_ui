//! Xilem view for the meter component.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::meter;
//!
//! meter(0.72).render::<(), ()>(&theme);
//! meter(0.42)
//!     .fill_gradient(theme.palette.green, theme.palette.coral)
//!     .render::<(), ()>(&theme);
//!
//! // A trailing "NN%" label derived from the fraction, composed alongside
//! // the bar — the label can never drift out of sync since it's computed
//! // from the same fraction, not supplied separately.
//! meter(0.72).percent_label().render::<(), ()>(&theme)
//! # ;
//! ```
//!
//! An arbitrary (non-percentage) trailing label is just as trivially
//! composed by hand:
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use xilem::view::flex_row;
//! use void_ui::{label, meter};
//!
//! flex_row((
//!     meter(0.72).fill_gradient(theme.palette.green, theme.palette.coral).render::<(), ()>(&theme),
//!     label("B+").render::<(), ()>(&theme),
//! ))
//! # ;
//! ```

use masonry::layout::Length;
use masonry::peniko::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use super::widget::MeterWidget;
use crate::Theme;

/// Default bar thickness in px — thicker than `slider::TRACK_HEIGHT` (4.0),
/// since a meter is the primary visual rather than a slider's chrome
/// accent. Not density-scaled: a visual sizing decision, not a spacing
/// token (same reasoning as `slider::TRACK_HEIGHT`'s own doc comment).
const DEFAULT_HEIGHT: f32 = 8.0;
/// Gap between the bar and its `.percent_label()` readout, in px.
const PERCENT_LABEL_GAP: f64 = 8.0;

/// How the fill portion of a [`crate::meter`] is painted.
///
/// [`MeterFill::Gradient`] spans the *full track width* regardless of the
/// current fraction — the fill rect is a window onto a fixed gradient, so a
/// given x-coordinate is always the same color no matter how much of the
/// track is filled. See `full_track_gradient` in `widget.rs`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MeterFill {
    /// A flat fill color.
    Solid(Color),
    /// A two-stop gradient: `from` at the track's left edge (fraction 0.0),
    /// `to` at its right edge (fraction 1.0).
    Gradient(Color, Color),
}

/// Builder for a themed track + fill meter.
///
/// Created with [`meter`]. Returns a view via [`Self::render`].
#[must_use = "Meter does nothing until rendered with .render(&theme)"]
pub struct Meter {
    fraction: f64,
    fill: Option<MeterFill>,
    height: Option<f32>,
    width: Option<f32>,
    percent_label: bool,
}

/// Create a meter showing `fraction` (clamped to `0.0..=1.0`) filled.
///
/// Defaults: solid `theme.palette.accent` fill, `8.0`px height, fills the
/// available width, no percent label.
pub fn meter(fraction: f64) -> Meter {
    Meter {
        fraction,
        fill: None,
        height: None,
        width: None,
        percent_label: false,
    }
}

impl Meter {
    /// Sets a flat fill color. Overrides any previous [`Self::fill_gradient`] call.
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(MeterFill::Solid(color));
        self
    }

    /// Sets a two-stop gradient fill, spanning the full track (`from` at
    /// fraction 0.0, `to` at fraction 1.0) regardless of how much of the bar
    /// is actually filled. Overrides any previous [`Self::fill`] call.
    pub fn fill_gradient(mut self, from: Color, to: Color) -> Self {
        self.fill = Some(MeterFill::Gradient(from, to));
        self
    }

    /// Override the bar's fixed height in px. Defaults to `8.0`.
    pub fn height(mut self, px: f32) -> Self {
        self.height = Some(px);
        self
    }

    /// Set a fixed width in px. Without this, the bar fills the available width.
    pub fn width(mut self, px: f32) -> Self {
        self.width = Some(px);
        self
    }

    /// Append a trailing label showing `fraction` formatted as a
    /// whole-number percentage (e.g. `"72%"`), composed alongside the bar.
    ///
    /// The label is always derived from `fraction` — there's no separate
    /// string to pass in, so it can never drift out of sync with the bar
    /// the way a hand-composed literal could. For any other text (a score,
    /// a letter grade, anything not a percentage), compose a
    /// [`crate::label`] alongside `.render(&theme)` yourself instead — see
    /// this module's doc example.
    pub fn percent_label(mut self) -> Self {
        self.percent_label = true;
        self
    }

    /// Resolves builder defaults against `theme` into a [`MeterView`],
    /// without deciding whether to wrap it with a percent label — shared by
    /// [`Self::render`] and this module's tests, which inspect the resolved
    /// fields directly.
    fn resolve(&self, theme: &Theme) -> MeterView {
        MeterView {
            fraction: self.fraction,
            fill: self.fill.unwrap_or(MeterFill::Solid(theme.palette.accent)),
            track_color: theme.palette.surface_2,
            height: f64::from(self.height.unwrap_or(DEFAULT_HEIGHT)),
            width: self.width.map(f64::from),
        }
    }

    /// Materialize the xilem view at the supplied theme.
    ///
    /// Returns a type-erased view because [`Self::percent_label`] may need
    /// to compose the bar with an adjacent label (same reasoning as
    /// [`crate::separator`]'s optional label) — a plain [`MeterView`]
    /// alone can't express that composed shape.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<S: 'static, A: 'static>(self, theme: &Theme) -> Box<AnyWidgetView<S, A>> {
        let view = self.resolve(theme);
        if self.percent_label {
            let pct = format!("{:.0}%", self.fraction.clamp(0.0, 1.0) * 100.0);
            Box::new(
                flex_row((view, crate::label(pct).render(theme)))
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .gap(Length::px(PERCENT_LABEL_GAP)),
            )
        } else {
            Box::new(view)
        }
    }
}

/// The materialized view for a [`Meter`]'s bar.
///
/// Not constructed directly; use [`meter`] + [`Meter::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct MeterView {
    pub(super) fraction: f64,
    pub(super) fill: MeterFill,
    pub(super) track_color: Color,
    pub(super) height: f64,
    pub(super) width: Option<f64>,
}

impl ViewMarker for MeterView {}

impl<S: 'static, A: 'static> View<S, A, ViewCtx> for MeterView {
    type Element = Pod<MeterWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(MeterWidget {
                fraction: self.fraction.clamp(0.0, 1.0),
                fill: self.fill,
                track_color: self.track_color,
                height: self.height,
                width: self.width,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut S,
    ) {
        let fraction = self.fraction.clamp(0.0, 1.0);
        if (fraction - prev.fraction.clamp(0.0, 1.0)).abs() > f64::EPSILON {
            MeterWidget::set_fraction(&mut element, fraction);
        }
        if self.fill != prev.fill {
            MeterWidget::set_fill(&mut element, self.fill);
        }
        if self.track_color != prev.track_color {
            MeterWidget::set_track_color(&mut element, self.track_color);
        }
        if (self.height - prev.height).abs() > f64::EPSILON {
            MeterWidget::set_height(&mut element, self.height);
        }
        if self.width != prev.width {
            MeterWidget::set_width(&mut element, self.width);
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _: &mut S,
    ) -> MessageResult<A> {
        MessageResult::Stale
    }
}

#[cfg(test)]
mod tests {
    use masonry::peniko::Color;

    use super::{MeterFill, meter};
    use crate::Theme;

    /// Fill defaults to a solid `theme.palette.accent` when neither
    /// `.fill()` nor `.fill_gradient()` is called.
    #[test]
    fn fill_defaults_to_solid_accent() {
        let theme = Theme::default();
        assert_eq!(
            meter(0.5).resolve(&theme).fill,
            MeterFill::Solid(theme.palette.accent)
        );
    }

    /// An explicit `.fill(..)` overrides the default.
    #[test]
    fn explicit_fill_overrides_default() {
        let theme = Theme::default();
        let custom = Color::from_rgb8(10, 20, 30);
        assert_eq!(
            meter(0.5).fill(custom).resolve(&theme).fill,
            MeterFill::Solid(custom)
        );
    }

    /// `.fill_gradient(from, to)` selects the gradient variant with both stops.
    #[test]
    fn fill_gradient_sets_gradient_variant() {
        let theme = Theme::default();
        let from = Color::from_rgb8(0, 200, 0);
        let to = Color::from_rgb8(200, 0, 0);
        assert_eq!(
            meter(0.5).fill_gradient(from, to).resolve(&theme).fill,
            MeterFill::Gradient(from, to)
        );
    }

    /// Height defaults to `8.0`px, and `.height(..)` overrides it.
    #[test]
    fn height_defaults_then_yields_to_explicit() {
        let theme = Theme::default();
        assert!((meter(0.5).resolve(&theme).height - 8.0).abs() < f64::EPSILON);
        assert!((meter(0.5).height(20.0).resolve(&theme).height - 20.0).abs() < f64::EPSILON);
    }

    /// Width defaults to `None` (fill available width), and `.width(..)`
    /// overrides it.
    #[test]
    fn width_defaults_to_none_then_yields_to_explicit() {
        let theme = Theme::default();
        assert_eq!(meter(0.5).resolve(&theme).width, None);
        assert_eq!(meter(0.5).width(120.0).resolve(&theme).width, Some(120.0));
    }

    /// `.percent_label()` derives its text from `fraction`, rounded to the
    /// nearest whole percent — not a separately-supplied string that could
    /// drift out of sync.
    #[test]
    fn percent_label_formats_fraction_as_whole_percent() {
        assert_eq!(format!("{:.0}%", 0.723_f64 * 100.0), "72%");
        assert_eq!(format!("{:.0}%", 1.0_f64.clamp(0.0, 1.0) * 100.0), "100%");
        // Out-of-range fractions clamp before formatting, matching the
        // widget's own fraction clamp — never a negative or >100% label.
        assert_eq!(format!("{:.0}%", (-0.2_f64).clamp(0.0, 1.0) * 100.0), "0%");
        assert_eq!(format!("{:.0}%", 1.4_f64.clamp(0.0, 1.0) * 100.0), "100%");
    }
}
