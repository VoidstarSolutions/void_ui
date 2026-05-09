//! Void UI components.
//!
//! Xilem-flavored widget library wrapping [`void_chart`] and the rest of the
//! domain widget surface (scanner grid, watch list, instrument context,
//! settings forms). This crate absorbs Xilem view-layer churn so application
//! code stays insulated from upstream renames.

#![forbid(unsafe_code)]

use std::marker::PhantomData;

use void_chart::ChartWidget;
use xilem_masonry::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem_masonry::{Pod, ViewCtx};

pub fn chart<State: 'static>() -> Chart<State> {
    Chart {
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct Chart<State> {
    phantom: PhantomData<fn() -> State>,
}

impl<State> ViewMarker for Chart<State> {}

impl<State, Action> View<State, Action, ViewCtx> for Chart<State>
where
    State: 'static,
    Action: 'static,
{
    type Element = Pod<ChartWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _state: &mut State) -> (Self::Element, Self::ViewState) {
        let element = ctx.with_action_widget(|ctx| ctx.create_pod(ChartWidget::new()));
        (element, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) {
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
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}
