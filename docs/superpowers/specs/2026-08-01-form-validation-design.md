# Form validation — design

Increment on #220 (*Form: layout container for label/control pairing*). Adds
presentation of a validation error state to `FormField`, driven by a
consumer-supplied validator. Still presentation-only: no baked-in rules, no
business/domain logic, no form-owned state.

## Purpose

Let a form field display an error message and error styling when its value is
invalid, while the *rules* that decide validity stay in the consuming app.
void_ui renders the error; the consumer computes it.

## Constraints (why the shape is what it is)

- **Presentation-only** (CLAUDE.md): validation *rules* (email regex,
  required-not-empty, min-length) are business logic and belong in the consumer,
  not in a product-agnostic library. void_ui ships zero rules.
- **Composition-only** (matches the existing form): no custom masonry `Widget`,
  no custom xilem `View`, no view state. The form already rebuilds every cycle,
  so a validator run at build time *is* live validation with no state to own.
- **The control is opaque**: `FormField` holds its control as a type-erased
  `Box<AnyWidgetView>`. The form cannot read the control's value, so the
  consumer must hand the value in alongside the validator. The form also cannot
  restyle the control's interior (see Limitations).

## Scope

**In scope**

- `error: Option<ArcStr>` state on `FormField` — the message to display, if any.
- `.error(msg)` — set the error message directly (server-side errors,
  cross-field checks, async results).
- `.validate(value, rule)` — run `rule` against `value` now and store its
  result. Because the field is rebuilt every cycle, this re-runs live.
- Rendering: when `error` is set, show it as a `palette.danger`,
  `size_caption` caption beneath the control. Error **replaces** the hint.

**Out of scope** (push to later issues)

- Any built-in validators / rules.
- An `invalid` accent on the control itself (danger border on the input). Needs
  `input`/`checkbox`/etc. to grow an error prop — a separate cross-component
  change. See Limitations.
- Touched/submitted gating owned by the form. The form is stateless; the
  consumer suppresses errors by attaching the validator (or `.error`) only when
  it wants them shown.
- Making `required` enforce non-emptiness. `required` stays cosmetic; a consumer
  that wants enforcement writes a one-line rule and passes it to `.validate`.

## Architecture

Composition-only, unchanged from the existing form. One new field, two new
builder methods, one edit to the shared `render_field` helper.

### Data model

```rust
pub struct FormField<State, Action = ()> {
    label: ArcStr,
    control: Box<AnyWidgetView<State, Action>>,
    required: bool,
    hint: Option<ArcStr>,
    error: Option<ArcStr>,   // NEW — displayed error message, if any
}
```

`form_field` initializes `error: None`.

### Builders

```rust
impl<State: 'static, Action: 'static> FormField<State, Action> {
    /// Set an error message directly. For errors the consumer already has in
    /// hand: server-side failures, cross-field checks, async validation.
    pub fn error(mut self, msg: impl Into<ArcStr>) -> Self {
        self.error = Some(msg.into());
        self
    }

    /// Run `rule` against `value` and store its result as this field's error.
    /// Called at build time, i.e. every rebuild -> live validation with no
    /// stored state. `T: ?Sized` so `&str` values work directly.
    pub fn validate<T: ?Sized>(
        mut self,
        value: &T,
        rule: impl FnOnce(&T) -> Option<ArcStr>,
    ) -> Self {
        self.error = rule(value);
        self
    }
}
```

Notes:

- `.validate` **overwrites** `error` with the rule's result (including clearing
  it to `None` when the value is now valid). Last write wins, so
  `.error("x").validate(v, ok_rule)` clears; `.validate(v, ok_rule).error("x")`
  keeps `"x"`. Documented on the methods.
- `FnOnce` — the closure is called exactly once, immediately.
- `rule` returns `Option<ArcStr>`: `Some(message)` = invalid, `None` = valid.
- Generic `T` covers non-text controls (`bool`, `f64`) as well as text.

### Rendering

The only visual change is the caption slot beneath the control in the shared
`render_field` helper. Today:

```rust
let control_cell = match hint {
    Some(hint_text) => flex_col(control, muted_caption(hint_text)),
    None => control,
};
```

New — error takes precedence over hint (they never show together):

```rust
// Error (danger) wins over hint (muted); at most one caption.
let caption = match (&error, &hint) {
    (Some(err), _)       => Some(danger_caption(err.clone())),
    (None, Some(hint))   => Some(muted_caption(hint.clone())),
    (None, None)         => None,
};
let control_cell = match caption {
    Some(c) => flex_col(control, c),  // Stretch, gap density.gap
    None    => control,
};
```

- **Error caption** = `label(msg).text_size(size_caption).color(palette.danger)
  .multiline(true)`.
- **Hint caption** = existing `label(...).color(text_muted)...` — unchanged.
- Wrapping is identical for both, so vertical and horizontal orientations pick
  the error up with no orientation-specific code.

No new theme tokens: `palette.danger` and `typography.size_caption` already
exist and are already used elsewhere (e.g. the required asterisk, the hint).

### Interaction with `required`

`required` and `error` are independent and coexist: the required asterisk still
renders next to the label; the error caption renders below the control. Making a
field `required` does **not** synthesize an error — that would be a baked rule.

## Limitations

- **No control-interior error accent.** Because the control is a type-erased
  view, the form cannot draw a danger border on the input/checkbox itself. The
  error signal is the caption beneath the control. A true `invalid` visual needs
  the individual input components to grow an error prop; tracked as a separate
  cross-component issue, out of scope here.

## Testing

Mirror the existing `form` test style (builder-state assertions +
build-without-panic):

- `.error("msg")` sets `error == Some("msg")`.
- `.validate(value, rule)` with a failing rule sets `error == Some(..)`; with a
  passing rule sets `error == None`.
- `.validate` overwrites a prior `.error` (last-write-wins), and vice versa.
- Build-without-panic: a field with an error, in both orientations; a field with
  both `hint` and `error` set (exercises the precedence branch); a multi-field
  form mixing errored and clean fields.

Rendered-structure assertions (error-below-control, danger color) are not
unit-tested — consistent with `label`/`group_box`/existing form, which don't
assert layout. Visual correctness is deferred to the gallery pass.

## Gallery

Add one live-validated field to the existing form demo panel: an Email field
whose rule flags a missing `@` (or empty) with a message, so the error appears
and clears as the user types. Keep it a local, self-contained rule in `demo.rs`
— no shared validator helper, no new public surface.

## Files

- `src/components/form/view.rs` — `error` field, `.error`/`.validate` builders,
  `render_field` caption change, tests.
- `src/components/form/demo.rs` — one validated field.
- `src/components/form/mod.rs` — no change (methods, not new exports).
- No new theme tokens, no `widget.rs`, no view state.
