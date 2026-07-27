# `label`: text-decoration (underline/strikethrough) support

## Context

Issue #127. citadel's workbench results table renders failed research cells
struck through (an `ObjectiveScore::Failed { reason }` cell/row, with the
failure reason on hover). `void_ui::label` has no text-decoration API, so
citadel composes a Unicode combining-strikethrough (U+0336) per glyph as a
workaround (`citadel crates/ui/src/workbench/results.rs`) — font/renderer
-dependent, doesn't compose with selection or wrapping. The issue explicitly
asks for a real decoration prop rather than that workaround, and floats "a
small `TextDecoration` enum shared across text-bearing widgets."

How citadel surfaces the failure *reason* on hover (a tooltip or similar) is
out of scope here — void_ui only needs to expose the decoration primitive on
`label`; composing a hover affordance around it is citadel's job, per this
repo's presentation-only, product-agnostic boundary (`CLAUDE.md`).

**The gap, confirmed by reading the vendored source (Cargo.lock-pinned
commit `c5950bcb03d4f3d187a20d1159f6aa276fd056bf` of
`github.com/linebender/xilem`):**

- `parley::style::StyleProperty<B>` already has full decoration support:
  `Underline(bool)`, `UnderlineOffset`/`UnderlineSize`/`UnderlineBrush`, and
  the `Strikethrough` equivalents
  (`parley-0.8.0/src/style/mod.rs:92-107`). No generic `TextDecoration`/
  `Overline` variant exists — only underline and strikethrough.
- `masonry::widgets::Label` (the actual widget) already fully supports
  setting these via its generic `with_style` (build-time builder method) /
  `insert_style` (`WidgetMut`-based, rebuild-time) API
  (`masonry/src/widgets/label.rs`) — it even pattern-matches
  `UnderlineBrush`/`StrikethroughBrush` internally to guard against
  non-default brush indices, proof the properties are already routed
  through and functional today at the widget level.
- The gap is entirely in the upstream **xilem convenience view wrapper**,
  `xilem_masonry::view::label::Label` (what void_ui's `label/view.rs` calls
  as `xl_label`). Its field list
  (`xilem_masonry/src/view/label.rs:53-64`) is `text_alignment, text_size,
  weight, enable_hinting, line_height, font, letter_spacing, word_spacing`
  — no decoration fields, and a literal `// TODO: add more attributes of
  masonry::widgets::Label` comment. There is no generic escape hatch either:
  `xilem_masonry`'s `Style` trait (`xilem_masonry/src/style.rs`, imported in
  `label/view.rs` as `use xilem::style::Style as _`) only covers the
  separate `masonry::properties`/`UsesProperty` system (`ContentColor`,
  `LineBreaking`, `Background`, etc. — what `.color()`/`.line_break_mode()`
  use today), which is unrelated to parley's per-run `StyleProperty`
  mechanism that `Underline`/`Strikethrough` belong to.

Three ways to close that gap were considered:

1. **(Chosen) void_ui writes its own small internal `View`** that
   constructs `masonry::widgets::Label` directly, bypassing `xl_label()`
   for this component, following the exact `with_style`/`insert_style`
   pattern `xilem_masonry`'s own `Label` view already uses internally.
   Fully within void_ui's control, no external merge dependency.
2. **Contribute the fields upstream to `xilem_masonry::view::label::Label`.**
   This is arguably the "real" fix and worth doing separately, but this repo
   doesn't control merge timing on an external repo, and `CLAUDE.md`'s
   stance on the linebender deps is to migrate to upstream changes as they
   land, not block local work waiting on them.
3. **Keep citadel's Unicode combining-mark workaround.** Rejected — this is
   exactly what issue #127 says to stop doing.

(User-confirmed during design: approach 1.)

## Goals

- A `TextDecoration` enum (`None` / `Underline` / `Strikethrough`) usable on
  `void_ui::label`, rendering a real parley-native decoration rule rather
  than a combining-mark workaround.
- Works for both single-line and multiline (`.multiline(true)`) labels —
  decoration is a per-run text style, orthogonal to wrap mode.
