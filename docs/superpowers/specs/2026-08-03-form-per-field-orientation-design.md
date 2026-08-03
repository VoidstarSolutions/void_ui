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

One optional field, three builder methods, and effective-orientation resolution
at the two callers of `render_field`. `render_field` itself is unchanged except
for binding the new struct field in its destructure.

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

The caller resolves the effective orientation and passes it to `render_field`
(whose signature already accepts `orientation: FormOrientation`):

- **`Form::render`**, per field:
  ```rust
  let effective = field.orientation.unwrap_or(self.orientation);
  render_field(field, effective, width, theme)
  ```
  Field override wins; otherwise the form-wide orientation.

- **`FormField::render`** (standalone):
  ```rust
  let effective = self.orientation.unwrap_or(FormOrientation::Vertical);
  render_field(self, effective, default_label_width(theme), theme)
  ```
  Honors the field's own override; falls back to Vertical (today's behavior)
  when unset.

`render_field`'s destructure gains `orientation: _` — the field's own value has
already been consumed by the caller to compute the argument, so the helper
ignores the struct field. (This mirrors the compile-break lesson from the
`error` field: the exhaustive destructure stops compiling the instant the struct
grows a field, so it must be bound.)

### `label_width` interaction

`label_width` remains a form-level setting. A field that is horizontal — whether
by inheritance or override — uses the form's `label_width` (or the theme default)
for its label column. A horizontal field inside an otherwise-vertical form
therefore uses the default label width unless the form set one. No per-field
width knob is added.

## Testing

Mirror the existing form test style (builder-state assertions + build-without-panic):

- `form_field(...)` defaults `orientation` to `None`.
- `.orientation(Horizontal)`, `.horizontal()`, `.vertical()` each set the
  expected `Some(..)`.
- A vertical `form(...)` containing one field with `.horizontal()` and one field
  that inherits builds without panic (mixed resolution exercised).
- Standalone `form_field(...).horizontal().render(theme)` builds without panic;
  the plain standalone render (no override) still builds.

Layout is not asserted (consistent with `label`/`group_box`/the rest of `form`).

## Gallery

Add a short "Mixed" section to the form demo panel: one form where most fields
inherit the form orientation and at least one field overrides it, so the
per-field capability is visible alongside the existing vertical/horizontal
sections.

## Files

- `src/components/form/view.rs` — `orientation` field, three builders,
  resolution in `Form::render` and `FormField::render`, `orientation: _` in the
  `render_field` destructure, tests.
- `src/components/form/demo.rs` — mixed-orientation demo section.
- No new theme tokens, no `widget.rs`, no view state.
