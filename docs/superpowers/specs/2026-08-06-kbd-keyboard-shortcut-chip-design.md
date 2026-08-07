# `kbd`: keyboard shortcut chip

Implements issue #228 (`228-kbd-keyboard-shortcut-chip`). A Phase 6
specialized component, size S.

## Summary

An inline keycap chip — the HTML `<kbd>` analog. Renders a key combo as a
single raised, bordered, monospace pill with platform-aware symbol mapping
(⌘/Ctrl, ⌥/Alt, …). It pairs with menu rows (`DropdownButton`,
`ContextMenuArea`) and `Tooltip` to show a command's shortcut.

**Presentation only.** It takes a typed key spec as input and renders it.
No key capture, no shortcut binding, no matching against live keyboard
events — that is host logic. Theme-driven: colors, radius, font, and
padding all come from the `Theme` passed to `.render(&theme)`.

Two-layer split per the repo convention: a xilem `View` builder (`view.rs`)
and a masonry `Widget` (`widget.rs`). Unlike `badge` (which is view-only
composition over `sized_box`), `kbd` ships a custom widget because the
raised keycap chrome — an asymmetric bottom "lip" edge — is not
expressible with `sized_box`'s symmetric border.

## Public API

Free function + builder, mirroring `badge`/`pill`:

```rust
use void_ui::{kbd, Modifier};

// bare key
kbd("K").render::<State, Action>(&theme);

// modified combo
kbd("K")
    .mods([Modifier::Cmd, Modifier::Shift])
    .render::<State, Action>(&theme);
```

- `kbd(key: impl Into<ArcStr>) -> Kbd` — the literal main-key label. Free
  text so any key name works: `"K"`, `"Enter"`, `"Esc"`, `"F5"`, `"→"`,
  `"Space"`. The component does not validate or remap the main key; it is
  rendered verbatim (only modifiers are symbol-mapped).
- `Kbd::mods(self, impl IntoIterator<Item = Modifier>) -> Kbd` — zero or
  more modifiers. Calling it replaces (not appends) any prior set.
- `Kbd::render<State, Action>(self, &Theme) -> impl WidgetView<State, Action>`
  — materializes the view wrapping `KbdWidget`.

`Kbd` is `#[must_use = "Kbd does nothing until rendered with .render(&theme)"]`,
matching `Badge`.

### `Modifier`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    /// The primary/action modifier: ⌘ on macOS, Ctrl elsewhere.
    Cmd,
    /// The literal control key: ⌃ on macOS, Ctrl elsewhere.
    Ctrl,
    /// ⌥ on macOS, Alt elsewhere.
    Alt,
    /// ⇧ on macOS, Shift elsewhere.
    Shift,
}
```

Four variants, each carrying a per-platform label. The set is chosen to
match the two examples in issue #228 verbatim (⌘/Ctrl and ⌥/Alt). `Cmd` is
the cross-platform "primary action" modifier — the one that is ⌘ on macOS
and Ctrl everywhere else — and is what most menu shortcuts want. `Ctrl` is
the literal control key, distinct on macOS (⌃) but collapsing to the same
"Ctrl" word as `Cmd` on other platforms; if a caller passes both on a
non-mac platform they will get "Ctrl+Ctrl", which is the caller's
responsibility (it is a nonsensical combo). No `Meta`/`Super`/`Win` variant
in v1 (YAGNI — no current consumer).

## Platform mapping and text composition

A single pure function in `view.rs`, unit-testable on any host by taking the
platform as an explicit parameter rather than reading `cfg!` inline:

```rust
enum Platform { Mac, Other }

fn resolve_platform() -> Platform {
    if cfg!(target_os = "macos") { Platform::Mac } else { Platform::Other }
}

/// The visible chip text (glyphs/thin-space/`+`).
fn compose_display(mods: &[Modifier], key: &str, platform: Platform) -> String;

