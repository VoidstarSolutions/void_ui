//! Xilem view + builder for the `kbd` keycap chip.

use masonry::core::ArcStr;
use masonry::parley::FontFamilyName;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use super::widget::KbdWidget;
use crate::Theme;

/// Symbol-first font stack for the keyboard-glyph styled runs. These faces
/// carry the macOS/Windows keyboard glyph set (⌘ ⇧ ⌫ → …) that the mono
/// faces (Geist Mono) lack; listed ahead of the mono stack so parley selects
/// them for the non-ASCII glyph runs instead of tofu-ing. Mirrors the
/// coverage tail documented in `theme::typography`.
const SYMBOL_FAMILIES: &[&str] = &["Apple Symbols", "Segoe UI Symbol"];

/// Parse a family-name list into parley families, dropping any that fail.
fn parse_families(names: &[&'static str]) -> Vec<FontFamilyName<'static>> {
    names
        .iter()
        .filter_map(|f| FontFamilyName::parse(f))
        .collect()
}

/// The theme's mono stack, applied to the whole label by default.
fn mono_families(theme: &Theme) -> Vec<FontFamilyName<'static>> {
    parse_families(theme.typography.mono.families)
}

/// Symbol faces first, then the mono stack as further fallback — used for the
/// styled runs over keyboard-glyph spans.
fn symbol_families(theme: &Theme) -> Vec<FontFamilyName<'static>> {
    let mut families = parse_families(SYMBOL_FAMILIES);
    families.extend(mono_families(theme));
    families
}

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
pub(super) enum Platform {
    Mac,
    Other,
}

/// The host platform's key vocabulary, resolved once at render time.
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
const CANONICAL_ORDER: [Modifier; 4] = [
    Modifier::Ctrl,
    Modifier::Alt,
    Modifier::Shift,
    Modifier::Cmd,
];

/// The visible glyph/word for a modifier on a platform.
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
fn spoken_token(m: Modifier) -> &'static str {
    match m {
        Modifier::Cmd => "Command",
        Modifier::Ctrl => "Control",
        Modifier::Alt => "Alt",
        Modifier::Shift => "Shift",
    }
}

/// Modifiers present in `mods`, in canonical order, de-duplicated.
fn ordered_mods(mods: &[Modifier]) -> impl Iterator<Item = Modifier> + '_ {
    CANONICAL_ORDER
        .into_iter()
        .filter(move |m| mods.contains(m))
}

/// The visible chip text: platform symbols/words joined by a thin space
/// (macOS) or `+` (other), with the literal key appended verbatim.
fn compose_display(mods: &[Modifier], key: &str, platform: Platform) -> String {
    let sep = match platform {
        Platform::Mac => "\u{2009}",
        Platform::Other => "+",
    };
    // On non-mac, Cmd shows as "Ctrl". Collapse Cmd into Ctrl before ordering
    // so the two never render as "Ctrl+Ctrl" and control sorts to Ctrl's slot
    // (ahead of Alt/Shift) instead of trailing at Cmd's canonical position.
    let normalized: Vec<Modifier> = match platform {
        Platform::Mac => mods.to_vec(),
        Platform::Other => mods
            .iter()
            .map(|&m| {
                if m == Modifier::Cmd {
                    Modifier::Ctrl
                } else {
                    m
                }
            })
            .collect(),
    };
    let mut tokens: Vec<&str> = ordered_mods(&normalized)
        .map(|m| display_token(m, platform))
        .collect();
    tokens.push(key);
    tokens.join(sep)
}

/// The spoken form for the accessibility name: modifier words in canonical
/// order then the key, space-joined, platform-independent. Keeps assistive
/// tech from reading raw glyphs like "⌘".
fn compose_spoken(mods: &[Modifier], key: &str) -> String {
    let mut parts: Vec<&str> = ordered_mods(mods).map(spoken_token).collect();
    parts.push(key);
    parts.join(" ")
}

