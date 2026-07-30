# sidebar_item hover-reveal trailing actions — design

**Issue:** [#97](https://github.com/VoidstarSolutions/void_ui/issues/97) — No hover-reveal / row-actions affordance for `sidebar_item` trailing controls.

**Status:** approved, ready for implementation plan.

**Branch:** `97-no-hover-reveal-row-actions-affordance-for-sidebar_item-trailing-controls`.

## Problem

A consumer (Citadel) wants a trailing per-row control on `sidebar_item` (e.g. a settings
gear) that is hidden until the row is hovered or focused, so idle rows stay uncluttered.
No such affordance exists: `sidebar_item` tracks hover only internally (to pick a paint
fill in `widget.rs::resolve_bg`) and has no trailing-actions slot.

## History — why the naive path is a trap

Two prior implementation passes on the now-quarantined branch `wip/97-broken-do-not-use`
both failed, **both on the same root cause**: the trailing actions were laid out by
hand-computed geometry (`actions_origin_x` pinned in `ThemedSidebarItem::layout()`), and a
view-supplied actions sub-view that was a *fill* `flex_row` reported the full row width
under `compute_size(fit(avail))`, landing on top of the label.

- Pass 1 hid via `set_stashed`; Pass 2 hid via `set_clip_path` (`RevealBox`). Both moved the
  reveal mechanism around and never fixed the layout.
- Pass 2 shipped **three passing unit tests** (`hidden_by_default_clips_to_empty`,
  `revealing_opens_the_clip`, …) while the component was **visibly broken on screen**. The
  tests asserted the mechanism toggled, not that the result was usable.

The salvageable parts of that branch — the `.action(view)` builder, the `RevealBox` widget,
and the View-lifecycle plumbing — work correctly. Only the two-child layout math is wrong.

## Decisions (locked during brainstorming)

- **Direction:** hover-reveal slot (chosen over the maintainer-preferred overflow-menu
  pivot and over surfacing hover state to the host).
- **Idle layout:** reclaim space — hidden actions collapse to zero width and the label
  reflows to full width; revealing shrinks the label to make room.
- **Scope:** `sidebar_item` only (`SidebarItem` view + `ThemedSidebarItem` widget). The
  `sidebar_nav` list form is out of scope.
- **Animation:** none. Reveal is instant.

## Architecture

### The root-cause fix: delegate arrangement to masonry `Flex`

The label + actions are arranged by a real masonry `Flex` row —
`flex_row((label.flex(1.0), reveal_box))` — the exact shape `alert/view.rs:240` uses. The
framework content-sizes the trailing `RevealBox` and flexes the label. **No code computes
child origins by hand**, so the fill-overlap failure is structurally impossible.

`ThemedSidebarItem` retains everything it owns today: accent bar, background fill by
hover/selected/pressed state, focus ring, padding, `ButtonPress` emission, and hover/press
tracking.

- **No `.action()` set** → behavior and layout are **unchanged**: a single label child, the
  existing measure/layout path, existing tests untouched. Zero regression surface.
- **`.action(view)` set** → the widget's content becomes a masonry `Flex` row it owns as a
  **typed** child, containing `[label.flex(1.0), RevealBox(actions)]`. Owning the `Flex`
  typed is what lets the widget still (a) recolor the label on selected/theme/disabled
  changes and (b) drive the reveal directly on its own hover/focus updates.

### RevealBox — collapse, not paint-clip

`RevealBox` (salvaged, repurposed) wraps the trailing actions and, given the reclaim
decision, **collapses** rather than merely clipping paint:

- `revealed == false` → reports **zero width** in measure/layout (while still reporting the
  child's natural cross-axis extent, i.e. its real height) and stashes/skips painting the
  child, freeing the horizontal space so the label flexes into it.
- `revealed == true` → natural size + paint.

Instant; no animator. This deliberately avoids the animator-re-arm cost documented in
`CLAUDE.md` ("Profiling") — a widget that re-arms `request_anim_frame` holds the whole
window at refresh rate — and there is nothing to tune in an instant toggle.

### Reveal predicate — idempotent, recomputed, never stored-and-drifted

The Pass-2 bug "rows accumulated revealed state as the cursor moved" came from a stored
`revealed` bool that drifted out of sync with reality. The fix: recompute the reveal on
every relevant `Update` as a **pure function of live widget context** and push it
idempotently (`RevealBox::set_revealed` is a no-op when unchanged):

```text
revealed = !disabled
        && (row hovered || descendant hovered || row focused || descendant focused)
```

Driven from `Update::WidgetAdded` (initial sync, in case the row is added already
hovered/focused) and `Update::{HoveredChanged, ChildHoveredChanged, FocusChanged,
ChildFocusChanged, DisabledChanged}`. No identity-based re-arm heuristic that can go stale;
the widget's own live pointer/focus state is the single source of truth. (This is the
"externally-supplied idempotent state over identity heuristics" preference applied to the
widget's own context.)

## View layer & event routing

`SidebarItem` gains a salvaged builder:

```rust
sidebar_item("AAPL", on_select)
    .action(gear_button)   // impl WidgetView<State, Action>
    .render(&theme)
```

`SidebarItemView` today owns **no** child views (the label is owned by the widget). Adding
an action child means it must adopt `ctx.with_id(...)` around the child's build/rebuild/
teardown/message and `message.take_first()` in `message()`, so the row's `ButtonPress`
(select) and the action's `ButtonPress` (its own callback) route to distinct paths rather
than colliding. This is the mandatory rule for a custom `View` that hosts child
`WidgetView`s; omitting it makes nested widgets share an action path and fire the wrong
callback.

**Click semantics:** clicking the action fires the action's callback and does **not**
select the row — the nested button consumes/handles the pointer so the row's pointer
handler never runs its select path.

**Icons:** action controls use `IconName` via `icon()` / `content_button`, never raw font
glyphs. (Pass 1's raw glyphs rendered as missing-glyph boxes; even glyphs that do render,
e.g. U+2190/2192 arrows at caption size, can be unusable.)

## Keyboard access

Reveal-on-**focus** is mandatory, not just reveal-on-hover: when the row takes roving/Tab
focus its actions appear and enter the focus order, so a keyboard user can Tab into them and
activate with Enter/Space. Disabled rows never reveal and never expose actions. The precise
focus-order interplay (row focus → actions appear → Tab reaches action) is inherently a
runtime/visual behavior and is a human-verified acceptance criterion below, not something a
unit test can vouch for.

## Demo

Extend `sidebar/demo.rs` with a short symbol-row list (`AAPL`, `MSFT`, …). Each row carries a
hover/focus-revealed gear (`IconName`, `content_button`) that toggles a per-row bit of
**demo-local** state (per the component-local-state pattern used elsewhere in the gallery),
exercising: reveal, label reflow, click-not-selecting-row, and keyboard reach. The demo is
also the surface for the acceptance loop below.

## Testing & verification

**The human-driven gallery loop is the acceptance gate — not the unit tests.** Pass 2's
three green tests are the cautionary precedent: they verified the mechanism toggled while
the component was broken on screen. Unit tests here are supporting-only and explicitly
insufficient; they may assert at most that:

- `RevealBox` reports zero size when not revealed and natural size when revealed;
- the reveal predicate is a pure function of the given hover/focus/disabled inputs;
- (existing) selected vs hovered still resolve to distinct fills (regression guard from #95).

They must **not** be treated as evidence the feature works.

**Acceptance criteria (human-verified in `cargo run --example gallery --features gallery`):**

1. Idle rows show full-width labels, **no** reserved trailing gap, and **no** ghost/leftover
   controls.
2. Hovering a row reveals the gear; the label reflows once, cleanly; moving off the row hides
   it again.
3. A **fast cursor sweep** across many rows leaves **no** row stuck in the revealed state.
4. Clicking the gear fires the gear's callback and does **not** select the row.
5. Keyboard: focusing a row reveals its gear; Tab reaches the gear; Enter/Space activates it.
6. Disabled rows never reveal a gear.

If any criterion fails, iterate one concrete fix per cycle against the live gallery — do not
declare completion on green unit tests.

## Out of scope

- `sidebar_nav` (the roving-tabindex list form).
- The overflow-menu (`⋯` → `dropdown_button`/`context_menu`) pivot.
- Surfacing raw hover state to the host as a callback.
- Any reveal animation.
