//! Shared primary-button click recognizer.
//!
//! The press state machine — **Down**: capture the pointer; **Up**: the
//! press completes a click only while still `is_active` *and*
//! `is_hovered`, so dragging out of the widget cancels it — was written
//! near-verbatim in three widgets
//! ([`ThemedButton`](super::button::Button),
//! [`RowClickable`](super::data_grid), `HeaderClickable`). Centralizing
//! it means a future change to the press semantics (pointer-capture
//! loss, cancel rules, non-primary buttons) lands once instead of
//! drifting across copies.
//!
//! The recognizer owns only the *shared* control flow: it captures the
//! pointer on Down and applies the active+hovered completion guard on
//! Up. Per-site side effects — focus requests, repaints, what action to
//! submit — stay at the call site, switched on the returned
//! [`ClickPhase`].

use masonry::core::{EventCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerState};

/// What a pointer event meant to the primary-click recognizer.
pub(crate) enum ClickPhase<'a> {
    /// A primary-button press began; the pointer has been captured.
    /// The widget adds its own Down side effects (focus, repaint).
    Down,
    /// A primary-button release. `Some(state)` ⇒ the press completed a
    /// click (the pointer was still captured *and* inside the widget —
    /// drag-out cancels), carrying the pointer state so the widget can
    /// read modifiers. `None` ⇒ the press ended without a click; the
    /// widget may still need to repaint its pressed state away.
    Up(Option<&'a PointerState>),
}

/// Feeds one pointer event through the shared primary-click state
/// machine (see the module docs). Returns `None` for events that play
/// no part in it (moves, other buttons, scrolls).
pub(crate) fn primary_click<'a>(
    ctx: &mut EventCtx<'_>,
    event: &'a PointerEvent,
) -> Option<ClickPhase<'a>> {
    match event {
        PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            ..
        }) => {
            ctx.capture_pointer();
            Some(ClickPhase::Down)
        }
        PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) => {
            let completed = ctx.is_active() && ctx.is_hovered();
            Some(ClickPhase::Up(completed.then_some(state)))
        }
        _ => None,
    }
}
