# Follow-up: ranged-brush support in masonry `TextArea`

## Background

`void-ui`'s `CodeViewWidget` (added 2026-05-26) drives parley's
`Layout<BrushIndex>` directly because `masonry::widgets::TextArea` paints
with a single hard-coded brush. Once an upstream change exposes per-range
brushes on `TextArea`, the editable text field path can be a thin wrapper
instead of duplicating selection/keyboard/IME plumbing on top of a
hand-rolled parley driver.

## Why we didn't do it inline

At the time void-ui shipped its text field, masonry's `TextArea`:

1. `TextArea::insert_style_inner` debug-panics on any `BrushIndex(1..)` —
   only the single global brush is allowed
   (`masonry/src/widgets/text_area.rs:219`).
2. `TextArea::paint` calls `render_text(..., &[text_color.color.into()],
   ...)` — a one-element brush palette, hard-coded
   (`masonry/src/widgets/text_area.rs:1028`).
3. The inner `editor: PlainEditor<BrushIndex>` is private; there is no
   public adapter to reach it.

parley's `PlainEditor::update_layout` also only pushes the global default
style (`parley/src/editing/editor.rs:1229`) — its `RangedBuilder` use is
private — so wrapping `PlainEditor` directly doesn't help either.

## Proposed upstream change

1. `TextArea::insert_style_inner`: stop debug-panicking on
   `BrushIndex(1..)`. Allow indices that fit within the brush palette.
2. Add `TextArea::with_brushes(Vec<Color>)` and a `set_brushes` setter
   that replaces the hard-coded `&[text_color.color.into()]` palette in
   `paint` with the user-supplied palette (index 0 still defaults to
   `ContentColor` if the palette is empty, preserving today's behavior).
3. Document that consumers building rich text should push `BrushIndex(n)`
   styles via `insert_style` and supply matching brushes via
   `with_brushes`.

## When to act

When void-ui needs an editable rich-text field — likely when the host app
needs a code editor or rich-text comment box. Until then the duplication
is contained to `text_field/widget.rs` and the maintenance cost is low.

A second motivator: if a downstream `void-ui` consumer asks for
syntax-highlighted editable code, the cleanest answer is "upstream the
masonry change, then wrap `TextArea`" rather than "we ship two parallel
text-handling stacks".
