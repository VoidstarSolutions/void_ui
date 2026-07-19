//! Xilem view for the meter component.
//!
//! ```ignore
//! use void_ui::meter;
//!
//! meter(0.72).render(&theme)
//! meter(0.42)
//!     .fill_gradient(theme.palette.green, theme.palette.coral)
//!     .label("42%")
//!     .render(&theme)
//! ```

use masonry::core::{ArcStr, NewWidget, StyleProperty, Widget as _};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::Label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::MeterWidget;
use crate::Theme;

/// Default bar thickness in px — thicker than `slider::TRACK_HEIGHT` (4.0),
/// since a meter is the primary visual rather than a slider's chrome
/// accent. Not density-scaled: a visual sizing decision, not a spacing
/// token (same reasoning as `slider::TRACK_HEIGHT`'s own doc comment).
const DEFAULT_HEIGHT: f32 = 8.0;

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
    label: Option<ArcStr>,
    height: Option<f32>,
    width: Option<f32>,
}

/// Create a meter showing `fraction` (clamped to `0.0..=1.0`) filled.
///
/// Defaults: solid `theme.palette.accent` fill, no label, `8.0`px height,
/// fills the available width.
pub fn meter(fraction: f64) -> Meter {
    Meter {
        fraction,
        fill: None,
        label: None,
        height: None,
        width: None,
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

    /// Add a centered overlay label. The caller controls the exact text —
    /// `void_ui` doesn't assume percentage formatting, since the fraction
    /// could mean anything (score, pass-rate, rung distribution).
    pub fn label(mut self, text: impl Into<ArcStr>) -> Self {
        self.label = Some(text.into());
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

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render(self, theme: &Theme) -> MeterView {
        MeterView {
            fraction: self.fraction,
            fill: self.fill.unwrap_or(MeterFill::Solid(theme.palette.accent)),
            track_color: theme.palette.surface_2,
            label_text: self.label,
            label_color: theme.palette.text,
            label_font_size: theme.density.ui_font_size,
            height: f64::from(self.height.unwrap_or(DEFAULT_HEIGHT)),
            width: self.width.map(f64::from),
        }
    }
}

/// Builds a `Label` child styled for a meter's centered overlay text.
fn build_label(text: &ArcStr, color: Color, font_size: f32) -> NewWidget<Label> {
    let mut label = Label::new(text.clone())
        .with_style(StyleProperty::FontSize(font_size))
        .prepare();
    label.properties.insert(ContentColor::new(color));
    label
}

/// The materialized view for a [`Meter`].
///
/// Not constructed directly; use [`meter`] + [`Meter::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct MeterView {
    pub(super) fraction: f64,
    pub(super) fill: MeterFill,
    pub(super) track_color: Color,
    pub(super) label_text: Option<ArcStr>,
    pub(super) label_color: Color,
    pub(super) label_font_size: f32,
    pub(super) height: f64,
    pub(super) width: Option<f64>,
}

impl ViewMarker for MeterView {}

impl<S: 'static, A: 'static> View<S, A, ViewCtx> for MeterView {
    type Element = Pod<MeterWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let label = self
            .label_text
            .as_ref()
            .map(|text| build_label(text, self.label_color, self.label_font_size).to_pod());
        (
            ctx.create_pod(MeterWidget {
                fraction: self.fraction.clamp(0.0, 1.0),
                fill: self.fill,
                track_color: self.track_color,
                height: self.height,
                width: self.width,
                label,
                label_text: self.label_text.clone(),
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
        if self.label_text != prev.label_text
            || self.label_color != prev.label_color
            || (self.label_font_size - prev.label_font_size).abs() > f32::EPSILON
        {
            match &self.label_text {
                Some(text) => MeterWidget::attach_label(
                    &mut element,
                    build_label(text, self.label_color, self.label_font_size),
                    text.clone(),
                ),
                None => MeterWidget::detach_label(&mut element),
            }
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
            meter(0.5).render(&theme).fill,
            MeterFill::Solid(theme.palette.accent)
        );
    }

    /// An explicit `.fill(..)` overrides the default.
    #[test]
    fn explicit_fill_overrides_default() {
        let theme = Theme::default();
        let custom = Color::from_rgb8(10, 20, 30);
        assert_eq!(
            meter(0.5).fill(custom).render(&theme).fill,
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
            meter(0.5).fill_gradient(from, to).render(&theme).fill,
            MeterFill::Gradient(from, to)
        );
    }

    /// Height defaults to `8.0`px, and `.height(..)` overrides it.
    #[test]
    fn height_defaults_then_yields_to_explicit() {
        let theme = Theme::default();
        assert!((meter(0.5).render(&theme).height - 8.0).abs() < f64::EPSILON);
        assert!((meter(0.5).height(20.0).render(&theme).height - 20.0).abs() < f64::EPSILON);
    }

    /// Width defaults to `None` (fill available width), and `.width(..)`
    /// overrides it.
    #[test]
    fn width_defaults_to_none_then_yields_to_explicit() {
        let theme = Theme::default();
        assert_eq!(meter(0.5).render(&theme).width, None);
        assert_eq!(meter(0.5).width(120.0).render(&theme).width, Some(120.0));
    }

    /// No label by default; `.label(..)` sets the caller-supplied text verbatim.
    #[test]
    fn label_defaults_to_none_then_yields_to_explicit() {
        let theme = Theme::default();
        assert_eq!(meter(0.5).render(&theme).label_text, None);
        assert_eq!(
            meter(0.5).label("72%").render(&theme).label_text.as_deref(),
            Some("72%")
        );
    }
}
