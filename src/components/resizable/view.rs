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
//!
//! For more than two panes, use [`h_resizable_panels`]/[`v_resizable_panels`]:
//!
//! ```ignore
//! h_resizable_panels(
//!     vec![
//!         ResizablePanel::new(sidebar).min_size(160.0).max_size(320.0),
//!         ResizablePanel::new(editor),
//!         ResizablePanel::new(inspector).min_size(200.0),
//!     ],
//!     state.pane_ratios.clone(),
//!     |s: &mut State, _handle: usize, ratios: Vec<f32>| s.pane_ratios = ratios,
//! )
//! .render(&theme)
//! ```

use std::marker::PhantomData;

use masonry::kurbo::Axis;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::widget::{ResizableWidget, ResizeHandleDragged};
use crate::Theme;

// --- MARK: PANEL SPEC

/// View state for a single boxed panel — the same erased state xilem uses for
/// `Box<AnyWidgetView<State, Action>>` (see [`AnyWidgetView`]).
type PanelViewState<State, Action> =
    <Box<AnyWidgetView<State, Action>> as View<State, Action, ViewCtx>>::ViewState;

/// A single pane of a [`ResizablePanels`] split: a type-erased view plus
/// optional pixel-size constraints.
///
/// Built with [`ResizablePanel::new`]; constraints unset by default — the pane
/// can shrink down to the structural collapse-prevention floor
/// ([`MIN_PANEL_SIZE`](super::widget::MIN_PANEL_SIZE)) and grow to fill
/// whatever space its neighbors don't claim.
pub struct ResizablePanel<State, Action = ()> {
    view: Box<AnyWidgetView<State, Action>>,
    min_size: Option<f64>,
    max_size: Option<f64>,
}

impl<State: 'static, Action: 'static> ResizablePanel<State, Action> {
    /// Wrap `view` as a resizable pane with no size constraints.
    pub fn new<V>(view: V) -> Self
    where
        V: WidgetView<State, Action>,
    {
        Self {
            view: view.boxed(),
            min_size: None,
            max_size: None,
        }
    }

    /// Constrain this pane's pixel size to be at least this many pixels.
    #[must_use]
    pub fn min_size(mut self, min_size: f64) -> Self {
        self.min_size = Some(min_size);
        self
    }

    /// Constrain this pane's pixel size to be at most this many pixels.
    #[must_use]
    pub fn max_size(mut self, max_size: f64) -> Self {
        self.max_size = Some(max_size);
        self
    }
}

// --- MARK: PANELS BUILDER

/// Builder for a multi-pane resizable split with `N - 1` draggable handles.
///
/// Created with [`h_resizable_panels`] or [`v_resizable_panels`]; returns a
/// xilem `WidgetView` via [`Self::render`].
///
/// `ratios` holds each panel's fraction of the usable extent (0.0–1.0,
/// conventionally summing to `1.0`); dragging handle `i` redistributes space
/// only between panels `i` and `i + 1`. `on_resize` is called on every drag
/// step (and keyboard nudge) with the dragged handle's index and the full
/// updated ratio vector — store it as-is and pass it back in on the next
/// render.
#[must_use = "ResizablePanels does nothing until rendered with .render(&theme)"]
pub struct ResizablePanels<State, Action, F> {
    panels: Vec<ResizablePanel<State, Action>>,
    ratios: Vec<f32>,
    on_resize: F,
    axis: Axis,
}

/// Create a horizontal (left-to-right) multi-pane split.
///
/// # Panics
///
/// [`Self::render`] panics (via [`ResizableWidget::new`](super::widget::ResizableWidget::new))
/// if `panels.len() != ratios.len()` or there are fewer than two panels.
pub fn h_resizable_panels<State, Action, F>(
    panels: Vec<ResizablePanel<State, Action>>,
    ratios: Vec<f32>,
    on_resize: F,
) -> ResizablePanels<State, Action, F> {
    ResizablePanels {
        panels,
        ratios,
        on_resize,
        axis: Axis::Horizontal,
    }
}

