//! Xilem view for the spinner component.
//!
//! ```
//! # use void_ui::Theme;
//! # let theme = Theme::default();
//! use void_ui::spinner;
//!
//! spinner().render::<(), ()>(&theme);
//! spinner().color(theme.palette.accent).size(24.0).render::<(), ()>(&theme)
//! # ;
//! ```

use std::marker::PhantomData;

use masonry::peniko::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use super::widget::SpinnerWidget;
use crate::Theme;

/// Builder for a standalone animated loading spinner.
///
/// Created with [`spinner`]. Returns a xilem view via [`Self::render`].
#[must_use = "Spinner does nothing until rendered with .render(&theme)"]
pub struct Spinner {
    color: Option<Color>,
    size: Option<f64>,
}

/// Create a themed loading spinner.
///
/// Color defaults to `palette.text_muted`; size defaults to
/// `density.ui_font_size`. Override either with builder methods before
/// calling `.render(&theme)`.
pub fn spinner() -> Spinner {
    Spinner {
        color: None,
        size: None,
    }
}

impl Spinner {
    /// Override the spinner color. Defaults to `palette.text_muted`.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Override the spinner size in pixels. Defaults to `density.ui_font_size`.
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(f64::from(size));
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State: 'static, Action: 'static>(
        self,
        theme: &Theme,
    ) -> SpinnerView<State, Action> {
        SpinnerView {
            color: self.color.unwrap_or(theme.palette.text_muted),
            size: self.size.unwrap_or(f64::from(theme.density.ui_font_size)),
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Spinner`].
///
/// Built only through [`Spinner::render`]; not constructed directly by callers.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SpinnerView<State, Action> {
    pub(super) color: Color,
    pub(super) size: f64,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<S, A> ViewMarker for SpinnerView<S, A> {}

impl<S: 'static, A: 'static> View<S, A, ViewCtx> for SpinnerView<S, A> {
    type Element = Pod<SpinnerWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, ()) {
        (
            ctx.create_pod(SpinnerWidget::new(self.color, self.size)),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut (),
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut S,
    ) {
        if self.color != prev.color {
            SpinnerWidget::set_color(&mut element, self.color);
        }
        if (self.size - prev.size).abs() > f64::EPSILON {
            SpinnerWidget::set_size(&mut element, self.size);
        }
    }

    fn teardown(&self, (): &mut (), _ctx: &mut ViewCtx, _element: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut (),
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

    use super::spinner;
    use crate::{Theme, test_support};

    #[derive(Default)]
    struct AppState;

    #[test]
    fn defaults_to_theme_muted_color_and_ui_font_size() {
        let theme = Theme::default();
        let v = spinner().render::<AppState, ()>(&theme);
        assert_eq!(v.color, theme.palette.text_muted);
        assert!((v.size - f64::from(theme.density.ui_font_size)).abs() < f64::EPSILON);
    }

    #[test]
    fn overrides_apply_to_the_rendered_view() {
        let theme = Theme::default();
        let v = spinner()
            .color(theme.palette.accent)
            .size(24.0)
            .render::<AppState, ()>(&theme);
        assert_eq!(v.color, theme.palette.accent);
        assert!((v.size - 24.0).abs() < f64::EPSILON);
    }

    #[test]
    fn spinner_builds_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = AppState;

        let _ = spinner()
            .color(theme.palette.accent)
            .size(24.0)
            .render::<AppState, ()>(&theme)
            .build(&mut ctx, &mut state);
    }
}
