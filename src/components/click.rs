//! Shared primary-button click recognizer.
//!
//! The press state machine — **Down**: capture the pointer; **Up**: the
//! press completes a click only while still `is_active` *and*
//! `is_hovered`, so dragging out of the widget cancels it — was written
//! near-verbatim across the press widgets (button, popover trigger,
//! row/header click wrappers, checkbox, toggle, radio, sidebar item,
//! tabs, collapsible header). Centralizing it means a future change to
//! the press semantics (pointer-capture loss, cancel rules, non-primary
//! buttons) lands once instead of drifting across copies.
//!
//! The recognizer owns only the *shared* control flow: it captures the
//! pointer on Down (gated by the caller's hit test in
//! [`primary_click_when`]) and applies the active+hovered completion
//! guard on Up. Per-site side effects — focus requests, repaints,
//! positional refinement, what action to submit — stay at the call
//! site, switched on the returned [`ClickPhase`].

use masonry::core::{EventCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerState};

/// What a pointer event meant to the primary-click recognizer.
pub(crate) enum ClickPhase<'a> {
    /// A primary-button press began; the pointer has been captured.
    /// Carries the pointer state so positional widgets can hit-test the
    /// press (`ctx.local_position(state.position)`). The widget adds its
    /// own Down side effects (focus, repaint, pressed-index tracking).
    /// None of the four users migrated in this task read the field yet
    /// (they don't need positional refinement) — the tabs/collapsible
    /// migrations in later tasks are the first readers.
    #[allow(dead_code)]
    Down(&'a PointerState),
    /// A primary-button release. `completed` ⇒ the press completed a
    /// click (the pointer was still captured *and* inside the widget —
    /// drag-out cancels). `state` is available on both outcomes so the
    /// widget can read modifiers or position even in the cancel path;
    /// either way it may need to repaint its pressed state away.
    Up {
        state: &'a PointerState,
        completed: bool,
    },
}

/// Feeds one pointer event through the shared primary-click state
/// machine (see the module docs), capturing unconditionally on Down.
/// Returns `None` for events that play no part in it (moves, other
/// buttons, scrolls).
pub(crate) fn primary_click<'a>(
    ctx: &mut EventCtx<'_>,
    event: &'a PointerEvent,
) -> Option<ClickPhase<'a>> {
    primary_click_when(ctx, event, |_, _| true)
}

/// Like [`primary_click`], but only captures the pointer (and reports
/// [`ClickPhase::Down`]) when `should_capture` approves the press.
///
/// Positional widgets — tabs items, the collapsible header — hit-test
/// the press position here. The guard matters: pointer events bubble,
/// so capturing unconditionally would let a container steal pointer
/// capture from an interactive child the press was actually aimed at
/// (e.g. a button inside a collapsible body).
pub(crate) fn primary_click_when<'a>(
    ctx: &mut EventCtx<'_>,
    event: &'a PointerEvent,
    should_capture: impl FnOnce(&mut EventCtx<'_>, &PointerState) -> bool,
) -> Option<ClickPhase<'a>> {
    match event {
        PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) => {
            if should_capture(ctx, state) {
                ctx.capture_pointer();
                Some(ClickPhase::Down(state))
            } else {
                None
            }
        }
        PointerEvent::Up(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) => {
            let completed = ctx.is_active() && ctx.is_hovered();
            Some(ClickPhase::Up { state, completed })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::NewWidget;
    use masonry::kurbo::{Axis, Point};
    use masonry::layout::Length;
    use masonry::testing::{ModularWidget, TestHarness};
    use masonry::theme::default_property_set;
    use xilem::view::PointerButton;

    use super::{ClickPhase, primary_click_when};

    /// Counters recording which phases the recognizer reported.
    #[derive(Default)]
    struct Probe {
        downs: usize,
        completed: usize,
        cancelled: usize,
    }

    /// A widget whose capture guard only accepts presses on its left
    /// half (x < 50) — a stand-in for tabs' item hit test and
    /// collapsible's header hit test.
    fn probe_widget() -> ModularWidget<Probe> {
        ModularWidget::new(Probe::default())
            .accepts_pointer_interaction(true)
            .measure_fn(|_, _, _, axis, _, _| match axis {
                Axis::Horizontal => Length::px(100.0),
                Axis::Vertical => Length::px(40.0),
            })
            .pointer_event_fn(|state, ctx, _props, event| {
                match primary_click_when(ctx, event, |ctx, ps| {
                    ctx.local_position(ps.position).x < 50.0
                }) {
                    Some(ClickPhase::Down(_)) => state.downs += 1,
                    Some(ClickPhase::Up {
                        completed: true, ..
                    }) => state.completed += 1,
                    Some(ClickPhase::Up {
                        completed: false, ..
                    }) => state.cancelled += 1,
                    None => {}
                }
            })
    }

    fn harness() -> TestHarness<ModularWidget<Probe>> {
        TestHarness::create_with_size(
            default_property_set(),
            NewWidget::new(probe_widget()),
            (100, 40),
        )
    }

    #[test]
    fn press_and_release_inside_completes_a_click() {
        let mut h = harness();
        h.mouse_move(Point::new(25.0, 20.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.state.downs, 1);
            assert_eq!(wm.widget.state.completed, 1);
            assert_eq!(wm.widget.state.cancelled, 0);
        });
    }

    #[test]
    fn dragging_out_of_the_widget_cancels() {
        let mut h = harness();
        h.mouse_move(Point::new(25.0, 20.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(300.0, 300.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.state.downs, 1);
            assert_eq!(wm.widget.state.completed, 0);
            assert_eq!(wm.widget.state.cancelled, 1);
        });
    }

    #[test]
    fn rejected_capture_guard_reports_no_down_and_never_completes() {
        let mut h = harness();
        // Right half: the guard rejects, so no capture, no Down —
        // and the Up cannot complete because the widget never went active.
        h.mouse_move(Point::new(75.0, 20.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_button_release(Some(PointerButton::Primary));
        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.state.downs, 0);
            assert_eq!(wm.widget.state.completed, 0);
        });
    }

    #[test]
    fn completion_is_widget_level_positional_refinement_is_the_callers_job() {
        // Press left half, release right half: still inside the widget
        // and still captured, so the recognizer reports completed — a
        // positional caller (tabs, collapsible) re-checks
        // `state.position` in its Up arm to refine this.
        let mut h = harness();
        h.mouse_move(Point::new(25.0, 20.0));
        h.mouse_button_press(Some(PointerButton::Primary));
        h.mouse_move(Point::new(75.0, 20.0));
        h.mouse_button_release(Some(PointerButton::Primary));
        h.edit_root_widget(|wm| {
            assert_eq!(wm.widget.state.completed, 1);
        });
    }
}
