# Tag / Chip (#222) — deliver by extending `Badge`

**Status:** approved design, pre-implementation
**Issue:** #222 — "Tag / Chip: semantic colored pill" (Phase 3 feedback, size S)
**Branch:** `222-tag-chip-semantic-colored-pill`

## Summary

Issue #222 asks for a semantic colored pill (info/success/warning/error +
neutral) with an optional leading icon and optional dismiss, "distinct from
`Badge` (count/dot)", built as a two-layer split with a gallery demo.

That premise is stale. In the current codebase:

- `Badge` (`src/components/badge/view.rs`) is **already** the semantic colored
  pill: it maps `AlertVariant` to themed `fg`/`bg`/`border` (`info`/`success`/
  `warning`/`danger` + neutral `Default`), supports `Rounded` and `Pill`
  shapes, and is pure composition over `label` inside a `sized_box`.
- The count/dot role is filled by `StatusDot` (`status_dot`), not `Badge`.

So the two components #222 wants to keep "distinct" are the same component.
The only capabilities the issue names that `Badge` lacks are a **leading icon**
and a **dismiss control**.

**Decision (approved):** do not create a new `Tag`/`Chip` component. Extend the
existing `Badge` with an optional leading icon and an optional dismiss handler.
No new folder, no new masonry widget — stay pure composition, matching how
`Alert` provides `on_close`.

### Why not a separate component

A parallel `Tag` would duplicate `Badge`'s variant→color mapping, shape logic,
and chrome, then differ only by having an icon and an X button — capabilities
that belong on the pill primitive itself. A second near-identical pill is the
kind of one-app-shaped duplication `CLAUDE.md` tells us to push back on. The
issue's "two-layer split" instruction assumes a stateful widget; dismiss here
is a composed button (see `Alert`), so no custom widget is warranted.

## Scope

**In scope**

- Extend `Badge` builder with `.icon(IconName)` (optional leading icon).
- Extend `Badge` builder with `.on_dismiss(f)` (optional trailing X button).
- Row layout inside existing chrome so icon/label/dismiss sit inline.
- Gallery demo additions exercising icon + dismissible badges.
- Unit tests mirroring the existing `badge` tests.
- Documentation reconciliation (module docs + README roadmap/taxonomy).

**Out of scope**

- No new `tag/` module, no `Tag`/`Chip` public names, no masonry widget.
- No changes to `AlertVariant` or the palette.
- No count/dot behavior on `Badge` (that stays `StatusDot`).
- No multi-select / tag-input group container (not requested by #222).

## API

Existing constructors and methods are unchanged and remain source-compatible:

```rust
badge("Draft").render::<(), ()>(&theme);
pill("Active").variant(AlertVariant::Success).render::<(), ()>(&theme);
```

New builder methods:

```rust
// Optional leading icon, tinted to the variant foreground, caption-sized.
badge("Active")
    .variant(AlertVariant::Success)
    .icon(IconName::CheckCircle)
    .render::<(), ()>(&theme);

// Optional trailing dismiss (X) button firing a state callback.
pill("api")
    .variant(AlertVariant::Info)
    .icon(IconName::Tag)
    .on_dismiss(|s: &mut State| s.remove_tag())
    .render(&theme);
```

- `fn icon(self, name: IconName) -> Self` — stores `Option<IconName>`; default
  `None`. Tinted with the variant `fg`, sized to `typography.size_caption`.
- `fn on_dismiss<F>(self, f: F) -> Badge<F>` — presence of the callback adds a
  trailing X button. Absent by default (no dismiss control, current behavior).

## Design

### Optional-callback generic (mirror `Alert`)

`Badge` becomes generic over the dismiss-callback type with a `()` default, so
callers that never dismiss keep writing `badge("x")` with clean inference and
pay no boxing:

```rust
pub struct Badge<C = ()> {
    text: ArcStr,
    variant: AlertVariant,
    shape: Shape,
    icon: Option<IconName>,
    on_dismiss: C,
}
```

Follow the established per-component pattern: `Alert` and `Dialog` each define
their own small `CloseCallback<State, Action>` trait with a no-op blanket
`impl … for ()` and an `impl` for `F: Fn(&mut State)`. `Badge` defines its own
local equivalent (e.g. `DismissCallback`) rather than widening `alert`'s API.
`.on_dismiss::<F>()` returns `Badge<F>`; the render path calls
`self.on_dismiss.call(state)` inside the dismiss button's callback.

`badge()` / `pill()` constructors return `Badge<()>` (dismiss defaults to `()`,
icon to `None`), so all current call sites and the doctest keep compiling.

### Layout

The single-child `sized_box` becomes a `sized_box` wrapping a `flex_row` (using
the `flex_row` view + `CrossAxisAlignment::Center`, as `Alert` does):

```
sized_box(
    flex_row([ leading_icon?, label, dismiss_button? ])
)
.padding(...).background_color(bg).border(border, 1px).corner_radius(radius)
```

- Chrome (padding, background, border, radius, shape) is unchanged.
- When both `icon` is `None` and `on_dismiss` is `()`, the row holds only the
  label — visually identical to today's badge. Existing badges render the same.
- `leading_icon` = `icon(name).color(fg).size(size_caption)`; small gap before
  the label (theme spacing) only when present.
- `dismiss_button` = `button(move |s| on_dismiss.call(s)).icon(IconName::X)`
  styled as an unobtrusive/ghost variant tinted to `fg`, mirroring `Alert`'s
  close button (`alert/view.rs` ~L230). Rendered only when `C != ()`.

### Color / theme

No palette changes. Continue using `AlertVariant::colors(&theme.palette)` for
`(fg, bg, border)`. Icon and dismiss glyph both use `fg` so the pill reads as
one semantic unit.

## Testing

Extend `badge/view.rs` `#[cfg(test)]` mirroring existing tests:

- `icon` defaults to `None`; `.icon(..)` stores it.
- `.on_dismiss(..)` yields a `Badge` whose callback fires (type-param changed
  from `()`), following `alert`'s `on_close` test shape.
- Badge with icon + dismiss builds without panicking (extend the existing
  build-smoke test) for both `badge` and `pill`.

## Gallery demo

Extend `badge/demo.rs` (do not add a new panel/component) with rows showing: a
variant with a leading icon, and a dismissible pill wired to component-local
demo state that removes the tag on dismiss (per the local-state demo pattern).

## Documentation reconciliation

The README carries the same stale taxonomy and must be corrected as part of
this change:

- `README.md` line ~56 `**\`Badge\`** — count/dot pill` and line ~124 table row
  `Badge … Count/dot overlay.` describe `Badge` as count/dot, which is wrong —
  that is `StatusDot`. Correct `Badge`'s description to "semantic colored pill
  (optional leading icon, optional dismiss)".
- Resolve the `Tag` / `Chip` roadmap row (line ~127) and the "remaining"/open
  list (line ~83, ~158): #222 is delivered by `Badge`, not a separate `Tag`.
  Mark it accordingly (e.g. fold into `Badge`, drop from the open list) rather
  than leaving a `—` for a component that will not exist.
- Update the `badge` module doc-comment (`badge/view.rs` top) to mention the
  leading icon and dismiss.

## Risks / notes

- Source compatibility hinges on the `C = ()` default and unchanged
  constructors; the existing module doctest is the guard that this holds.
- `flex_row` with a single label child must not introduce extra padding/spacing
  vs the current direct-child `sized_box`; verify existing badges look
  unchanged in the gallery.
- `clippy::pedantic` is denied workspace-wide — the new generic/trait code must
  be pedantic-clean (no `allow`).
```