/// Create a vertical (top-to-bottom) multi-pane split.
///
/// # Panics
///
/// [`Self::render`] panics (via [`ResizableWidget::new`](super::widget::ResizableWidget::new))
/// if `panels.len() != ratios.len()` or there are fewer than two panels.
pub fn v_resizable_panels<State, Action, F>(
    panels: Vec<ResizablePanel<State, Action>>,
    ratios: Vec<f32>,
    on_resize: F,
) -> ResizablePanels<State, Action, F> {
    ResizablePanels {
        panels,
        ratios,
        on_resize,
        axis: Axis::Vertical,
    }
}

impl<State, Action, F> ResizablePanels<State, Action, F> {
    /// Materialize the xilem view at the supplied theme.
    pub fn render(self, theme: &Theme) -> ResizablePanelsView<State, Action, F>
    where
        State: 'static,
        Action: 'static,
        F: Fn(&mut State, usize, Vec<f32>) -> Action + Send + Sync + 'static,
    {
        ResizablePanelsView {
            panels: self.panels,
            ratios: self.ratios,
            on_resize: self.on_resize,
            axis: self.axis,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

// --- MARK: PANELS VIEW

/// The materialized [`View`] backing a [`ResizablePanels`].
///
/// Built only through [`ResizablePanels::render`]; not constructed directly.
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ResizablePanelsView<State, Action, F> {
    panels: Vec<ResizablePanel<State, Action>>,
    ratios: Vec<f32>,
    on_resize: F,
    axis: Axis,
    theme: Theme,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<State, Action, F> ViewMarker for ResizablePanelsView<State, Action, F> {}

impl<State, Action, F> View<State, Action, ViewCtx> for ResizablePanelsView<State, Action, F>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, usize, Vec<f32>) -> Action + Send + Sync + 'static,
{
    type Element = Pod<ResizableWidget>;
    type ViewState = Vec<PanelViewState<State, Action>>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut new_widgets = Vec::with_capacity(self.panels.len());
        let mut states = Vec::with_capacity(self.panels.len());
        for (i, panel) in self.panels.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let (pod, state) =
                ctx.with_id(ViewId::new(i as u64), |ctx| panel.view.build(ctx, app_state));
            new_widgets.push(pod.new_widget);
            states.push(state);
        }
        let min_sizes = self.panels.iter().map(|p| p.min_size).collect();
        let max_sizes = self.panels.iter().map(|p| p.max_size).collect();
        let widget = ResizableWidget::new(
            new_widgets,
            self.axis,
            self.ratios.clone(),
            min_sizes,
            max_sizes,
            &self.theme,
        );
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, states)
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
        if self.ratios != prev.ratios {
            ResizableWidget::set_ratios(&mut element, self.ratios.clone());
        }

        let min_sizes: Vec<Option<f64>> = self.panels.iter().map(|p| p.min_size).collect();
        if min_sizes != prev.panels.iter().map(|p| p.min_size).collect::<Vec<_>>() {
            ResizableWidget::set_min_sizes(&mut element, min_sizes);
        }
        let max_sizes: Vec<Option<f64>> = self.panels.iter().map(|p| p.max_size).collect();
        if max_sizes != prev.panels.iter().map(|p| p.max_size).collect::<Vec<_>>() {
            ResizableWidget::set_max_sizes(&mut element, max_sizes);
        }

        if self.panels.len() == prev.panels.len() {
            for (i, (panel, prev_panel)) in self.panels.iter().zip(&prev.panels).enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let mut child = ResizableWidget::panel_mut(&mut element, i);
                    panel.view.rebuild(
                        &prev_panel.view,
                        &mut view_state[i],
                        ctx,
                        child.downcast(),
                        app_state,
                    );
                });
            }
        } else {
            for (i, prev_panel) in prev.panels.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                ctx.with_id(ViewId::new(i as u64), |ctx| {
                    let mut child = ResizableWidget::panel_mut(&mut element, i);
                    prev_panel
                        .view
                        .teardown(&mut view_state[i], ctx, child.downcast());
                });
            }
            let mut new_widgets = Vec::with_capacity(self.panels.len());
            let mut new_states = Vec::with_capacity(self.panels.len());
            for (i, panel) in self.panels.iter().enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                let (pod, state) =
                    ctx.with_id(ViewId::new(i as u64), |ctx| panel.view.build(ctx, app_state));
                new_widgets.push(pod.new_widget);
                new_states.push(state);
            }
            ResizableWidget::set_panels(&mut element, new_widgets);
            *view_state = new_states;
        }
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        for (i, panel) in self.panels.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            ctx.with_id(ViewId::new(i as u64), |ctx| {
                let mut child = ResizableWidget::panel_mut(&mut element, i);
                panel
                    .view
                    .teardown(&mut view_state[i], ctx, child.downcast());
            });
        }
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
                return MessageResult::Action((self.on_resize)(app_state, msg.0, msg.1));
            }
            return MessageResult::Stale;
        }
        let id = message.take_first().expect("remaining_path was non-empty");
        let index = usize::try_from(id.routing_id()).unwrap_or(usize::MAX);
        if let Some(panel) = self.panels.get(index) {
            let mut child = ResizableWidget::panel_mut(&mut element, index);
            panel
                .view
                .message(&mut view_state[index], message, child.downcast(), app_state)
        } else {
            MessageResult::Stale
        }
    }
}

