//! Shared keyboard / accessibility / lifecycle scaffolding for
//! press-activated widgets.
//!
//! The Space/Enter key-up activation block, the accesskit `Click`
//! activation predicate, and the `WidgetAdded`/`HoveredChanged` update
//! block were each written near-verbatim across the press widgets
//! (button, checkbox, toggle, radio, sidebar item, sidebar panel strip,
//! sidebar nav). Centralizing them here means a future change to
//! activation semantics lands once instead of drifting across copies.
//! Every press widget now composes these helpers — there are no
//! hand-rolled copies left. The pointer press machine lives next door in
//! [`super::click`].
//!
//! Widgets with extra concerns still keep their own `update()` — the
//! sidebar panel strip and nav track *positional* hover (which item /
//! whether the strip is under the pointer), which these shared blocks
//! have no notion of.

use masonry::accesskit::{self, Node};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{AccessEvent, PaintCtx, TextEvent, Update, UpdateCtx};

/// True when `event` is a keyboard activation for a press widget: a
/// key-*up* of Space, or of Enter when `accept_enter` is true.
///
/// Pass `accept_enter: false` for ARIA radio semantics (Space toggles
/// the focused radio; Enter is reserved for the form's default action).
/// The caller owns the side effects — `ctx.set_handled()` and submitting
/// its action — as well as any disabled/focus-target guards.
pub(crate) fn keyboard_activate(event: &TextEvent, accept_enter: bool) -> bool {
    let TextEvent::Keyboard(key) = event else {
        return false;
    };
    key.state.is_up()
        && (matches!(&key.key, Key::Character(c) if c == " ")
            || (accept_enter && key.key == Key::Named(NamedKey::Enter)))
}

/// True when `event` is the key-*down* half of a press-widget activation:
/// Space, or Enter when `accept_enter` is true — the same key predicate as
/// [`keyboard_activate`], just on press instead of release.
///
/// Pointer interaction shows a "pressed" fill for the whole press-down span
/// (`ClickPhase::Down` in [`super::click`]); [`keyboard_activate`] alone
/// gives keyboard activation no equivalent, since it only ever fires once,
/// on key-up. Callers should flip a `keyboard_pressed` field true here,
/// repaint, and flip it back false (with another repaint) on the matching
/// [`keyboard_activate`] key-up — and on focus loss, in case the key is
/// released or the widget loses focus off-screen — so Space/Enter shows the
/// same visual feedback a pointer click does.
pub(crate) fn keyboard_press_start(event: &TextEvent, accept_enter: bool) -> bool {
    let TextEvent::Keyboard(key) = event else {
        return false;
    };
    key.state.is_down()
        && (matches!(&key.key, Key::Character(c) if c == " ")
            || (accept_enter && key.key == Key::Named(NamedKey::Enter)))
}

/// True when `event` is an assistive-technology Click activation.
///
/// One comparison today, but it is the single definition of "what counts
/// as an accessibility activation" for every press widget — future
/// additions (e.g. `accesskit::Action::Default`) land here once.
pub(crate) fn is_access_click(event: &AccessEvent) -> bool {
    event.action == accesskit::Action::Click
}

/// Stamps the shared `Role::Button` accessibility for a disclosure control
/// (a collapsible header, a tree row's expand/collapse chevron): the
/// `Click` action — advertised only while `enabled`, so a disabled control
/// isn't exposed as clickable — plus `aria-expanded` reflecting the current
/// open/closed state. Callers add any control-specific attributes on top
/// (e.g. `aria-level` for a tree row's depth).
pub(crate) fn disclosure_accessibility(node: &mut Node, expanded: bool, enabled: bool) {
    if enabled {
        node.add_action(accesskit::Action::Click);
    }
    node.set_expanded(expanded);
}

/// The shared `update()` block for disabled-aware press widgets:
/// propagate the host-supplied initial `disabled` to masonry on widget
/// add (without this, the first build leaves masonry's `is_disabled()`
/// out of sync with paint state, breaking event routing and the
/// accessibility pass), and repaint on hover / disabled / focus changes.
///
/// Widgets with extra update concerns (sidebar item has no `disabled`
/// field; tabs and collapsible track positional hover state) keep their
/// own `update()`.
pub(crate) fn interaction_update(ctx: &mut UpdateCtx<'_>, event: &Update, disabled: bool) {
    match event {
        Update::WidgetAdded => {
            ctx.set_disabled(disabled);
        }
        Update::HoveredChanged(_) | Update::DisabledChanged(_) | Update::FocusChanged(_) => {
            ctx.request_paint_only();
        }
        _ => {}
    }
}