- Decoration is settable independently on `label`'s main text and its
  optional `.secondary()` text (e.g. strike through a failed value without
  striking its muted status label).
- Decoration always renders in the text's own color — no separate color
  override knob.

## Non-goals

- No `Overline` variant, no offset/size/brush customization — parley
  supports these but no cited use case needs them; `TextDecoration` exposes
  exactly the two variants issue #127 asks for. Can extend later without a
  breaking change (adding an enum variant plus optional builder methods).
- No combining both underline and strikethrough on one run — `label` never
  needs both at once; `TextDecoration` stays a plain exclusive enum
  (`.decoration(TextDecoration)` replaces whatever was set before), not a
  bitflag.
- No hover/tooltip affordance for a decoration's "reason" — citadel's
  concern, not void_ui's (see Context).
- No changes to any other text-bearing component. `TextDecoration` is
  defined in a shared location (see Design) so a future component can adopt
  it, but `label` is the only consumer this issue requires.

## Decisions

**1. `TextDecoration` lives in a new shared module, `src/text_style.rs`,
not inside `components/label/`.** The issue explicitly frames this as
"shared across text-bearing widgets," and `label` isn't the only
text-bearing component in principle. `src/theme/` was considered and
rejected — its types (`Palette`, `Typography`, `Density`, `Radii`,
`ThemeVariant`) are specifically theme-driven defaults, not a per-call style
choice like decoration; a new small standalone module keeps that boundary
intact. (User-confirmed during design.)

**2. `label` gets its own internal `View`, replacing `xl_label` entirely,
rather than forking upstream or attempting a post-hoc wrapper.** A "wrap
the already-built `Pod<widgets::Label>` and mutate it after the fact"
approach was considered and rejected: `View::build` returns an already
-constructed `Pod`, and `insert_style` requires a `WidgetMut` (only
available inside the tree, i.e. at rebuild time, or via a fresh
`Widget::with_style` call at construction) — there's no way to inject an
extra style property onto an already-built inner view's `Pod` without
reimplementing its `build`/`rebuild`. Since reimplementing was unavoidable
either way, void_ui's internal view replicates upstream's `Label` view
directly (see Design) rather than wrapping it.

**3. Decoration is settable independently on main vs. secondary text.**
(User-confirmed during design, overriding the simpler "one setting for
both" default other `Label` knobs like `.multiline()`/`.masked()` use —
this one has a real motivating case: striking a failed value without
striking the muted status text beside it.)

## Design

### `src/text_style.rs` (new)

```rust
//! Shared text-styling types usable across text-bearing components.

/// A single text decoration rule: underline, strikethrough, or none.
///
/// Renders as a real parley-native decoration line rather than a
/// font/renderer-dependent character-composition workaround (e.g. Unicode
/// combining marks). Always matches the decorated text's own color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextDecoration {
    /// No decoration. Default.
    #[default]
    None,
    /// A line under the text.
    Underline,
    /// A line through the text.
    Strikethrough,
}
```

Re-exported from `lib.rs` alongside `Theme` (same tier as other free
-standing public types, not nested under `components::`).

### `src/components/label/styled_text.rs` (new, private module)

A `View` that constructs `masonry::widgets::Label` directly, replacing
`label`'s current use of `xl_label`. It replicates every field void_ui's
`Label` already forwards to `xl_label` today — `text_alignment, text_size,
letter_spacing, font, line_height` — using upstream's exact
`with_style`/`insert_style` diffing pattern
(`xilem_masonry/src/view/label.rs`), plus the two new decoration-derived
style properties. Fields upstream's wrapper exposes but void_ui's `Label`
never has (`weight`, `enable_hinting`, `word_spacing`) are hardcoded to the
same defaults upstream uses (`FontWeight::NORMAL`, hinting on, word-spacing
`0.0`) — no new surface, no behavior change for existing callers.

