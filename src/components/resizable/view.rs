//! Xilem view wrapper for the resizable split panel.
//!
//! ```ignore
//! h_resizable(
//!     left_content,
//!     right_content,
//!     |s: &mut State, ratio: f32| s.split_ratio = ratio,
//! )
//! .ratio(state.split_ratio)
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::core::FromDynWidget;
use masonry::kurbo::Axis;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::{ResizableWidget, ResizeHandleDragged};
use crate::Theme;

// --- MARK: BUILDER

/// Builder for a two-pane resizable split.
///
/// Created with [`h_resizable`] or [`v_resizable`]; returns a xilem
/// `WidgetView` via [`Self::render`].
#[must_use = "Resizable does nothing until rendered with .render(&theme)"]
pub struct Resizable<V1, V2, F> {
    first: V1,
    second: V2,
    on_resize: F,
    axis: Axis,
    ratio: f32,
    first_min_size: Option<f64>,
    first_max_size: Option<f64>,
    second_min_size: Option<f64>,
    second_max_size: Option<f64>,
}

/// Create a horizontal (left | right) split. `ratio` defaults to `0.5`.
pub fn h_resizable<V1, V2, F>(first: V1, second: V2, on_resize: F) -> Resizable<V1, V2, F> {
    Resizable {
        first,
        second,
        on_resize,
        axis: Axis::Horizontal,
        ratio: 0.5,
        first_min_size: None,
        first_max_size: None,
        second_min_size: None,
        second_max_size: None,
    }
}

/// Create a vertical (top / bottom) split. `ratio` defaults to `0.5`.
pub fn v_resizable<V1, V2, F>(first: V1, second: V2, on_resize: F) -> Resizable<V1, V2, F> {
    Resizable {
        first,
        second,
        on_resize,
        axis: Axis::Vertical,
        ratio: 0.5,
        first_min_size: None,
        first_max_size: None,
        second_min_size: None,
        second_max_size: None,
    }
}

impl<V1, V2, F> Resizable<V1, V2, F> {
    /// Set the initial first-panel fraction (0.0–1.0). Defaults to `0.5`.
    pub fn ratio(mut self, ratio: f32) -> Self {
        self.ratio = ratio;
        self
    }

    /// Constrain the first (left/top) pane's pixel size to be at least this
    /// many pixels. Unset by default — the pane can shrink down to the
    /// structural collapse-prevention floor ([`MIN_PANEL_SIZE`](super::widget::MIN_PANEL_SIZE)).
    pub fn first_min_size(mut self, min_size: f64) -> Self {
        self.first_min_size = Some(min_size);
        self
    }

    /// Constrain the first (left/top) pane's pixel size to be at most this
    /// many pixels. Unset by default — the pane can grow to fill the
    /// available space (minus the second pane's collapse-prevention floor).
    pub fn first_max_size(mut self, max_size: f64) -> Self {
        self.first_max_size = Some(max_size);
        self
    }

    /// Constrain the second (right/bottom) pane's pixel size to be at least
    /// this many pixels. Unset by default — the pane can shrink down to the
    /// structural collapse-prevention floor ([`MIN_PANEL_SIZE`](super::widget::MIN_PANEL_SIZE)).
    pub fn second_min_size(mut self, min_size: f64) -> Self {
        self.second_min_size = Some(min_size);
        self
    }

    /// Constrain the second (right/bottom) pane's pixel size to be at most
    /// this many pixels. Unset by default — the pane can grow to fill the
    /// available space (minus the first pane's collapse-prevention floor).
    pub fn second_max_size(mut self, max_size: f64) -> Self {
        self.second_max_size = Some(max_size);
        self
    }