/// The spoken form for the accessibility name (words, always, space-joined).
fn compose_spoken(mods: &[Modifier], key: &str) -> String;
```

Precedent for `cfg!(target_os = "macos")` as the platform switch:
`src/collection/row_click.rs:125`, `src/components/data_grid/copy_shortcut.rs`,
`src/components/code_view/widget.rs:555`.

### Rules

1. **Canonical modifier order.** Modifiers are always emitted in the order
   `Ctrl, Alt, Shift, Cmd` regardless of the order the caller passed them.
   This yields ⌃⌥⇧⌘ on macOS (the platform's own convention) and
   `Ctrl+Alt+Shift` elsewhere, so `[Cmd, Shift]` and `[Shift, Cmd]` render
   identically. Duplicate modifiers in the input are de-duplicated.

2. **Per-platform tokens** (`compose_display`):

   | `Modifier` | macOS | Other |
   |---|---|---|
   | `Ctrl` | `⌃` | `Ctrl` |
   | `Alt` | `⌥` | `Alt` |
   | `Shift` | `⇧` | `Shift` |
   | `Cmd` | `⌘` | `Ctrl` |

   The main key is appended verbatim as the final token.

3. **Join** (`compose_display`):
   - **macOS:** tokens joined by a thin space `\u{2009}` → `⌃ ⌥ ⇧ K`.
   - **Other:** tokens joined by `+` → `Ctrl+Shift+K`.

4. **Spoken form** (`compose_spoken`, platform-independent): modifier words
   `Control, Alt, Shift, Command` (note `Cmd` → "Command") in canonical
   order, then the key, space-joined → `"Shift Command K"`. Used only for
   the accessibility name so assistive tech never reads raw glyphs like "⌘".

The composed display string is one `ArcStr` handed to a single monospace
`Label`. The widget never sees individual keys — it is purely a chrome
container, which keeps it small.

## Widget: `KbdWidget` (`widget.rs`)

Follows the `button`/`meter` custom-widget pattern. Owns a single child
`Label` (the composed text) plus a `Theme` value, and paints the raised
keycap chrome behind it. Non-interactive: `type Action = NoAction`, no
pointer/keyboard/focus handling.

Fields (roughly):

```rust
pub struct KbdWidget {
    child: WidgetPod<Label>,   // the mono combo text
    theme: Theme,              // read as a value, not via the property stack
    spoken_name: ArcStr,       // accessibility name
}
```

### Theme as a value

Reads `Theme` as a plain value, not the ambient masonry property stack —
the same choice `button/widget.rs` and `meter/widget.rs` document and for
the same reason (no `Theme` is reachable through the property system in this
library). Colors used:

- body fill: a keycap surface color from `theme.palette` (the neutral
  surface/raised token — pick the same one `badge`'s default variant tint
  resolves to, so a `kbd` sits consistently among badges).
- bottom-lip edge: a slightly darker shade of the body (border color, or the
  body darkened) to read as a physical key edge.
- border: `theme.palette` border token, 1px hairline.
- text: mono foreground, sized `theme.typography.size_body`, family
  `theme.typography.mono`, set on the child `Label` in `view.rs`.
- radius: `theme.radius.small`.

### Layout

`register_children` / `children_ids` expose the one `Label` pod.
`measure`/`layout`:

- Measure the child label.
- Add symmetric horizontal padding and vertical padding derived from
  `theme.density` (reuse the same `button_pad_*`/`pad` tokens `badge` uses so
  chip metrics match).
- Reserve **1px extra height at the bottom** for the raised lip, and place
  the child offset up by that 1px so the glyph stays optically centered above
  the lip rather than sinking onto it.

### Paint

`pre_paint` draws the keycap bottom-up (so later draws sit on top), reusing
masonry's `paint_background` helper (as `meter.rs` does) rather than
hand-rolling rounded-rect geometry where it fits:

1. **Bottom lip:** a rounded rect the width of the body, offset ~1px down,
   in the darker edge color — drawn first so the body covers all but the
   exposed bottom sliver, producing the 3D "raised key" edge.
2. **Body:** the rounded-rect fill in the surface color.
3. **Border:** 1px hairline around the body.
4. *(optional, if it reads well)* a 1px lighter inner highlight along the top
   edge for extra dimensionality. Cut if it muddies the look.

The child `Label` paints on top via the normal child-paint pass.

### Accessibility

`accessibility_role` → `Role::Label`. `accessibility` sets the node name to
`spoken_name` (e.g. `"Command Shift K"`), so screen readers announce the
shortcut in words rather than reading unlabeled symbol glyphs.

### Setters (rebuild support)

Value-diff-guarded mutators in the `WidgetMut` style, matching `meter`'s
setter discipline:

- `set_text(this, ArcStr)` — updates the child label text; `request_layout`
  on change (width changes).
- `set_spoken_name(this, ArcStr)` — updates the a11y name; `request_render`
  (or an accessibility-only request) on change.
- `set_theme(this, &Theme)` — re-applies theme to self and child; requests
  the appropriate pass on change.

## View → widget wiring (`view.rs`)

`Kbd::render`:

1. `resolve_platform()`.
2. `compose_display(&mods, &key, platform)` → display `ArcStr`.
3. `compose_spoken(&mods, &key)` → spoken `ArcStr`.
4. Build the mono child `Label` (family `theme.typography.mono`, size
   `size_body`, foreground color).
5. Wrap in `KbdWidget` carrying the theme + spoken name.

`rebuild` re-applies text/spoken-name/theme **only on value change**, through
the setters, honoring the CLAUDE.md theme-diff rule (theme re-applied only
when the value differs).

Follow the `feedback_xilem_view_with_id` rule: since the view owns a child
`WidgetView` (the label), if message routing is involved it must use
`ctx.with_id` + `message.take_first()`. `kbd` emits no actions and the label
is non-interactive, so there is likely no message path — but wire the id
scope the same way the other single-child views do to stay consistent.

## Files

```
src/components/kbd/
  mod.rs      — re-exports: kbd, Kbd, Modifier
  view.rs     — kbd(), Kbd, Modifier, Platform, compose_* , render/rebuild
  widget.rs   — KbdWidget: fields, setters, Widget impl (layout + paint)
  demo.rs     — gallery panel (behind the `gallery` feature)
