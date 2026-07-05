//! Shared keyboard / accessibility / lifecycle scaffolding for
//! press-activated widgets.
//!
//! The Space/Enter key-up activation block, the accesskit `Click`
//! activation predicate, and the `WidgetAdded`/`HoveredChanged` update
//! block were each written near-verbatim in five widgets (button,
//! checkbox, toggle, radio, sidebar item). Centralizing them here means
//! a future change to activation semantics lands once instead of
//! drifting across copies. The pointer press machine lives next door in
//! [`super::click`].

use masonry::accesskit;
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{AccessEvent, TextEvent};

/// True when `event` is a keyboard activation for a press widget: a
/// key-*up* of Space, or of Enter when `accept_enter` is true.
///
/// Pass `accept_enter: false` for ARIA radio semantics (Space toggles
/// the focused radio; Enter is reserved for the form's default action).
/// The caller owns the side effects — `ctx.set_handled()` and submitting
/// its action — as well as any disabled/focus-target guards.
#[allow(dead_code)]
pub(crate) fn keyboard_activate(event: &TextEvent, accept_enter: bool) -> bool {
    let TextEvent::Keyboard(key) = event else {
        return false;
    };
    key.state.is_up()
        && (matches!(&key.key, Key::Character(c) if c == " ")
            || (accept_enter && key.key == Key::Named(NamedKey::Enter)))
}

/// True when `event` is an assistive-technology Click activation.
///
/// One comparison today, but it is the single definition of "what counts
/// as an accessibility activation" for every press widget — future
/// additions (e.g. `accesskit::Action::Default`) land here once.
#[allow(dead_code)]
pub(crate) fn is_access_click(event: &AccessEvent) -> bool {
    event.action == accesskit::Action::Click
}

#[cfg(test)]
mod tests {
    use masonry::core::TextEvent;
    use masonry::core::keyboard::{Key, NamedKey};

    use super::{is_access_click, keyboard_activate};

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
}
