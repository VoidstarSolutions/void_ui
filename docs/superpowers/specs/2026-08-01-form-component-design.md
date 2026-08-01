# Form component — design

Issue: #220 — *Form: layout container for label/control pairing*. Phase 1 form
primitive, Size S. Presentation only: no validation, no business logic, no
ambient state. Alignment and spacing driven by `Theme`.

## Purpose

Pair a `Label` with a control (input, checkbox, slider, …) in a consistent,
theme-driven layout, and stack several such pairs into a form. Two orientations:
labels above controls (vertical) or beside them in an aligned column
(horizontal).

## Scope

**In scope**

- `form_field(label, control)` row primitive: one label/control pair.
- `.required(bool)` — presentation-only required marker (trailing asterisk in
  `palette.danger`). No validation attached.
- `.hint(text)` — muted caption under the control.
- `form(vec![...])` container: stacks fields, owns orientation + label-column
  width.
- Two orientations: `Vertical` (default) and `Horizontal`.

**Out of scope** (push to later issues)

- Validation / error state / error messages.
- Field grouping, sections, fieldset legends (`group_box` already covers titled
  grouping).
- Baseline alignment of label to control text; horizontal labels align to the
  top of the control cell.
- Auto-sizing the label column to the widest label (see "Alternatives").

## Architecture

Composition-only, matching `label` and `group_box`: **no custom masonry
`Widget`, no custom xilem `View`, no view state.** Both `render` methods build
from the existing themed `label` plus xilem's built-in `flex_row`, `flex_col`,
and `sized_box`, and return a type-erased `Box<AnyWidgetView<State, Action>>`.

Because every child control lives inside built-in flex sequences (never a
hand-written `View` impl), the `ctx.with_id` + `message.take_first()` routing
requirement does not apply here — flex view-sequences already give each child a
distinct action path, so N controls with distinct callbacks route correctly.
This is the reason to stay composition-only rather than introduce a widget.

No `widget.rs` in this component (same as `group_box`).

### Types

```rust
/// Orientation of label relative to control.
pub enum FormOrientation {
    /// Label above control. Default.
    Vertical,
    /// Label beside control, in a fixed-width column.
    Horizontal,
}
// Default = Vertical.

/// One label/control pair. Created with `form_field`.
pub struct FormField<State, Action> {
    label: ArcStr,
    control: Box<AnyWidgetView<State, Action>>,
    required: bool,
    hint: Option<ArcStr>,
}

/// Container stacking fields. Created with `form`.
pub struct Form<State, Action> {
    fields: Vec<FormField<State, Action>>,
    orientation: FormOrientation,
    label_width: Option<Length>,
}
```

### Constructors and builders

```rust
pub fn form_field<State, Action>(
    label: impl Into<ArcStr>,
    control: impl WidgetView<State, Action> + 'static,
) -> FormField<State, Action>;

impl<State, Action> FormField<State, Action> {
    pub fn required(self, on: bool) -> Self;
    pub fn hint(self, text: impl Into<ArcStr>) -> Self;
    /// Standalone render: always Vertical, default label width.
    pub fn render(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>>;
}

pub fn form<State, Action>(
    fields: Vec<FormField<State, Action>>,
) -> Form<State, Action>;

impl<State, Action> Form<State, Action> {
    pub fn orientation(self, o: FormOrientation) -> Self;
    pub fn vertical(self) -> Self;   // convenience
    pub fn horizontal(self) -> Self; // convenience
    /// Fixed label-column width, horizontal orientation only. No effect when
    /// vertical. Defaults to a theme-derived width when unset.
    pub fn label_width(self, w: Length) -> Self;
    pub fn render(self, theme: &Theme) -> Box<AnyWidgetView<State, Action>>;
}
```

The control is type-erased to `Box<AnyWidgetView>` inside `form_field` so a
`Vec<FormField>` can hold heterogeneous controls (input, checkbox, slider). All
fields in one form share the same `State`/`Action`.

### Rendering

A single shared internal helper renders one field given
`(orientation, label_width, theme)`; `FormField::render` calls it with
`(Vertical, default_width)`, and `Form::render` calls it per field with the
container's settings. This keeps row layout in one place and makes the row
independently usable and testable.

Per-field build:

- **label row** = `flex_row(label(text)…, [danger "*" when required])`
  cross-aligned `Center`, small gap. The `*` is a separate
  `label("*").color(palette.danger)`; the main label uses default text color and
  `size_body`.
- **control cell** = `control` alone, or `flex_col(control, hint)` when a hint
  is set. Hint = `label(text).text_size(size_caption).color(text_muted)
  .multiline(true)`.
- **Vertical**: `flex_col(label_row, control_cell)`, cross-aligned `Stretch`,
  gap `density.pad`.
- **Horizontal**: `flex_row(sized_box(label_row).width(label_width),
  control_cell)`, cross-aligned `Start` (label pinned to top of the cell), gap
  `density.gap_lg`.

Container build: `flex_col(all fields)`, cross-aligned `Stretch`, gap
`density.gap_lg`.

### Theme mapping

| Element                        | Source                       |
|--------------------------------|------------------------------|
| Label text                     | `label` defaults (`palette.text`, `size_body`) |
| Required `*`                   | `palette.danger`             |
| Hint text                      | `palette.text_muted`, `typography.size_caption`, multiline |
| Label→control gap (vertical)   | `density.pad`                |
| Label→control gap (horizontal) | `density.gap_lg`             |
| Field→field gap                | `density.gap_lg`             |
| Default label-column width     | theme-derived constant (chosen at implementation; a fixed multiple of `density.pad`) |

No new theme tokens are added. Exact default `label_width` value is settled
during implementation against the gallery.

## Alternatives considered

- **Masonry `Grid` widget** for true auto-sized 2-column layout. Rejected:
  heavier, not composition-only, over-scoped for Size S.
- **Custom widget measuring the widest label** to auto-size the column.
  Rejected: breaks pure composition, most complex; fixed `label_width` is
  predictable and sufficient.
- **Tuple of fields** (`form((a, b, c))`) matching xilem's `flex_row` idiom.
  Rejected in favor of `Vec` — simpler for N heterogeneous rows, no arity cap.

## Testing

Mirror `label` / `group_box` test style (unit + build-without-panic):

- `form_field` builder sets `required` / `hint` fields as expected; defaults are
  `required == false`, `hint == None`.
- `FormOrientation::default()` is `Vertical`; `.horizontal()` / `.vertical()` /
  `.orientation()` set it.
- Build-without-panic across the matrix: vertical and horizontal; with and
  without required; with and without hint; a multi-field form mixing control
  types (e.g. input + checkbox).
- `FormField::render` (standalone) builds without panic.

## Gallery

A `form/demo.rs` panel using `with_source!`, shown before the API is considered
final: a vertical form and a horizontal form, each with a required field and a
field carrying a hint, exercising at least two control types.

## Files

- `src/components/form/view.rs` — builders + `render` (all logic).
- `src/components/form/demo.rs` — gallery panel.
- `src/components/form/mod.rs` — re-exports.
- Register in `src/components/mod.rs` and re-export from `src/lib.rs` alongside
  `group_box`.
- No `widget.rs`.
