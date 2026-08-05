//! Xilem view for the separator component.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::separator;
//!
//! separator().render::<(), ()>(&theme);
//! separator().vertical().dashed().color(theme.palette.accent).render::<(), ()>(&theme);
//! separator().label("Section").render::<(), ()>(&theme)
//! # ;
//! ```

use masonry::peniko::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row};
use xilem::{AnyWidgetView, Pod, ViewCtx};

use super::widget::SeparatorWidget;
use crate::{Orientation, Theme};

/// Visual style of the separator line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeparatorVariant {
    Solid,
    Dashed,
}

/// Builder for a themed divider line.
///
/// Created with [`separator`]. Returns a view via [`Self::render`].
#[must_use = "Separator does nothing until rendered with .render(&theme)"]
pub struct Separator {
    orientation: Orientation,
    style: SeparatorVariant,
    color: Option<Color>,
    label: Option<String>,
}

/// Create a horizontal solid separator.
pub fn separator() -> Separator {
    Separator {
        orientation: Orientation::Horizontal,
        style: SeparatorVariant::Solid,
        color: None,
        label: None,
    }
}

impl Separator {
    /// Render as a vertical line instead of horizontal.
    pub fn vertical(mut self) -> Self {
        self.orientation = Orientation::Vertical;
        self
    }

    /// Render as a dashed line instead of solid.
    pub fn dashed(mut self) -> Self {
        self.style = SeparatorVariant::Dashed;
        self
    }

    /// Override the line color. Defaults to `palette.border`.
    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    /// Add a text label centered on the separator.
    ///
    /// For horizontal separators, the label appears between two line segments in a row.
    /// For vertical separators, it appears between two line segments in a column.
    pub fn label(mut self, text: impl Into<String>) -> Self {
        self.label = Some(text.into());
        self
    }

    /// Materialize a view at the supplied theme.
    #[must_use]
    pub fn render<S: 'static, A: 'static>(self, theme: &Theme) -> Box<AnyWidgetView<S, A>> {
        let color = self.color.unwrap_or(theme.palette.border);
        let line = || SeparatorLineView {
            orientation: self.orientation,
            style: self.style,
            color,
        };

        if let Some(text) = self.label {
            let lbl = crate::label(text)
                .color(theme.palette.text_faint)
                .text_size(theme.typography.size_caption)
                .render(theme);
            match self.orientation {
                Orientation::Horizontal => Box::new(
                    flex_row((line().flex(1.0), lbl, line().flex(1.0)))
                        .cross_axis_alignment(CrossAxisAlignment::Center),
                ),
                Orientation::Vertical => Box::new(
                    flex_col((line().flex(1.0), lbl, line().flex(1.0)))
                        .cross_axis_alignment(CrossAxisAlignment::Center),
                ),
            }
        } else {
            Box::new(line())
        }
    }
}

/// The materialized view for a single separator line segment.
///
/// Not constructed directly; use [`separator`] + [`Separator::render`].
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SeparatorLineView {
    pub(super) orientation: Orientation,
    pub(super) style: SeparatorVariant,
    pub(super) color: Color,
}

impl ViewMarker for SeparatorLineView {}

impl<S: 'static, A: 'static> View<S, A, ViewCtx> for SeparatorLineView {
    type Element = Pod<SeparatorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(SeparatorWidget {
                orientation: self.orientation,
                style: self.style,
                color: self.color,
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
            SeparatorWidget::set_color(&mut element, self.color);
        }
        if self.style != prev.style {
            SeparatorWidget::set_style(&mut element, self.style);
        }
        if self.orientation != prev.orientation {
            SeparatorWidget::set_orientation(&mut element, self.orientation);
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
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{SeparatorVariant, separator};
    use crate::{Orientation, Theme, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn defaults_to_horizontal_solid_no_color_or_label_override() {
        let s = separator();
        assert_eq!(s.orientation, Orientation::Horizontal);
        assert_eq!(s.style, SeparatorVariant::Solid);
        assert!(s.color.is_none());
        assert!(s.label.is_none());
    }

    #[test]
    fn builder_methods_set_the_expected_fields() {
        let theme = Theme::default();
        let s = separator()
            .vertical()
            .dashed()
            .color(theme.palette.accent)
            .label("Section");
        assert_eq!(s.orientation, Orientation::Vertical);
        assert_eq!(s.style, SeparatorVariant::Dashed);
        assert_eq!(s.color, Some(theme.palette.accent));
        assert_eq!(s.label.as_deref(), Some("Section"));
    }

    #[test]
    fn plain_labeled_vertical_and_dashed_separators_build_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = separator()
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = separator()
            .vertical()
            .dashed()
            .color(theme.palette.accent)
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = separator()
            .label("Section")
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = separator()
            .vertical()
            .label("Section")
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
    }
}