/// Builder for a keycap chip. Create with [`kbd`], materialize with
/// [`Self::render`].
#[must_use = "Kbd does nothing until rendered with .render(&theme)"]
pub struct Kbd {
    key: ArcStr,
    mods: Vec<Modifier>,
    platform: Option<Platform>,
}

/// Create a keycap chip for `key` (e.g. `"K"`, `"Enter"`, `"F5"`, `"→"`).
///
/// The key label is rendered verbatim; only modifiers are symbol-mapped.
pub fn kbd(key: impl Into<ArcStr>) -> Kbd {
    Kbd {
        key: key.into(),
        mods: Vec::new(),
        platform: None,
    }
}

impl Kbd {
    /// Set the modifiers. Replaces any previously-set modifiers.
    pub fn mods(mut self, mods: impl IntoIterator<Item = Modifier>) -> Self {
        self.mods = mods.into_iter().collect();
        self
    }

    /// Force a specific platform vocabulary instead of resolving the host's.
    /// For the gallery, which shows both branches on any host; production
    /// callers omit this and get the host platform.
    pub(super) fn platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Materialize the xilem view at the supplied theme.
    #[must_use = "View values do nothing unless provided to Xilem."]
    pub fn render<State, Action>(
        self,
        theme: &Theme,
    ) -> impl WidgetView<State, Action> + use<State, Action>
    where
        State: 'static,
        Action: 'static,
    {
        let platform = self.platform.unwrap_or_else(resolve_platform);
        KbdView {
            display: ArcStr::from(compose_display(&self.mods, &self.key, platform)),
            spoken: ArcStr::from(compose_spoken(&self.mods, &self.key)),
            theme: *theme,
        }
    }
}

/// Materialized view for a [`Kbd`]. Not constructed directly.
struct KbdView {
    display: ArcStr,
    spoken: ArcStr,
    theme: Theme,
}

impl ViewMarker for KbdView {}

impl<State: 'static, Action: 'static> View<State, Action, ViewCtx> for KbdView {
    type Element = Pod<KbdWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = KbdWidget::new(
            self.display.clone(),
            &self.theme,
            self.spoken.clone(),
            mono_families(&self.theme),
            symbol_families(&self.theme),
        );
        (ctx.create_pod(widget), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.theme != prev.theme {
            KbdWidget::set_theme(&mut element, &self.theme);
        }
        if self.theme.typography.mono != prev.theme.typography.mono {
            KbdWidget::set_fonts(
                &mut element,
                mono_families(&self.theme),
                symbol_families(&self.theme),
            );
        }
        if self.display != prev.display {
            KbdWidget::set_text(&mut element, self.display.clone());
        }
        if self.spoken != prev.spoken {
            KbdWidget::set_spoken_name(&mut element, self.spoken.clone());
        }
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
        _: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

#[cfg(test)]
mod tests {
    use xilem::ViewCtx;
    use xilem::core::View;

    use super::{Modifier, Platform, compose_display, compose_spoken, kbd};
    use crate::{Theme, test_support};

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
        assert_eq!(got, "Ctrl+Shift+K");
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

    #[test]
    fn builder_stores_key_and_mods() {
        let k = kbd("K").mods([Modifier::Cmd, Modifier::Shift]);
        assert_eq!(k.key.as_ref(), "K");
        assert_eq!(k.mods, vec![Modifier::Cmd, Modifier::Shift]);
    }

    #[test]
    fn mods_replaces_rather_than_appends() {
        let k = kbd("K").mods([Modifier::Cmd]).mods([Modifier::Alt]);
        assert_eq!(k.mods, vec![Modifier::Alt]);
    }

    #[test]
    fn render_builds_without_panicking() {
        let theme = Theme::default();
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut state = ();

        let _ = kbd("K")
            .mods([Modifier::Cmd])
            .render::<(), ()>(&theme)
            .build(&mut ctx, &mut state);
        let _ = kbd("J")
            .mods([Modifier::Cmd, Modifier::Shift])
            .render::<(), ()>(&theme)
            .build(&mut ctx, &mut state);
    }
}
