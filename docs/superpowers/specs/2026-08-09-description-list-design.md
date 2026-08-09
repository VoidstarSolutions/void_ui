# DescriptionList — label/value pairs (issue #225)

**Date:** 2026-08-09
**Branch:** `225-descriptionlist-labelvalue-pairs`
**Roadmap:** Phase 5 — Data display, size S.

## Summary

A presentation-only `<dl>` analog: an ordered set of label/value pairs. In the
default horizontal layout, values align in a shared column whose x-origin is the
width of the widest label (auto-fit). A stacked layout renders each value below
its label. Theme-driven spacing and type; no domain logic, no state ownership.

## Decisions (locked during brainstorming)

- **Value content model:** value is an arbitrary child view
  (`AnyWidgetView<State, Action>`) — a string, `badge`, `status_dot`, link, etc.
  Label is a plain `String`, rendered as themed muted text via an internal
  `Label` child so the widget stays pure layout.
- **Layouts:** both horizontal (default) and stacked, selected by a
  `DescriptionListOrientation` enum in the builder (mirrors `Separator`'s
  orientation switch; named distinctly because `separator::Orientation` and
  `FormOrientation` already occupy the crate root — follow the `FormOrientation`
  precedent).
- **Horizontal column width:** auto-fit the widest label. The DescriptionList
  widget owns cross-row layout; this is *not* reusable from `form`, which uses a
  fixed `label_width(Length)` column via flex composition.
- **Item identity:** positional/index. Items diff by position (like
  `Separator`'s children). No per-item keys (YAGNI for a display component).
- **Horizontal vertical alignment:** first-baseline align — label's first text
  baseline aligns to the value's first baseline. Fallback: when a value reports
  no usable baseline (e.g. a non-text widget like `status_dot`), that row
  top-aligns instead.

## Architecture

Standard two-layer split under `src/components/description_list/`:

- `view.rs` — builder + xilem `View` impl.
- `widget.rs` — masonry `Widget` impl (owns layout + child management; no paint
  beyond children).
- `demo.rs` — gallery panel (gated behind `gallery` feature).
- `mod.rs` — re-exports.

### view.rs — builder API

```rust
description_list()
    .item("Name",   label("Ada Lovelace").render(&theme))
    .item("Status", status_dot(Status::Online).render(&theme))
    .item("Role",   badge("Admin").render(&theme))
    .stacked()                       // or .horizontal() (default)
    .render::<State, Action>(&theme)
```

- `description_list() -> DescriptionList` — constructor, default
  `DescriptionListOrientation::Horizontal`.
- `.item(label: impl Into<String>, value: impl Into<AnyWidgetView<State, Action>>)`
  — repeatable; pushes a `(String, AnyWidgetView)` pair. (Value generic over
  `State`/`Action` — the builder is generic like other value-bearing views in
  this crate; match the existing idiom found in `tooltip`/`popover` content
  slots.)
- `.horizontal()` / `.stacked()` — set `DescriptionListOrientation`.
- `.render::<State, Action>(&theme) -> impl WidgetView` — materializes:
  builds one internal muted `Label` child per label string and passes the value
  child views straight through, alongside the resolved `Orientation` and theme
  metrics (column gap, row gap), into `DescriptionListWidget`.

`#[must_use]` on the builder (consistent with `Separator`).

**Child action routing:** each item's value view is mounted under
`ctx.with_id` with a per-row id and messages routed via `take_first`, per the
crate's `with_id` routing rule — values are interactive views (links, buttons),
and without this their action paths would collide.

### widget.rs — `DescriptionListWidget`

A custom multi-child container. Children are stored as a flat, ordered list of
pods: for each item, a label pod followed by a value pod. `register_children`
registers both; `children_ids` returns them in order.

**Horizontal `layout`:**
1. Measure pass — lay out each label pod under an unbounded/loose width
   constraint, record its desired width. `col_w = max(label widths)`.
2. Place pass — for each row: lay out the label at `x = 0` within `col_w`; lay
   out the value at `x = col_w + column_gap` within the remaining width. Row
   height = `max(label_h, value_h)`. Advance y by row height + `row_gap`.
3. Vertical align within a row — align label and value by first baseline using
   masonry's child baseline offsets; if the value reports no usable baseline,
   top-align that row.

**Stacked `layout`:** each item lays out its label at full width, then its value
below at full width (small intra-pair gap), then a larger inter-item gap before
the next item. No shared column, no measure pass.

**Paint:** none beyond children — labels and values paint themselves. No custom
drawing ⇒ no theme-swap redraw concern, no animator.

Theme is copied in at build time and re-applied on rebuild only when changed
(crate convention). Column gap / row gap / intra-pair gap derive from
`Theme::density` (`gap`, `gap_lg`, `pad`).

### Rebuild / diffing

View `rebuild` walks items positionally: relabel changed label strings, rebuild
value children in place, and add/remove trailing pods when the item count
changes. `DescriptionListOrientation` change is a plain widget field mutation (`mutate_later`
or direct in `rebuild`) that requests a relayout.

## Testing

- **View rebuild test** — add, remove, and change items across a rebuild;
  assert child count and label text track the builder.
- **Layout test (horizontal)** — with labels of differing widths, assert every
  value pod shares a common x-origin equal to `widest_label + column_gap`.
- **Layout test (stacked)** — assert each value's y is below its label's.
- **Gallery demo** — `with_source!` panel exercising both orientations and at
  least one rich value (`status_dot` or `badge`) alongside plain text values.

## Out of scope (YAGNI)

- Per-item keys / keyed diffing.
- Fixed `label_width` override (auto-fit only; revisit if a consumer needs it).
- Dividers between rows, hover/selection, dense/striped variants.
- Center vertical alignment mode.

## Wiring checklist

- Register module in `src/components/mod.rs`; re-export `description_list`,
  `DescriptionList`, `DescriptionListOrientation` at crate root.
- Flip README Phase 5 `DescriptionList` status `—` → `✓`.