    /// Materialize the xilem view at the supplied theme.
    pub fn render<State, Action>(self, theme: &Theme) -> ResizableView<V1, V2, F, State, Action>
    where
        State: 'static,
        Action: 'static,
        V1: WidgetView<State, Action>,
        V2: WidgetView<State, Action>,
        F: Fn(&mut State, f32) -> Action + Send + Sync + 'static,
    {
        ResizableView {
            first: self.first,
            second: self.second,
            on_resize: self.on_resize,
            axis: self.axis,
            ratio: self.ratio,
            first_min_size: self.first_min_size,
            first_max_size: self.first_max_size,
            second_min_size: self.second_min_size,
            second_max_size: self.second_max_size,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

// --- MARK: VIEW

/// The materialized [`View`] backing a [`Resizable`].
///
/// Built only through [`Resizable::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ResizableView<V1, V2, F, State, Action> {
    first: V1,
    second: V2,
    on_resize: F,
    axis: Axis,
    ratio: f32,
    first_min_size: Option<f64>,
    first_max_size: Option<f64>,
    second_min_size: Option<f64>,
    second_max_size: Option<f64>,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<V1, V2, F, State, Action> ViewMarker for ResizableView<V1, V2, F, State, Action> {}

impl<V1, V2, F, State, Action> View<State, Action, ViewCtx>
    for ResizableView<V1, V2, F, State, Action>
where
    V1: WidgetView<State, Action>,
    V1::Widget: FromDynWidget + Sized,
    V2: WidgetView<State, Action>,
    V2::Widget: FromDynWidget + Sized,
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, f32) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ResizableWidget<V1::Widget, V2::Widget>>;
    type ViewState = (V1::ViewState, V2::ViewState);

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (first_pod, first_state) =
            ctx.with_id(ViewId::new(0), |ctx| self.first.build(ctx, app_state));
        let (second_pod, second_state) =
            ctx.with_id(ViewId::new(1), |ctx| self.second.build(ctx, app_state));
        let widget = ResizableWidget::new(
            first_pod.new_widget,
            second_pod.new_widget,
            self.axis,
            self.ratio,
            self.first_min_size,
            self.first_max_size,
            self.second_min_size,
            self.second_max_size,
            &self.theme,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, (first_state, second_state))
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.theme != prev.theme {
            ResizableWidget::set_theme(&mut element, &self.theme);
        }
        if (self.ratio - prev.ratio).abs() > 1e-5 {
            ResizableWidget::set_ratio(&mut element, self.ratio);
        }
        if self.first_min_size != prev.first_min_size {
            ResizableWidget::set_first_min_size(&mut element, self.first_min_size);
        }
        if self.first_max_size != prev.first_max_size {
            ResizableWidget::set_first_max_size(&mut element, self.first_max_size);
        }
        if self.second_min_size != prev.second_min_size {
            ResizableWidget::set_second_min_size(&mut element, self.second_min_size);
        }
        if self.second_max_size != prev.second_max_size {
            ResizableWidget::set_second_max_size(&mut element, self.second_max_size);
        }
        ctx.with_id(ViewId::new(0), |ctx| {
            let mut first = ResizableWidget::first_mut(&mut element);
            self.first.rebuild(
                &prev.first,
                &mut view_state.0,
                ctx,
                first.downcast(),
                app_state,
            );
        });
        ctx.with_id(ViewId::new(1), |ctx| {
            let mut second = ResizableWidget::second_mut(&mut element);
            self.second.rebuild(
                &prev.second,
                &mut view_state.1,
                ctx,
                second.downcast(),
                app_state,
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(ViewId::new(0), |ctx| {
            let mut first = ResizableWidget::first_mut(&mut element);
            self.first
                .teardown(&mut view_state.0, ctx, first.downcast());
        });
        ctx.with_id(ViewId::new(1), |ctx| {
            let mut second = ResizableWidget::second_mut(&mut element);
            self.second
                .teardown(&mut view_state.1, ctx, second.downcast());
        });
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        if message.remaining_path().is_empty() {
            if let Some(msg) = message.take_message::<ResizeHandleDragged>() {
                return MessageResult::Action((self.on_resize)(app_state, msg.0));
            }
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        match id.routing_id() {
            0 => {
                let mut first = ResizableWidget::first_mut(&mut element);
                self.first
                    .message(&mut view_state.0, message, first.downcast(), app_state)
            }
            1 => {
                let mut second = ResizableWidget::second_mut(&mut element);
                self.second
                    .message(&mut view_state.1, message, second.downcast(), app_state)
            }
            _ => MessageResult::Stale,
        }
    }
}
