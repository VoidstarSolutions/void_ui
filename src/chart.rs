//! Xilem `Chart` view + supporting types.
//!
//! Wraps the masonry-side [`citadel_chart::ChartWidget`] in a xilem
//! [`View`] that reactively re-baselines or appends as the host's
//! [`ChartData`] grows. Originally defined inline in
//! [`crate::lib`]; lifted here so the lib root stays a thin
//! re-export hub.

use std::marker::PhantomData;

use citadel_chart::ChartWidget;
use citadel_core::Tick;
use citadel_core::pf::{ChartSnapshot, ColumnDelta};
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx};

pub use citadel_chart::ChartAction;

/// Snapshot of the chart's input state. Cloned into the [`Chart`] view on
/// every `app_logic` rebuild; cheap for the verification-vector sizes we
/// run today (single-digit deltas), worth revisiting if very long histories
/// flow through.
///
/// `ticks` carries the input price stream alongside the derived
/// `deltas`. It isn't used for painting; it's there so the chart
/// widget can copy the raw price history to the system clipboard
/// (Cmd-C / Ctrl-C) — the seed for new validation vectors carved out
/// of real data.
#[derive(Default, Clone, Debug)]
pub struct ChartData {
    pub snapshot: Option<ChartSnapshot>,
    pub deltas: Vec<ColumnDelta>,
    pub ticks: Vec<Tick>,
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
        let widget = ChartWidget::with_state(
            self.data.snapshot.clone(),
            self.data.deltas.clone(),
            self.data.ticks.clone(),
        );
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
        // Re-baseline whenever the snapshot identity changes (load /
        // reset / no-snapshot transitions). The deltas / ticks
        // streams reset alongside it.
        if self.data.snapshot != prev.data.snapshot {
            ChartWidget::apply_state(
                &mut element,
                self.data.snapshot.clone(),
                self.data.deltas.clone(),
                self.data.ticks.clone(),
            );
            return;
        }

        let deltas_grew = self.data.deltas.len() > prev.data.deltas.len()
            && self.data.deltas[..prev.data.deltas.len()] == prev.data.deltas[..];
        let ticks_grew = self.data.ticks.len() > prev.data.ticks.len()
            && self.data.ticks[..prev.data.ticks.len()] == prev.data.ticks[..];

        if deltas_grew && ticks_grew {
            // Append-only growth on both streams — push tails only.
            for delta in &self.data.deltas[prev.data.deltas.len()..] {
                ChartWidget::push_delta(&mut element, delta.clone());
            }
            for tick in &self.data.ticks[prev.data.ticks.len()..] {
                ChartWidget::push_tick(&mut element, tick.clone());
            }
        } else if self.data.deltas != prev.data.deltas || self.data.ticks != prev.data.ticks {
            // History diverged in some other way — re-apply current state.
            ChartWidget::apply_state(
                &mut element,
                self.data.snapshot.clone(),
                self.data.deltas.clone(),
                self.data.ticks.clone(),
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
