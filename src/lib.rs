//! Void UI components.
//!
//! Xilem-flavored widget library wrapping [`citadel_chart`] and the rest of
//! the domain widget surface (scanner grid, watch list, instrument context,
//! settings forms). This crate absorbs Xilem view-layer churn so application
//! code stays insulated from upstream renames.

#![forbid(unsafe_code)]

use std::marker::PhantomData;

use citadel_chart::ChartWidget;
use citadel_core::pf::{ChartSnapshot, ColumnDelta};
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx};

pub use citadel_chart::ChartAction;

/// Snapshot of the chart's input state. Cloned into the [`Chart`] view on
/// every `app_logic` rebuild; cheap for the verification-vector sizes we
/// run today (single-digit deltas), worth revisiting if very long histories
/// flow through.
#[derive(Default, Clone, Debug)]
pub struct ChartData {
    pub snapshot: Option<ChartSnapshot>,
    pub deltas: Vec<ColumnDelta>,
}

/// Construct a chart view that forwards key-press actions from the chart
/// widget to `on_key`. The callback receives the action emitted by the
/// underlying [`ChartWidget`] and may mutate `State` and/or return an
/// `Action` for the parent view.
pub fn chart<F, State, Action>(data: ChartData, on_key: F) -> Chart<F, State, Action>
where
    F: Fn(&mut State, ChartAction) -> Action + 'static,
{
    Chart {
        data,
        on_key,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Chart<F, State, Action> {
    data: ChartData,
    on_key: F,
    phantom: PhantomData<fn(State) -> Action>,
}

impl<F, State, Action> ViewMarker for Chart<F, State, Action> {}

impl<F, State, Action> View<State, Action, ViewCtx> for Chart<F, State, Action>
where
    State: 'static,
    Action: 'static,
    F: Fn(&mut State, ChartAction) -> Action + 'static,
{
    type Element = Pod<ChartWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = ChartWidget::with_state(self.data.snapshot.clone(), self.data.deltas.clone());
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        (element, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
        if self.data.snapshot != prev.data.snapshot {
            // Snapshot replacement (including transitions to/from None):
            // re-baseline the widget from scratch.
            ChartWidget::apply_state(
                &mut element,
                self.data.snapshot.clone(),
                self.data.deltas.clone(),
            );
        } else if self.data.deltas.len() > prev.data.deltas.len()
            && self.data.deltas[..prev.data.deltas.len()] == prev.data.deltas[..]
        {
            // Append-only growth: push only the new tail.
            for delta in &self.data.deltas[prev.data.deltas.len()..] {
                ChartWidget::push_delta(&mut element, delta.clone());
            }
        } else if self.data.deltas != prev.data.deltas {
            // History diverged in some other way — re-apply current state.
            ChartWidget::apply_state(
                &mut element,
                self.data.snapshot.clone(),
                self.data.deltas.clone(),
            );
        }
    }

    fn teardown(
        &self,
        (): &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        ctx.teardown_action_source(element);
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<ChartAction>() {
            Some(action) => MessageResult::Action((self.on_key)(app_state, *action)),
            None => MessageResult::Stale,
        }
    }
}