```

- `src/components/mod.rs`: add `pub mod kbd;` and re-export, placed
  alphabetically (near `input`/`label`).
- `lib.rs` (or wherever the crate root re-exports components): export `kbd`,
  `Kbd`, `Modifier`. Do **not** export `KbdWidget` or `Platform` (internal),
  mirroring how `MeterWidget`/`SeparatorLineView` stay unexported.

## Gallery demo (`demo.rs`)

A `with_source!` panel exercising:

- a bare key (`kbd("F5")`),
- a single modifier (`kbd("S").mods([Modifier::Cmd])`),
- a full combo (`kbd("K").mods([Modifier::Cmd, Modifier::Shift])`),
- a non-letter key (`kbd("Enter")` or `kbd("→")`),
- an in-context example: a fake menu row (label + trailing `kbd`) to show the
  pairing use case.

Because `compose_display` takes `Platform` explicitly, the demo can render
both the macOS and the Other branch side by side (calling the composer with
each platform) so the platform mapping is visible in the gallery without
needing two machines. The live chip itself uses `resolve_platform()`.

Register the panel wherever the gallery aggregates component demos (same spot
`badge`/`meter` register theirs).

## Testing

**`compose_*` unit tests** (the bulk of the value — pure functions):

- canonical ordering: `[Cmd, Shift]` and `[Shift, Cmd]` produce identical
  output; full `[Shift, Cmd, Alt, Ctrl]` emits `⌃⌥⇧⌘`-order on mac and
  `Ctrl+Alt+Shift+…` on other.
- per-platform tokens: each `Modifier` maps to the right glyph (mac) and word
  (other); `Cmd` → `⌘`/`Ctrl`.
- joining: thin space on mac, `+` on other.
- bare key (no mods) on both platforms.
- duplicate modifiers de-duplicated.
- `compose_spoken`: words form, `Cmd` → "Command", platform-independent.

**Widget tests** (mirroring `meter`'s harness tests):

- mounts, lays out, and paints without panicking for bare key and full combo,
  with both mac-form and other-form text.
- `set_text` / `set_theme` diff guards behave (no spurious pass requests when
  the value is unchanged).
- accessibility node reports `Role::Label` and the spoken name.

**View tests** (mirroring `badge`'s):

- `render(...).build(...)` and a `rebuild` do not panic for bare and modified
  chips.

## Non-goals (YAGNI)

- No per-key individual keycaps — one pill per combo (design decision:
  single pill + inner separators).
- No `.separator(..)` override — platform-aware separator is fixed in v1.
- No key capture, event matching, or shortcut binding — host logic.
- No leading/trailing icon slot, no click/dismiss — it is inert chrome.
- No `Meta`/`Super`/`Win` modifier variant.

## Decisions (from the design conversation)

1. **Input API:** typed `Modifier` enum + literal key string, so the
   component (not the host) owns platform symbol mapping — the roadmap's
   stated job. (Rejected: raw string list — pushes mapping to host; single
   combo string — needs a parser + edge-case handling.)
2. **Combo layout:** one outer pill with inner separators. (Rejected:
   one keycap per key; single pill with no separators.)
3. **Chrome:** custom widget with a true raised look (asymmetric bottom lip),
   honoring the two-layer split literally. (Rejected: uniform-border view-only
   composition like `badge` — cannot express the raised lip.)
4. **Separator/symbols:** platform-aware — macOS uses symbol modifiers +
   thin-space join; other platforms use word modifiers + `+` join. (Rejected:
   always-`+`; configurable separator.)
5. **Canonical modifier ordering** and a **words-form accessible name** added
   as low-cost correctness wins, confirmed during design.
