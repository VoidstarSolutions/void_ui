//! Xilem view for the skeleton loading placeholder.
//!
//! ```ignore
//! use void_ui::skeleton;
//!
//! // A full-width text line.
//! skeleton().render(&theme)
//!
//! // A fixed block, secondary tone, no animation.
//! skeleton().rectangle().width(200.0).height(80.0).secondary().animated(false).render(&theme)
//!
//! // A 40px avatar circle with a shimmer sweep.
//! skeleton().circle(40.0).wave().render(&theme)
//! ```

use masonry::peniko::Color;
use masonry::peniko::color::HueDirection;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::SkeletonWidget;
use crate::Theme;

/// The shape a skeleton placeholder stands in for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkeletonShape {
    /// A single line, sized to stand in for text. The default.
    #[default]
    Text,
    /// A rectangular block, e.g. an image or content area.
    Rectangle,
    /// A circle, e.g. an avatar.
    Circle,
}

/// Which animation, if any, plays over a skeleton placeholder's fill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkeletonAnimation {
    /// A gentle opacity breathing. The default.
    #[default]
    Pulse,
    /// A brighter band that sweeps left-to-right across the shape.
    Wave,
    /// No animation; a static fill.
    None,
}

/// Builder for a themed loading placeholder.
///
/// Created with [`skeleton`]. Renders a single rounded shape — a text line by
/// default, or a rectangle / circle via [`Self::rectangle`] / [`Self::circle`]
/// — that stands in for content while it loads, animating unless disabled.
/// Returns a view via [`Self::render`].
#[must_use = "Skeleton does nothing until rendered with .render(&theme)"]
pub struct Skeleton {
    shape: SkeletonShape,
    width: Option<f64>,
    height: Option<f64>,
    radius: Option<f64>,
    color: Option<Color>,
    secondary: bool,
    animation: SkeletonAnimation,
}

/// Create a full-width, single-line skeleton placeholder.
pub fn skeleton() -> Skeleton {
    Skeleton {
        shape: SkeletonShape::Text,
        width: None,
        height: None,
        radius: None,
        color: None,
        secondary: false,
        animation: SkeletonAnimation::Pulse,
    }
}

impl Skeleton {
    /// Render as a text line (the default). Overrides a previous
    /// [`Self::rectangle`] or [`Self::circle`] call.
    pub fn text(mut self) -> Self {
        self.shape = SkeletonShape::Text;
        self
    }

    /// Render as a rectangular block, e.g. an image or content area.
    pub fn rectangle(mut self) -> Self {
        self.shape = SkeletonShape::Rectangle;
        self
    }

    /// Render as a circle of the given diameter in px — the idiom for avatar
    /// placeholders.
    pub fn circle(mut self, diameter: f64) -> Self {
        self.shape = SkeletonShape::Circle;
        self.width = Some(diameter);
        self.height = Some(diameter);
        self
    }

    /// Set the shape directly.
    pub fn shape(mut self, shape: SkeletonShape) -> Self {
        self.shape = shape;
        self
    }

    /// Set a fixed width in px. Without this, the placeholder fills the
    /// available width.
    pub fn width(mut self, px: f64) -> Self {
        self.width = Some(px);
        self
    }

    /// Set a fixed height in px. Defaults to one line of body text.
    pub fn height(mut self, px: f64) -> Self {
        self.height = Some(px);
        self
    }

    /// Set both width and height to the same square size in px.
    pub fn size(mut self, px: f64) -> Self {
        self.width = Some(px);
        self.height = Some(px);
        self
    }

    /// Override the corner radius in px. Defaults to `theme.radius.small`.
    /// Ignored when the shape is [`SkeletonShape::Circle`].
    pub fn rounded(mut self, px: f64) -> Self {
        self.radius = Some(px);
        self
    }

    /// Use the secondary (softer) placeholder tone. Ignored when an explicit
    /// [`Self::color`] is set.
    pub fn secondary(mut self) -> Self {
        self.secondary = true;
        self
    }

    /// Override the base fill color. Takes precedence over [`Self::secondary`].
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    /// Set the animation directly.
    pub fn animation(mut self, animation: SkeletonAnimation) -> Self {
        self.animation = animation;
        self
    }

    /// Use the pulse animation (the default) — a gentle opacity breathing.
    pub fn pulse(mut self) -> Self {
        self.animation = SkeletonAnimation::Pulse;
        self
    }

    /// Use the wave animation — a brighter band sweeping left-to-right.
    pub fn wave(mut self) -> Self {
        self.animation = SkeletonAnimation::Wave;
        self
    }

    /// Enable or disable animation. Enabled (pulse) by default; `false` maps
    /// to [`SkeletonAnimation::None`] and `true` maps to
    /// [`SkeletonAnimation::Pulse`].
    pub fn animated(mut self, animated: bool) -> Self {
        self.animation = if animated {
            SkeletonAnimation::Pulse
        } else {
            SkeletonAnimation::None
        };
        self
    }

    /// Materialize a view at the supplied theme.
    pub fn render(self, theme: &Theme) -> SkeletonView {
        let color = self.color.unwrap_or(if self.secondary {
            theme.palette.surface_2
        } else {
            theme.palette.surface_hi
        });
        // The wave's bright band: the base color lightened toward a faint
        // text token, so it reads on both light and dark surfaces.
        let highlight = color.lerp(theme.palette.text_faint, 0.22, HueDirection::default());
        SkeletonView {
            color,
            highlight,
            radius: self.radius.unwrap_or(f64::from(theme.radius.small)),
            circle: self.shape == SkeletonShape::Circle,
            width: self.width,
            height: self
                .height
                .unwrap_or(f64::from(theme.typography.size_body) * 1.2),
            animation: self.animation,
        }
    }
}

/// The materialized view for a skeleton placeholder.
///
/// Not constructed directly; use [`skeleton`] + [`Skeleton::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SkeletonView {
    pub(super) color: Color,
    pub(super) highlight: Color,
    pub(super) radius: f64,
    pub(super) circle: bool,
    pub(super) width: Option<f64>,
    pub(super) height: f64,
    pub(super) animation: SkeletonAnimation,
}

impl ViewMarker for SkeletonView {}

impl<S: 'static, A: 'static> View<S, A, ViewCtx> for SkeletonView {
    type Element = Pod<SkeletonWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(SkeletonWidget {
                color: self.color,
                highlight: self.highlight,
                radius: self.radius,
                circle: self.circle,
                width: self.width,
                height: self.height,
                animation: self.animation,
                t: 0.0,
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
        if self.color != prev.color {
            SkeletonWidget::set_color(&mut element, self.color);
        }
        if self.highlight != prev.highlight {
            SkeletonWidget::set_highlight(&mut element, self.highlight);
        }
        if (self.radius - prev.radius).abs() > f64::EPSILON {
            SkeletonWidget::set_radius(&mut element, self.radius);
        }
        if self.circle != prev.circle {
            SkeletonWidget::set_circle(&mut element, self.circle);
        }
        if self.width != prev.width {
            SkeletonWidget::set_width(&mut element, self.width);
        }
        if (self.height - prev.height).abs() > f64::EPSILON {
            SkeletonWidget::set_height(&mut element, self.height);
        }
        if self.animation != prev.animation {
            SkeletonWidget::set_animation(&mut element, self.animation);
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
