# Per-field orientation — design

Increment on #220 (*Form: layout container for label/control pairing*). Lets an
individual `FormField` choose its own orientation (label above vs. beside the
control), overriding the form-wide setting. Still presentation-only,
composition-only, no new theme tokens.

## Purpose

Today orientation is form-wide: `form(...).horizontal()` applies to every field.
Real forms mix layouts — a wide text area reads better with its label above it
even in an otherwise-horizontal form, and vice versa. This adds a per-field
override that wins over the form's setting when present, and inherits it
otherwise.

## Constraints (unchanged from the form)

- **Presentation-only** (CLAUDE.md): layout choice only, no logic.
- **Composition-only**: no custom masonry `Widget`, no custom xilem `View`, no
  view state. `render_field` already takes an `orientation` argument — the
  plumbing is in place; this change only decides *which* value is passed.
- **No new theme tokens.**

## Scope

**In scope**

- `orientation: Option<FormOrientation>` on `FormField` — `None` means inherit
  the form's orientation.
- `FormField::orientation(o)`, `FormField::horizontal()`, `FormField::vertical()`
  — mirror the existing `Form` builder names; each sets `Some(..)`.
- Resolution at the two call sites that build rows.
- Standalone `FormField::render` honors the field's override, else Vertical.

**Out of scope** (unchanged)

- Per-field `label_width` — the label-column width stays a form-level setting.
- Baseline alignment, auto-sizing the label column, validation changes.

## Architecture

One optional field, three builder methods, and a pure `resolve_orientation`
helper called at the two callers of `render_field` to compute each field's
effective orientation. `render_field` itself is unchanged except for binding the
new struct field in its destructure.

### Data model

```rust
pub struct FormField<State, Action = ()> {
    label: ArcStr,
    control: Box<AnyWidgetView<State, Action>>,
    required: bool,
    hint: Option<ArcStr>,
    error: Option<ArcStr>,
    orientation: Option<FormOrientation>,   // NEW — None = inherit the form's
}
```

`form_field` initializes `orientation: None`.

### Builders

```rust
impl<State: 'static, Action: 'static> FormField<State, Action> {
    /// Override this field's orientation, ignoring the form's. Unset by
    /// default, in which case the field inherits the form's orientation.
    pub fn orientation(mut self, orientation: FormOrientation) -> Self {
        self.orientation = Some(orientation);
        self
    }

    /// Shorthand for `.orientation(FormOrientation::Horizontal)`.
    pub fn horizontal(mut self) -> Self {
        self.orientation = Some(FormOrientation::Horizontal);
        self
    }

    /// Shorthand for `.orientation(FormOrientation::Vertical)`.
    pub fn vertical(mut self) -> Self {
        self.orientation = Some(FormOrientation::Vertical);
        self
    }
}
```

These share names with the `Form` builder methods; that is intentional
symmetry — they live on a different type, so there is no conflict.

### Resolution

Resolution is a single pure function, so precedence is unit-testable directly
(not only through build-without-panic):

```rust
/// Effective orientation for a field: the field's own override if set,
/// otherwise the surrounding form's orientation.
fn resolve_orientation(
    field: Option<FormOrientation>,
    form: FormOrientation,
) -> FormOrientation {
    field.unwrap_or(form)
}
```

Both callers resolve through it and pass the result to `render_field` (whose
signature already accepts `orientation: FormOrientation`):

- **`Form::render`**, per field:
  `resolve_orientation(field.orientation, self.orientation)` — field override
  wins; otherwise the form-wide orientation.
- **`FormField::render`** (standalone):
  `resolve_orientation(self.orientation, FormOrientation::Vertical)` — honors
  the field's own override; falls back to Vertical (today's behavior) when unset.

`render_field`'s destructure gains `orientation: _` — the field's own value has
already been consumed by the caller to compute the argument, so the helper
ignores the struct field. (This mirrors the compile-break lesson from the
`error` field: the exhaustive destructure stops compiling the instant the struct
grows a field, so it must be bound.)

### `label_width` interaction

`label_width` remains a form-level setting, but it applies per *resolved*
orientation, not per form orientation. Every field that **resolves to
`Horizontal`** — whether by inheriting a horizontal form or by overriding a
vertical form with `.horizontal()` — uses the form's `label_width` (or the theme
default) for its label column. A field resolving to `Vertical` ignores it. So a
horizontal override inside an otherwise-vertical form does get the label column;
it uses the default width unless the form set one. No per-field width knob is
added.

### Orientation-dependent doc comments to correct

These existing comments are written form-centrically and become inaccurate under
the resolved model. Update the wording only — behavior is unchanged:

- `FormField::render` — currently "always vertical". Now: uses the field's own
  orientation if set, otherwise Vertical.
- `Form::label_width` — currently "Horizontal orientation only; ignored when
  vertical." Now: applies to every field that resolves to `Horizontal`
  (including a `.horizontal()` override inside a vertical form); ignored for
  fields resolving to `Vertical`.
- `default_label_width` — currently "for horizontal forms". Now: for fields
  resolving to horizontal.

## Testing

Builder-state assertions, **direct resolution assertions**, and
build-without-panic:

Builder state:
- `form_field(...)` defaults `orientation` to `None`.
- `.orientation(Horizontal)`, `.horizontal()`, `.vertical()` each set the
  expected `Some(..)`.

Resolution (`resolve_orientation`) — assert the *resolved* value across the full
precedence matrix, so a reversed-precedence bug is caught (build-without-panic
alone would not):
- Default field (`None`) inheriting a **vertical** form resolves to `Vertical`.
- Default field (`None`) inheriting a **horizontal** form resolves to `Horizontal`.
- A `.horizontal()` override against a **vertical** form resolves to `Horizontal`.
- A `.vertical()` override against a **horizontal** form resolves to `Vertical`.
- Standalone default (`None`, form defaulted to `Vertical`) resolves to `Vertical`.

`label_width` rule coverage: because `label_width` applies exactly to fields
resolving to `Horizontal`, the "horizontal override against a vertical form
resolves to `Horizontal`" assertion above *is* the coverage that such a field
receives the label column — no separate structural layout assertion is needed
(and none is feasible without adding a layout accessor, which composition-only
forbids).

Build-without-panic (render path):
- A vertical `form(...)` with one `.horizontal()` field and one inheriting field.
- A horizontal `form(...)` with one `.vertical()` field and one inheriting field.
- Standalone `form_field(...).horizontal().render(theme)`, and the plain
  standalone render (no override).

Layout geometry itself is not asserted (consistent with
`label`/`group_box`/the rest of `form`); resolution is asserted via the pure
helper instead.

## Gallery

Add a short "Mixed" section to the form demo panel: one form where most fields
inherit the form orientation and at least one field overrides it, so the
per-field capability is visible alongside the existing vertical/horizontal
sections.

## Files

- `src/components/form/view.rs` — `orientation` field, three builders, the
  pure `resolve_orientation` helper used by `Form::render` and
  `FormField::render`, `orientation: _` in the `render_field` destructure,
  corrected orientation-dependent doc comments (`FormField::render`,
  `Form::label_width`, `default_label_width`), tests (builder state, resolution
  matrix, build-without-panic).
- `src/components/form/demo.rs` — mixed-orientation demo section.
- No new theme tokens, no `widget.rs`, no view state.