/// Snapshot of the pointer/focus interaction flags read at the top of
/// every press widget's `paint()`: `pressed` is the shared
/// "active *and* hovered" definition (dragging out of the widget
/// releases the pressed appearance).
pub(crate) struct InteractionState {
    pub(crate) hovered: bool,
    pub(crate) pressed: bool,
    pub(crate) focused: bool,
}

impl InteractionState {
    pub(crate) fn from_paint_ctx(ctx: &PaintCtx<'_>) -> Self {
        let hovered = ctx.is_hovered();
        Self {
            hovered,
            pressed: ctx.is_active() && hovered,
            focused: ctx.is_focus_target(),
        }
    }
}

#[cfg(test)]
mod tests {
    use masonry::core::TextEvent;
    use masonry::core::keyboard::{Key, NamedKey};

    use super::{is_access_click, keyboard_activate, keyboard_press_start};

    #[test]
    fn space_key_up_activates() {
        let ev = TextEvent::key_up(Key::Character(" ".into()));
        assert!(keyboard_activate(&ev, true));
        assert!(
            keyboard_activate(&ev, false),
            "Space activates regardless of the Enter policy"
        );
    }

    #[test]
    fn enter_key_up_respects_the_accept_enter_policy() {
        let ev = TextEvent::key_up(Key::Named(NamedKey::Enter));
        assert!(keyboard_activate(&ev, true));
        assert!(
            !keyboard_activate(&ev, false),
            "ARIA radio convention: Enter must not activate when accept_enter is false"
        );
    }

    #[test]
    fn key_down_does_not_activate() {
        // Activation fires on key *up*, matching every existing widget block.
        let ev = TextEvent::key_down(Key::Character(" ".into()));
        assert!(!keyboard_activate(&ev, true));
    }

    #[test]
    fn space_key_down_starts_the_press() {
        let ev = TextEvent::key_down(Key::Character(" ".into()));
        assert!(keyboard_press_start(&ev, true));
        assert!(
            keyboard_press_start(&ev, false),
            "Space starts the press regardless of the Enter policy"
        );
    }

    #[test]
    fn enter_key_down_respects_the_accept_enter_policy() {
        let ev = TextEvent::key_down(Key::Named(NamedKey::Enter));
        assert!(keyboard_press_start(&ev, true));
        assert!(!keyboard_press_start(&ev, false));
    }

    #[test]
    fn key_up_does_not_start_the_press() {
        // The press starts on key *down*, mirroring how pointer-down (not
        // pointer-up) is what shows the pressed fill.
        let ev = TextEvent::key_up(Key::Character(" ".into()));
        assert!(!keyboard_press_start(&ev, true));
    }

    #[test]
    fn other_keys_do_not_activate() {
        let ev = TextEvent::key_up(Key::Character("x".into()));
        assert!(!keyboard_activate(&ev, true));
        let ev = TextEvent::key_up(Key::Named(NamedKey::Escape));
        assert!(!keyboard_activate(&ev, true));
    }

    #[test]
    fn access_click_predicate_matches_only_click() {
        use masonry::accesskit;
        use masonry::core::AccessEvent;

        let click = AccessEvent {
            action: accesskit::Action::Click,
            data: None,
        };
        assert!(is_access_click(&click));

        let focus = AccessEvent {
            action: accesskit::Action::Focus,
            data: None,
        };
        assert!(!is_access_click(&focus));
    }

    #[test]
    fn disclosure_accessibility_reports_expanded_and_gated_click() {
        use masonry::accesskit::{Action, Node, Role};

        use super::disclosure_accessibility;

        let mut node = Node::new(Role::Button);
        disclosure_accessibility(&mut node, true, true);
        assert!(node.is_expanded() == Some(true));
        assert!(node.supports_action(Action::Click));

        let mut node = Node::new(Role::Button);
        disclosure_accessibility(&mut node, false, false);
        assert!(node.is_expanded() == Some(false));
        assert!(
            !node.supports_action(Action::Click),
            "a disabled disclosure control must not advertise Click"
        );
    }

    #[test]
    fn interaction_update_syncs_disabled_on_widget_added() {
        use masonry::core::NewWidget;
        use masonry::testing::{ModularWidget, TestHarness};
        use masonry::theme::default_property_set;

        // A widget built with `disabled: true` must propagate that to
        // masonry's system disabled flag when first attached — the exact
        // behavior of the WidgetAdded arm the four widgets duplicate.
        let widget = ModularWidget::new(()).update_fn(|(), ctx, _props, event| {
            super::interaction_update(ctx, event, true);
        });
        let mut h = TestHarness::create(default_property_set(), NewWidget::new(widget));
        h.edit_root_widget(|wm| assert!(wm.ctx.is_disabled()));
    }
}
