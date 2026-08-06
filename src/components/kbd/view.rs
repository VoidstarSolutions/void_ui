//! Xilem view + builder for the `kbd` keycap chip.

/// A keyboard modifier, rendered platform-aware.
///
/// `Cmd` is the cross-platform "primary action" modifier: ⌘ on macOS, Ctrl
/// everywhere else. `Ctrl` is the literal control key (⌃ on macOS). The four
/// variants match issue #228's examples (⌘/Ctrl, ⌥/Alt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    /// Primary action modifier: ⌘ (macOS) / Ctrl (other).
    Cmd,
    /// Literal control key: ⌃ (macOS) / Ctrl (other).
    Ctrl,
    /// ⌥ (macOS) / Alt (other).
    Alt,
    /// ⇧ (macOS) / Shift (other).
    Shift,
}

/// Which symbol vocabulary + separator to render with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Platform {
    Mac,
    Other,
}

/// The host platform's key vocabulary, resolved once at render time.
#[expect(dead_code, reason = "used by KbdView in Task 3")]
fn resolve_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Other
    }
}

/// Modifiers are always emitted in this order, whatever order the caller
/// passed them in — so `[Cmd, Shift]` and `[Shift, Cmd]` render identically,
/// and the sequence matches each platform's own convention (⌃⌥⇧⌘ on macOS).
#[expect(dead_code)]
const CANONICAL_ORDER: [Modifier; 4] = [
    Modifier::Ctrl,
    Modifier::Alt,
    Modifier::Shift,
    Modifier::Cmd,
];

/// The visible glyph/word for a modifier on a platform.
#[expect(dead_code)]
fn display_token(m: Modifier, platform: Platform) -> &'static str {
    match (m, platform) {
        (Modifier::Ctrl, Platform::Mac) => "⌃",
        (Modifier::Alt, Platform::Mac) => "⌥",
        (Modifier::Shift, Platform::Mac) => "⇧",
        (Modifier::Cmd, Platform::Mac) => "⌘",
        (Modifier::Ctrl | Modifier::Cmd, Platform::Other) => "Ctrl",
        (Modifier::Alt, Platform::Other) => "Alt",
        (Modifier::Shift, Platform::Other) => "Shift",
    }
}

/// The spoken word for a modifier, for the accessibility name.
#[expect(dead_code)]
fn spoken_token(m: Modifier) -> &'static str {
    match m {
        Modifier::Cmd => "Command",
        Modifier::Ctrl => "Control",
        Modifier::Alt => "Alt",
        Modifier::Shift => "Shift",
    }
}

/// Modifiers present in `mods`, in canonical order, de-duplicated.
#[expect(dead_code)]
fn ordered_mods(mods: &[Modifier]) -> impl Iterator<Item = Modifier> + '_ {
    CANONICAL_ORDER
        .into_iter()
        .filter(move |m| mods.contains(m))
}

/// The visible chip text: platform symbols/words joined by a thin space
/// (macOS) or `+` (other), with the literal key appended verbatim.
#[expect(dead_code)]
fn compose_display(mods: &[Modifier], key: &str, platform: Platform) -> String {
    let sep = match platform {
        Platform::Mac => "\u{2009}",
        Platform::Other => "+",
    };
    let mut tokens: Vec<&str> = ordered_mods(mods)
        .map(|m| display_token(m, platform))
        .collect();
    tokens.push(key);
    tokens.join(sep)
}

/// The spoken form for the accessibility name: modifier words in canonical
/// order then the key, space-joined, platform-independent. Keeps assistive
/// tech from reading raw glyphs like "⌘".
#[expect(dead_code)]
fn compose_spoken(mods: &[Modifier], key: &str) -> String {
    let mut parts: Vec<&str> = ordered_mods(mods).map(spoken_token).collect();
    parts.push(key);
    parts.join(" ")
}

// TODO: Placeholder for the Kbd builder and View (Task 3)
pub struct Kbd;
#[must_use]
pub fn kbd() -> Kbd {
    Kbd
}

#[cfg(test)]
mod tests {
    use super::{Modifier, Platform, compose_display, compose_spoken};

    const THIN: &str = "\u{2009}";

    #[test]
    fn bare_key_has_no_separators_or_modifiers() {
        assert_eq!(compose_display(&[], "K", Platform::Mac), "K");
        assert_eq!(compose_display(&[], "Enter", Platform::Other), "Enter");
    }

    #[test]
    fn mac_uses_symbols_and_thin_space() {
        let got = compose_display(&[Modifier::Cmd, Modifier::Shift], "K", Platform::Mac);
        assert_eq!(got, format!("⇧{THIN}⌘{THIN}K"));
    }

    #[test]
    fn other_uses_words_and_plus() {
        let got = compose_display(&[Modifier::Cmd, Modifier::Shift], "K", Platform::Other);
        assert_eq!(got, "Shift+Ctrl+K");
    }

    #[test]
    fn modifier_order_is_canonical_regardless_of_input_order() {
        let a = compose_display(&[Modifier::Cmd, Modifier::Shift], "K", Platform::Mac);
        let b = compose_display(&[Modifier::Shift, Modifier::Cmd], "K", Platform::Mac);
        assert_eq!(a, b);
        // Full set emits Ctrl, Alt, Shift, Cmd order → ⌃⌥⇧⌘.
        let all = compose_display(
            &[
                Modifier::Shift,
                Modifier::Cmd,
                Modifier::Alt,
                Modifier::Ctrl,
            ],
            "K",
            Platform::Mac,
        );
        assert_eq!(all, format!("⌃{THIN}⌥{THIN}⇧{THIN}⌘{THIN}K"));
    }

    #[test]
    fn duplicate_modifiers_are_emitted_once() {
        let got = compose_display(&[Modifier::Cmd, Modifier::Cmd], "K", Platform::Other);
        assert_eq!(got, "Ctrl+K");
    }

    #[test]
    fn cmd_maps_to_ctrl_word_on_other() {
        assert_eq!(
            compose_display(&[Modifier::Cmd], "S", Platform::Other),
            "Ctrl+S"
        );
        assert_eq!(
            compose_display(&[Modifier::Cmd], "S", Platform::Mac),
            format!("⌘{THIN}S")
        );
    }

    #[test]
    fn spoken_uses_words_and_is_platform_independent() {
        assert_eq!(
            compose_spoken(&[Modifier::Cmd, Modifier::Shift], "K"),
            "Shift Command K"
        );
        assert_eq!(compose_spoken(&[], "Enter"), "Enter");
    }
}
