//! Shared optional close-handler trait for dismissable chrome.
//!
//! Several components expose an optional "close" affordance — an alert's or
//! dialog's close (X) button, a badge's trailing dismiss (x), a notification's
//! close button and timeout. Each stores the handler as a generic `C` that
//! defaults to `()` so the callback is optional *without* boxing: `()` means
//! "no close affordance", and any `Fn(&mut State) -> Action` closure enables
//! it. [`CloseCallback`] is the shared contract behind that pattern so the
//! components don't each re-declare an identical trait.

/// Optional close handler for dismissable chrome.
///
/// Implemented by `()` (no close affordance) and by any
/// `Fn(&mut State) -> Action` closure, letting a component take the handler as
/// a generic `C = ()` that is optional without boxing. [`Self::call`] is only
/// invoked when [`Self::enabled`] returns `true`.
pub trait CloseCallback<State, Action>: Send + Sync + 'static {
    /// Whether a close affordance should be shown and wired up.
    #[must_use]
    fn enabled() -> bool {
        false
    }

    /// Invoke the handler. Only called when [`Self::enabled`] is `true`.
    fn call(&self, _state: &mut State) -> Action {
        unreachable!("CloseCallback::call on a disabled callback")
    }
}

impl<State: 'static, Action: 'static> CloseCallback<State, Action> for () {}

impl<State, Action, F> CloseCallback<State, Action> for F
where
    F: Fn(&mut State) -> Action + Send + Sync + 'static,
{
    fn enabled() -> bool {
        true
    }

    fn call(&self, state: &mut State) -> Action {
        self(state)
    }
}