// --- MARK: TWO-PANE BUILDER

/// Adapter closure type that projects a [`ResizeHandleDragged`]'s full ratio
/// vector down to the single first-panel fraction `h_resizable`/`v_resizable`
/// callers expect — keeps [`ResizableView`] non-generic over the user's
/// `on_resize` closure type while delegating everything else to
/// [`ResizablePanelsView`].
type ResizeAdapter<State, Action> =
    Box<dyn Fn(&mut State, usize, Vec<f32>) -> Action + Send + Sync>;

/// Builder for a two-pane resizable split.
///
/// Created with [`h_resizable`] or [`v_resizable`]; returns a xilem
/// `WidgetView` via [`Self::render`]. A thin two-element wrapper over the same
/// underlying multi-pane widget as [`ResizablePanels`] — see that type if you
/// need more than two panes.
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
    pub fn render<State, Action>(self, theme: &Theme) -> ResizableView<State, Action>
    where
        State: 'static,
        Action: 'static,
        V1: WidgetView<State, Action>,
        V2: WidgetView<State, Action>,
        F: Fn(&mut State, f32) -> Action + Send + Sync + 'static,
    {
        let on_resize = self.on_resize;
        let adapter: ResizeAdapter<State, Action> = Box::new(
            move |state: &mut State, _handle: usize, ratios: Vec<f32>| on_resize(state, ratios[0]),
        );
        ResizablePanelsView {
            panels: vec![
                ResizablePanel {
                    view: self.first.boxed(),
                    min_size: self.first_min_size,
                    max_size: self.first_max_size,
                },
                ResizablePanel {
                    view: self.second.boxed(),
                    min_size: self.second_min_size,
                    max_size: self.second_max_size,
                },
            ],
            ratios: vec![self.ratio, 1.0 - self.ratio],
            on_resize: adapter,
            axis: self.axis,
            theme: *theme,
            phantom: PhantomData,
        }
    }
}

/// The materialized [`View`] backing a [`Resizable`].
///
/// Built only through [`Resizable::render`]; not constructed directly. A thin
/// two-pane specialization of [`ResizablePanelsView`].
pub type ResizableView<State, Action> = ResizablePanelsView<State, Action, ResizeAdapter<State, Action>>;