```rust
pub(super) struct StyledLabel {
    label: ArcStr,
    text_alignment: TextAlign,
    text_size: f32,
    letter_spacing: f32,
    font: FontFamily<'static>,
    line_height: LineHeight,
    decoration: TextDecoration,
}

pub(super) fn styled_label(label: impl Into<ArcStr>, decoration: TextDecoration) -> StyledLabel {
    StyledLabel {
        label: label.into(),
        text_alignment: TextAlign::default(),
        text_size: masonry::theme::TEXT_SIZE_NORMAL,
        letter_spacing: 0.0,
        font: FontFamily::Single(FontFamilyName::Generic(GenericFamily::SystemUi)),
        line_height: LineHeight::default(),
        decoration,
    }
}

impl StyledLabel {
    pub(super) fn text_alignment(mut self, v: TextAlign) -> Self { self.text_alignment = v; self }
    pub(super) fn text_size(mut self, v: f32) -> Self { self.text_size = v; self }
    pub(super) fn letter_spacing(mut self, v: f32) -> Self { self.letter_spacing = v; self }
    pub(super) fn font(mut self, v: FontFamily<'static>) -> Self { self.font = v; self }
    pub(super) fn line_height(mut self, v: LineHeight) -> Self { self.line_height = v; self }
}
```

`decoration` maps to style properties as two independent booleans (always
set both, so switching variants correctly clears the other):

```rust
StyleProperty::Underline(matches!(self.decoration, TextDecoration::Underline)),
StyleProperty::Strikethrough(matches!(self.decoration, TextDecoration::Strikethrough)),
```

`build`/`rebuild`/`teardown`/`message` otherwise mirror
`xilem_masonry::view::label::Label`'s implementation field-for-field
(diffing each field in `rebuild`, calling `insert_style`/`set_text`/
`set_text_alignment` as appropriate; `message` logs and returns
`MessageResult::Stale`, matching the fact that a label consumes no
messages).

Because `StyledLabel`'s `Element` is `Pod<masonry::widgets::Label>` (same
as `xl_label`'s), the existing `.color()` / `.line_break_mode()` calls in
`Label::single` keep working unchanged — those come from `xilem_masonry`'s
blanket-impl'd `Style` trait keyed on the widget type, not from whichever
view constructed it.

### `src/components/label/view.rs` (changed)

`Label` gains two fields and two builder methods:

```rust
pub struct Label {
    // ...existing fields...
    decoration: TextDecoration,
    secondary_decoration: TextDecoration,
}

impl Label {
    /// Set a text decoration (underline/strikethrough) on the main text.
    /// Defaults to `TextDecoration::None`.
    pub fn decoration(mut self, decoration: TextDecoration) -> Self {
        self.decoration = decoration;
        self
    }

    /// Set a text decoration on the secondary text, independently of the
    /// main text's decoration. No effect unless `.secondary()` is also set.
    pub fn secondary_decoration(mut self, decoration: TextDecoration) -> Self {
        self.secondary_decoration = decoration;
        self
    }
}
```

`single()` gains a `decoration: TextDecoration` parameter and calls
`styled_label(text, decoration)` in place of `xl_label(text)`; `render()`
passes `self.decoration` for the main text call and
`self.secondary_decoration` for the secondary text call.

### `src/components/label/demo.rs` (changed)

New "Decoration" section, following the existing one-section-per-feature
pattern (Sizes, Colors, Secondary, Masked, Multiline, Alignment), showing:
underlined text, struck-through text, and one row combining `.secondary()`
with independent `.decoration()`/`.secondary_decoration()` values — wrapped
in `with_source!` like every other panel section.

## Testing plan

`label/view.rs` unit tests (extending the existing `#[cfg(test)] mod
tests`):
- Builder methods set `decoration`/`secondary_decoration` correctly;
  default for both is `TextDecoration::None`.
- Extend the existing
  `plain_secondary_masked_and_multiline_labels_build_without_panicking`
  test (or add a sibling) to build labels with `.decoration(Underline)`,
  `.decoration(Strikethrough)`, each combined with `.multiline(true)`, and
  one with `.secondary("...")` where `.decoration()` and
  `.secondary_decoration()` differ — confirming no panic and that the two
  fields are independent.

No new widget-level tests: `masonry::widgets::Label` itself is unmodified
and already exercised upstream; only the view-layer construction is new.

## Open questions

None — the upstream-gap investigation and the three scope questions
(shared-enum location, independent secondary decoration, decoration always
matching text color) were resolved during design with the user.
