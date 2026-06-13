# Collection Substrate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract `data_grid`'s row-virtualization machinery into a crate-internal `collection` substrate and migrate `data_grid` onto it, so a later branch can rebuild `list` on the same substrate instead of duplicating it.

**Architecture:** A new `pub(crate) mod collection` owns the shared types (`SelectionState`, `ScrollState`), the id/index helpers (`IdSource`, `visual_range_ids`, the integer-domain converters), the central selection-click application, and a unified body (xilem `View` + masonry `Widget`) that virtualizes rows, applies scroll-to-anchor + lazy-load, handles row clicks centrally, and supports Up/Down keyboard navigation. `data_grid` keeps its columns/header/sort/filter/horizontal-scroll/TSV-copy and supplies a per-row cell strip through a closure seam. The substrate is carved *from* `data_grid`'s proven code, with its existing test suite as the migration guardrail.

**Tech Stack:** Rust, xilem (linebender `main`), masonry, `xilem_core::memoize`. Workspace denies `clippy::pedantic`. Spec: `docs/superpowers/specs/2026-06-13-collection-substrate-design.md`.

---

## File Structure

**Created:**
- `src/collection/mod.rs` — `pub(crate)` module root; re-exports the public types and crate-internal items.
- `src/collection/selection.rs` — `SelectionState` (moved verbatim from `data_grid/selection.rs`).
- `src/collection/scroll.rs` — `ScrollState` + `clamp_scroll_index` (moved from `data_grid/scroll.rs`).
- `src/collection/ids.rs` — `IdSource` (renamed from `RowIdSource`), `visual_range_ids`, `scroll_range_end`, `scroll_idx_to_slice`, `position_fallback_id`.
- `src/collection/click.rs` — `apply_row_click` (central selection-click application).
- `src/collection/row_click.rs` — `RowClickable` + `clickable_row` (moved from `data_grid`; shared row-click primitive).
- `src/collection/single_child.rs` — single-child passthrough helpers (moved from `data_grid`; lowest shared primitive).
- `src/collection/body.rs` — `CollectionBodyWidget` (masonry) + `collection_body` (xilem `View`).
- `src/collection/bench.rs` (or `benches/row_build.rs`) — per-row build measurement (Task 8).

**Modified:**
- `src/lib.rs` — add `mod collection;`; keep `SelectionState`/`ScrollState` re-exports pointing at it.
- `src/components/data_grid/mod.rs` — re-export `SelectionState`/`ScrollState` from `crate::collection`.
- `src/components/data_grid/view.rs` — delete the moved helpers; rewrite `build_body_view` to call `collection_body`.
- `src/components/data_grid/scroll.rs` — delete `ScrollState`/`clamp_scroll_index`/`ScrollToView` (superseded); file removed.
- `src/components/data_grid/selection.rs` — file removed (moved).
- `src/components/data_grid/{header_click,copy_shortcut,column_strip}.rs` — update `use super::single_child` → `use crate::collection::single_child`.

**Layering note:** `RowClickable`/`clickable_row` and `single_child` move *down* into the substrate (Task 4) so the substrate never imports from `data_grid`. `data_grid`'s remaining widgets (`header_click`, `copy_shortcut`, `column_strip`) import `single_child` from `collection` — correct direction (component depends on substrate, never the reverse).

**Naming locked across tasks** (the self-review checks these): module `collection`; type `IdSource` (variants `Explicit`, `Position`; method `id_of(&self, pos: usize, item: &Item) -> u64`); `collection_body(...)`; `CollectionBodyWidget`; `apply_row_click(...)`.

---

## Task 1: Create the `collection` module and move `SelectionState`

**Files:**
- Create: `src/collection/mod.rs`
- Create: `src/collection/selection.rs`
- Modify: `src/lib.rs`
- Modify: `src/components/data_grid/mod.rs`
- Remove: `src/components/data_grid/selection.rs`

- [ ] **Step 1: Create `src/collection/selection.rs`**

Move the entire contents of `src/components/data_grid/selection.rs` (all 195 lines, including the `#[cfg(test)] mod tests`) into the new file verbatim. The doc-comment links `super::data_grid` / `super::view::DataGrid::row_id` will break; update the two intra-doc links in the module doc to plain text (e.g. ``[`DataGrid::row_id`]`` → `the host's row-id projector`) so rustdoc stays warning-free. No logic changes.

- [ ] **Step 2: Create `src/collection/mod.rs`**

```rust
//! Crate-internal substrate shared by the virtualized collection
//! components (`data_grid`, and later `list`).
//!
//! Owns the row-virtualization machinery both components need: the
//! selection model, programmatic scroll requests, stable-id keying, the
//! shift/toggle/replace click application, and the unified virtualized
//! body widget. Components supply per-row *content* through a closure;
//! the substrate owns everything vertical (virtualization, scroll-to,
//! lazy-load, keyboard nav, click routing).
//!
//! Not public: the only consumers are in-crate. `SelectionState` and
//! `ScrollState` are surfaced to consumers by re-export from the
//! components and the crate root.

mod selection;

pub(crate) use selection::SelectionState;
```

- [ ] **Step 3: Wire the module into `src/lib.rs`**

Add `mod collection;` alongside the other top-level `mod` declarations (it is `pub(crate)`, so a plain `mod` is correct). In the existing `pub use components::{... SelectionState ...}` re-export, nothing changes yet — `data_grid` will continue to re-export `SelectionState` (next step), so the crate-root path stays valid.

- [ ] **Step 4: Re-export from `data_grid`, delete the old file**

In `src/components/data_grid/mod.rs`, replace the `mod selection;` + `pub use selection::SelectionState;` (or equivalent) with:

```rust
pub use crate::collection::SelectionState;
```

Delete `src/components/data_grid/selection.rs`. Update any `use super::selection::SelectionState;` inside `data_grid/view.rs` / `scroll.rs` to `use crate::collection::SelectionState;`.

- [ ] **Step 5: Build and run the moved tests**

Run: `cargo test --lib selection`
Expected: PASS — the six `SelectionState` tests run from their new location.

Run: `cargo clippy --all-targets --all-features`
Expected: no warnings (no dead code: `SelectionState` is used by `data_grid` via the re-export).

- [ ] **Step 6: Commit**

```bash
git add src/collection src/lib.rs src/components/data_grid/mod.rs src/components/data_grid/view.rs
git rm src/components/data_grid/selection.rs
git commit -m "collection: move SelectionState into shared substrate"
```

---

## Task 2: Move `ScrollState` + `clamp_scroll_index`

**Files:**
- Create: `src/collection/scroll.rs`
- Modify: `src/collection/mod.rs`
- Modify: `src/components/data_grid/scroll.rs`, `src/components/data_grid/mod.rs`

- [ ] **Step 1: Create `src/collection/scroll.rs`**

Move `ScrollState` (its full impl + the `#[derive]`), the `pub(crate) fn generation`/`index` accessors, and `clamp_scroll_index` from `data_grid/scroll.rs` into `src/collection/scroll.rs`. Keep the `#[cfg(test)] mod tests` block that covers `ScrollState`/`clamp_scroll_index` (the `default_state_has_no_pending_request`, `scroll_to_index_bumps_generation_and_stores_index`, `same_index_retriggers_via_new_generation`, and the three `clamp_*` tests). Make `clamp_scroll_index` `pub(crate)` (it was `pub(super)`). `ScrollState` stays `pub`.

- [ ] **Step 2: Export from `collection/mod.rs`**

```rust
mod scroll;
mod selection;

pub(crate) use scroll::{ScrollState, clamp_scroll_index};
pub(crate) use selection::SelectionState;
```

(Re-exporting a `pub` type with `pub(crate) use` is fine — the type's own visibility governs the public surface via the component/crate-root re-exports.)

- [ ] **Step 3: Leave `ScrollToView` in `data_grid/scroll.rs` for now, point it at the moved code**

`data_grid/scroll.rs` still defines `ScrollToView`/`ScrollToViewState` (deleted later in Task 7). Replace its local `ScrollState`/`clamp_scroll_index` definitions with `use crate::collection::{ScrollState, clamp_scroll_index};`. Update the field accesses `self.scroll.generation` / `self.scroll.index` to the accessor methods `self.scroll.generation()` / `self.scroll.index()` (they are now in a different module, so the private fields aren't reachable — the `pub(crate)` accessors are).

- [ ] **Step 4: Re-export from `data_grid`**

In `src/components/data_grid/mod.rs`, change the `ScrollState` re-export to `pub use crate::collection::ScrollState;`.

- [ ] **Step 5: Build, test, lint**

Run: `cargo test --lib scroll`
Expected: PASS — `ScrollState`/`clamp` tests run from `collection`.

Run: `cargo test --all-features`
Expected: PASS — `data_grid`'s scroll-to integration tests still pass through `ScrollToView`.

Run: `cargo clippy --all-targets --all-features`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add src/collection src/components/data_grid
git commit -m "collection: move ScrollState + clamp_scroll_index into substrate"
```

---

## Task 3: Move `IdSource` + `visual_range_ids` + integer-domain helpers

**Files:**
- Create: `src/collection/ids.rs`
- Modify: `src/collection/mod.rs`, `src/components/data_grid/view.rs`

- [ ] **Step 1: Create `src/collection/ids.rs` with the renamed id source**

Move `RowIdSource` from `data_grid/view.rs` (~lines 73–100) into `ids.rs`, **renamed to `IdSource`** with generic param `Item` (was `R`). Keep the hand-written `Clone` impl (bound on the `Arc`, not on `Item`) and `id_of(&self, pos: usize, item: &Item) -> u64`. Also move `position_fallback_id`, `scroll_range_end`, `scroll_idx_to_slice` (the integer-domain converters, ~lines 699–711) and `visual_range_ids` (~lines 1115–1135, generic over `IdSource<Item>`). Move the `#[cfg(test)] mod tests` covering `visual_range_ids` (the four/five cases at the bottom of `view.rs`) into `ids.rs`, updating `RowIdSource` → `IdSource` in the test bodies.

```rust
//! Stable-id keying and the integer-domain conversions shared by the
//! virtualized collection components.

use std::sync::Arc;

/// How a collection derives a row's stable id: the host's projector, or a
/// fallback to the item's current slice position.
pub(crate) enum IdSource<Item> {
    /// Host-supplied id projector.
    Explicit(Arc<dyn Fn(&Item) -> u64 + Send + Sync>),
    /// No projector: use the item's current slice position as its id.
    Position,
}

impl<Item> Clone for IdSource<Item> {
    fn clone(&self) -> Self {
        match self {
            Self::Explicit(f) => Self::Explicit(Arc::clone(f)),
            Self::Position => Self::Position,
        }
    }
}

impl<Item> IdSource<Item> {
    pub(crate) fn id_of(&self, pos: usize, item: &Item) -> u64 {
        match self {
            Self::Explicit(f) => f(item),
            Self::Position => position_fallback_id(pos),
        }
    }
}

/// Slice position (`usize`) → stable id (`u64`) for the position
/// fallback. Saturates to `u64::MAX`.
pub(crate) fn position_fallback_id(pos: usize) -> u64 {
    u64::try_from(pos).unwrap_or(u64::MAX)
}

/// `virtual_scroll` range bound: item count (`u64`) → `i64`. Saturates to
/// `i64::MAX`.
pub(crate) fn scroll_range_end(item_count: u64) -> i64 {
    i64::try_from(item_count).unwrap_or(i64::MAX)
}

/// `virtual_scroll` callback index (`i64`) → slice index (`usize`).
/// Saturates so a stray negative/oversized index reads as past-the-end.
pub(crate) fn scroll_idx_to_slice(idx: i64) -> usize {
    usize::try_from(idx).unwrap_or(usize::MAX)
}

/// Resolves the stable ids spanning the visual range between `anchor_id`
/// and `target_id` (inclusive) in current display order, or `None` when
/// either endpoint is absent. O(n) over the slice; called only on a
/// shift-click.
pub(crate) fn visual_range_ids<Item>(
    data: &[Item],
    id_source: &IdSource<Item>,
    anchor_id: u64,
    target_id: u64,
) -> Option<Vec<u64>> {
    let mut anchor_pos = None;
    let mut target_pos = None;
    for (pos, item) in data.iter().enumerate() {
        let id = id_source.id_of(pos, item);
        if id == anchor_id {
            anchor_pos = Some(pos);
        }
        if id == target_id {
            target_pos = Some(pos);
        }
    }
    let (a, t) = (anchor_pos?, target_pos?);
    let (lo, hi) = if a <= t { (a, t) } else { (t, a) };
    Some((lo..=hi).map(|pos| id_source.id_of(pos, &data[pos])).collect())
}
```

- [ ] **Step 2: Export from `collection/mod.rs`**

```rust
mod ids;
pub(crate) use ids::{
    IdSource, position_fallback_id, scroll_idx_to_slice, scroll_range_end, visual_range_ids,
};
```

- [ ] **Step 3: Rewire `data_grid/view.rs`**

Delete the moved items from `view.rs`. Add `use crate::collection::{IdSource, scroll_idx_to_slice, scroll_range_end, visual_range_ids};` (and `position_fallback_id` if referenced). Replace every `RowIdSource` with `IdSource` in `view.rs` (the `row_id: RowIdSource<R>` fields, the `match row_id { Some(f) => RowIdSource::Explicit(f), None => RowIdSource::Position }` construction, and the `BodyParams`/closure sites). The `warn_missing_row_id` path and `project_tsv` debug-assert stay in `data_grid` — they only *use* `id_of`, which is unchanged.

- [ ] **Step 4: Build, test, lint**

Run: `cargo test --all-features`
Expected: PASS — `visual_range_ids` tests run from `collection::ids`; all `data_grid` tests still green.

Run: `cargo clippy --all-targets --all-features`
Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/collection src/components/data_grid/view.rs
git commit -m "collection: move IdSource, visual_range_ids, index converters into substrate"
```

---

## Task 4: Shared row-click primitive + central selection-click

**Files:**
- Move: `src/components/data_grid/single_child.rs` → `src/collection/single_child.rs`
- Move: `src/components/data_grid/row_click.rs` → `src/collection/row_click.rs`
- Create: `src/collection/click.rs`
- Modify: `src/collection/mod.rs`; `src/components/data_grid/{mod,header_click,copy_shortcut,column_strip}.rs`
- Test: in `src/collection/click.rs` (`#[cfg(test)] mod tests`)

First move the shared row-click widget and the passthrough helpers *down* into the substrate (so `collection_body` in Task 6 can wrap rows without the substrate importing from `data_grid`). Then extract the shift/toggle/replace logic (currently inline in `data_grid/view.rs:1061–1095`) into one generic, unit-tested function.

- [ ] **Step 1: Move `single_child.rs` into the substrate**

`git mv src/components/data_grid/single_child.rs src/collection/single_child.rs`. Change its `pub(super)` helpers to `pub(crate)`. Add `mod single_child;` to `collection/mod.rs`. Update every `data_grid` user (`row_click.rs` — moving next — plus `header_click.rs`, `copy_shortcut.rs`, `column_strip.rs`): `use super::single_child;` → `use crate::collection::single_child;`. Build to confirm.

- [ ] **Step 2: Move `row_click.rs` into the substrate**

`git mv src/components/data_grid/row_click.rs src/collection/row_click.rs`. It keeps its current contents unchanged (the `RowClickable` widget with focus ring + accesskit `Selected` + Enter/Space activation, and the `clickable_row` view with signature `clickable_row<V, State, F>(child, selected: bool, theme: &Theme, on_click)`), except: its `use super::single_child;` becomes `use crate::collection::single_child;`, and its `use crate::components::click::{self, ClickPhase};` path stays valid (that's a crate-level module — confirm with a build). Add `mod row_click;` + `pub(crate) use row_click::{RowClickAction, RowClickable, clickable_row};` to `collection/mod.rs`. In `data_grid/mod.rs`, remove `mod row_click;`; update `data_grid/view.rs` and any tests from `use super::row_click::{...}` / `use crate::components::data_grid::row_click::{...}` to `use crate::collection::row_click::{...}`.

- [ ] **Step 3: Build the moves before adding new code**

Run: `cargo test --all-features`
Expected: PASS — pure relocation, `data_grid`'s row-click and copy/header widgets still green.

Run: `cargo clippy --all-targets --all-features`
Expected: clean.

- [ ] **Step 4: Define the items/lens type aliases and write the failing test**

Create `src/collection/click.rs`:

```rust
//! Central selection-click application: maps a row click's modifiers to
//! the matching `SelectionState` operation, keyed by stable row id.

use std::sync::Arc;

use crate::collection::{IdSource, SelectionState, visual_range_ids};

/// Item-data accessor (`Fn(&State) -> &[Item]`).
pub(crate) type ItemsFn<State, Item> =
    Arc<dyn for<'a> Fn(&'a State) -> &'a [Item] + Send + Sync>;
/// Selection lens (`Fn(&mut State) -> &mut SelectionState`).
pub(crate) type SelectionLens<State> =
    Arc<dyn for<'a> Fn(&'a mut State) -> &'a mut SelectionState + Send + Sync>;

/// A row click's resolved modifiers (mirrors `data_grid`'s `RowClickAction`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RowClick {
    pub(crate) shift: bool,
    pub(crate) action_mod: bool,
}

/// Applies a row click at slice position `pos` to the host's
/// `SelectionState`: shift extends the visual range from the anchor, the
/// action modifier toggles membership, a plain click replaces.
pub(crate) fn apply_row_click<State, Item>(
    state: &mut State,
    click: RowClick,
    pos: usize,
    items: &ItemsFn<State, Item>,
    selection_lens: Option<&SelectionLens<State>>,
    id_source: &IdSource<Item>,
) {
    let Some(sel_lens) = selection_lens else {
        return;
    };
    let Some(target_id) = ({
        let data = (*items)(state);
        data.get(pos).map(|item| id_source.id_of(pos, item))
    }) else {
        return;
    };

    if click.shift {
        let anchor = (**sel_lens)(state).anchor();
        let range = anchor.and_then(|anchor_id| {
            let data = (*items)(state);
            visual_range_ids(data, id_source, anchor_id, target_id)
        });
        match range {
            Some(ids) => (**sel_lens)(state).extend_range(ids),
            None => (**sel_lens)(state).replace_with(target_id),
        }
    } else if click.action_mod {
        (**sel_lens)(state).toggle(target_id);
    } else {
        (**sel_lens)(state).replace_with(target_id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ItemsFn, RowClick, SelectionLens, apply_row_click};
    use crate::collection::{IdSource, SelectionState};

    struct S {
        items: Vec<u64>,
        sel: SelectionState,
    }

    fn fixtures() -> (ItemsFn<S, u64>, SelectionLens<S>, IdSource<u64>) {
        let items: ItemsFn<S, u64> = Arc::new(|s: &S| &s.items[..]);
        let lens: SelectionLens<S> = Arc::new(|s: &mut S| &mut s.sel);
        let id_source = IdSource::Explicit(Arc::new(|item: &u64| *item));
        (items, lens, id_source)
    }

    #[test]
    fn plain_click_replaces_selection_with_target_id() {
        let mut s = S { items: vec![10, 20, 30], sel: SelectionState::new() };
        let (items, lens, id_source) = fixtures();
        apply_row_click(
            &mut s,
            RowClick { shift: false, action_mod: false },
            1,
            &items,
            Some(&lens),
            &id_source,
        );
        assert!(s.sel.contains(20));
        assert_eq!(s.sel.len(), 1);
        assert_eq!(s.sel.anchor(), Some(20));
    }

    #[test]
    fn action_mod_toggles_membership() {
        let mut s = S { items: vec![10, 20, 30], sel: SelectionState::new() };
        let (items, lens, id_source) = fixtures();
        let click = RowClick { shift: false, action_mod: true };
        apply_row_click(&mut s, click, 0, &items, Some(&lens), &id_source);
        apply_row_click(&mut s, click, 0, &items, Some(&lens), &id_source);
        assert!(!s.sel.contains(10));
    }

    #[test]
    fn shift_extends_visual_range_from_anchor() {
        let mut s = S { items: vec![10, 20, 30, 40], sel: SelectionState::new() };
        let (items, lens, id_source) = fixtures();
        apply_row_click(
            &mut s,
            RowClick { shift: false, action_mod: false },
            0,
            &items,
            Some(&lens),
            &id_source,
        );
        apply_row_click(
            &mut s,
            RowClick { shift: true, action_mod: false },
            2,
            &items,
            Some(&lens),
            &id_source,
        );
        assert_eq!(s.sel.iter().collect::<Vec<_>>(), vec![10, 20, 30]);
        assert_eq!(s.sel.anchor(), Some(10));
    }

    #[test]
    fn no_lens_is_a_noop() {
        let mut s = S { items: vec![10], sel: SelectionState::new() };
        let (items, _lens, id_source) = fixtures();
        apply_row_click(
            &mut s,
            RowClick { shift: false, action_mod: false },
            0,
            &items,
            None,
            &id_source,
        );
        assert!(s.sel.is_empty());
    }
}
```

- [ ] **Step 5: Export from `collection/mod.rs`**

```rust
mod click;
pub(crate) use click::{ItemsFn, RowClick, SelectionLens, apply_row_click};
```

- [ ] **Step 6: Run the tests**

Run: `cargo test --lib collection::click`
Expected: PASS (all four).

- [ ] **Step 7: Lint and commit**

Run: `cargo clippy --all-targets --all-features`
Expected: clean.

```bash
git add src/collection src/components/data_grid
git commit -m "collection: move row-click + single-child primitives into substrate, add central selection-click"
```

---

## Task 5: `CollectionBodyWidget` — virtualized body with keyboard nav

**Files:**
- Modify: `src/collection/body.rs` (create)
- Modify: `src/collection/mod.rs`
- Test: `#[cfg(test)] mod tests` in `body.rs`

This promotes `list`'s `ListBodyWidget` (arrow-key Up/Down nav over the materialized `VirtualScroll` rows) into the substrate. New behavior for the substrate, so test-first. The widget body and its tests can be lifted from `src/components/list/body.rs` (lines 103–218 for the widget, 340–490 for the tests) — but `list` is not on this branch, so the code below is the authoritative copy.

- [ ] **Step 1: Create `src/collection/body.rs` with the widget + failing tests**

Write the masonry widget exactly as below (it is a single-child passthrough around `VirtualScroll` that intercepts Up/Down to move focus between adjacent materialized rows). Include the test module from `list/body.rs` adapted to construct rows with `data_grid`'s `RowClickable` — but to avoid a dependency on `data_grid`'s widget here, the tests wrap plain `Label` rows.

```rust
//! Unified virtualized body: a masonry widget that adds Up/Down row
//! navigation over `VirtualScroll`, plus the xilem `View`
//! (`collection_body`) that drives scroll-to-anchor, lazy-load, and
//! central click routing.

use masonry::accesskit::Role;
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LayoutSize, LenReq, Length, SizeDef};
use xilem::masonry::widgets::VirtualScroll as VirtualScrollWidget;

/// Single-child wrapper around masonry's `VirtualScroll` adding Up/Down
/// arrow-key navigation between materialized rows.
pub(crate) struct CollectionBodyWidget {
    child: WidgetPod<VirtualScrollWidget>,
}

impl CollectionBodyWidget {
    pub(crate) fn new(child: NewWidget<VirtualScrollWidget>) -> Self {
        Self { child: child.to_pod() }
    }

    pub(crate) fn virtual_scroll_mut<'t>(
        this: &'t mut WidgetMut<'_, Self>,
    ) -> WidgetMut<'t, VirtualScrollWidget> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl Widget for CollectionBodyWidget {
    type Action = NoAction;

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        let delta: isize = match (key.state, &key.key) {
            (KeyState::Down, Key::Named(NamedKey::ArrowDown)) => 1,
            (KeyState::Down, Key::Named(NamedKey::ArrowUp)) => -1,
            _ => return,
        };
        let Some(focused) = ctx.focus_target_id() else {
            return;
        };
        let (virtual_scroll, _) = ctx.get_raw(&mut self.child);
        let row_ids = virtual_scroll.children_ids();
        let Some(pos) = row_ids.iter().position(|&id| id == focused) else {
            return;
        };
        let Some(&target) = pos.checked_add_signed(delta).and_then(|i| row_ids.get(i)) else {
            return;
        };
        ctx.set_focus(target);
        ctx.set_handled();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let auto_length = len_req.into();
        ctx.compute_length(
            &mut self.child,
            auto_length,
            LayoutSize::maybe(axis.cross(), cross_length),
            axis,
            cross_length,
        )
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let child_size = ctx.compute_size(&mut self.child, SizeDef::fixed(size), size.into());
        ctx.run_layout(&mut self.child, child_size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _p: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut masonry::accesskit::Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
```

Append the test module — port verbatim from `src/components/list/body.rs:340–490`, replacing `ListBodyWidget` with `CollectionBodyWidget` and `RowClickable::new(... , false, &Theme::default())` with `NewWidget::new(Label::new(format!("row {idx}"))).erased()` (drop the `RowClickable`/`Theme` imports). The five tests are: `arrow_down_moves_focus_to_next_row`, `arrow_up_moves_focus_to_previous_row`, `arrow_up_at_first_row_is_a_no_op`, `arrow_down_at_last_materialized_row_is_a_no_op`, `arrow_keys_on_non_row_focus_are_unhandled`.

- [ ] **Step 2: Export from `collection/mod.rs`**

```rust
mod body;
pub(crate) use body::CollectionBodyWidget;
```

- [ ] **Step 3: Run tests — they should pass (widget + ported tests land together)**

Run: `cargo test --lib collection::body`
Expected: PASS (all five arrow-key tests).

> Note: this task lands widget and tests together because the tests exercise a masonry widget that cannot be stubbed meaningfully before it exists. The "fail first" guarantee is provided by running the suite before Step 1's code is in place during development (the module won't compile), then confirming green after.

- [ ] **Step 4: Lint and commit**

Run: `cargo clippy --all-targets --all-features`
Expected: clean (the widget is used by its tests; `collection_body` in Task 6 makes it non-dead in the main build — until then, gate with `#[cfg_attr(not(test), expect(dead_code))]` on `CollectionBodyWidget` and remove that attribute in Task 6).

```bash
git add src/collection
git commit -m "collection: add CollectionBodyWidget with arrow-key row navigation"
```

---

## Task 6: `collection_body` — the body `View` (scroll-to + lazy-load + central click)

**Files:**
- Modify: `src/collection/body.rs`, `src/collection/mod.rs`
- Test: `#[cfg(test)] mod view_tests` in `body.rs`

This is the seam. It composes `ScrollToView`'s generation logic + `ListBodyView`'s lazy-load peek into one `View` whose element is `Pod<CollectionBodyWidget>`, wraps each row in `data_grid`'s `clickable_row`, computes `is_selected`, applies the selection background, and routes clicks through `apply_row_click`.

- [ ] **Step 1: Add the builder, params, and `View` impl**

Append to `body.rs`. The render seam is `Fn(&Item, bool, &Theme) -> Box<AnyWidgetView<State>>` (content only); the substrate owns selection background + click wrapping.

```rust
use std::sync::Arc;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::widgets::VirtualScrollAction;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{sized_box, virtual_scroll};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::Theme;
use crate::collection::{
    IdSource, ItemsFn, RowClick, ScrollState, SelectionLens, apply_row_click, clamp_scroll_index,
    scroll_idx_to_slice, scroll_range_end,
};
use crate::collection::row_click::{RowClickAction, clickable_row};

/// Per-item content renderer: `(item, selected, theme) -> content view`.
pub(crate) type RenderRow<State, Item> =
    Arc<dyn Fn(&Item, bool, &Theme) -> Box<AnyWidgetView<State>> + Send + Sync>;
/// Lazy-load config: fire `callback` when the active range comes within
/// `threshold` items of the end.
pub(crate) struct Lazy<State> {
    pub(crate) threshold: u64,
    pub(crate) callback: Arc<dyn Fn(&mut State) + Send + Sync>,
}

/// All inputs needed to materialize the virtualized body.
pub(crate) struct CollectionBodyParams<State, Item> {
    pub(crate) item_count: u64,
    pub(crate) items: ItemsFn<State, Item>,
    pub(crate) id_source: IdSource<Item>,
    pub(crate) selection_lens: Option<SelectionLens<State>>,
    pub(crate) scroll: ScrollState,
    pub(crate) lazy: Option<Lazy<State>>,
    pub(crate) render_row: RenderRow<State, Item>,
    pub(crate) theme: Theme,
}

/// Builds the virtualized body view. The substrate owns virtualization,
/// scroll-to-anchor, lazy-load, keyboard nav, selection background, and
/// click routing; the caller supplies only per-row content via
/// `render_row`.
pub(crate) fn collection_body<State, Item>(
    params: CollectionBodyParams<State, Item>,
) -> impl WidgetView<State, ()> + use<State, Item>
where
    State: 'static,
    Item: 'static,
{
    let CollectionBodyParams {
        item_count,
        items,
        id_source,
        selection_lens,
        scroll,
        lazy,
        render_row,
        theme,
    } = params;
    let valid_range_end = scroll_range_end(item_count);

    let child = virtual_scroll(0..valid_range_end, {
        let items = Arc::clone(&items);
        let id_source = id_source.clone();
        let selection_lens = selection_lens.clone();
        let render_row = Arc::clone(&render_row);
        move |state: &mut State, idx: i64| {
            let pos = scroll_idx_to_slice(idx);

            let data = (*items)(state);
            let id_at_pos = data.get(pos).map(|item| id_source.id_of(pos, item));
            let is_selected = match (selection_lens.as_ref(), id_at_pos) {
                (Some(sel), Some(id)) => (**sel)(state).contains(id),
                _ => false,
            };

            // Re-borrow: `is_selected` took `&mut State` via the lens.
            let data = (*items)(state);
            let content: Box<AnyWidgetView<State>> = match data.get(pos) {
                Some(item) => render_row(item, is_selected, &theme),
                None => Box::new(sized_box(xilem::view::label(""))),
            };

            let row_bg = if is_selected {
                theme.palette.surface_2
            } else {
                Color::TRANSPARENT
            };
            let row_view = sized_box(content).background_color(row_bg);

            let items = Arc::clone(&items);
            let id_source = id_source.clone();
            let selection_lens = selection_lens.clone();
            // NOTE: on this base branch `clickable_row` is the 2-arg form
            // `(child, on_click)` — the `selected`/`theme`/focus-ring
            // enhancements live on the list branch and are reconciled into
            // `collection/row_click.rs` when `list` rebuilds on the substrate.
            // The substrate applies the selection background itself (above),
            // matching data_grid's current behavior.
            clickable_row(
                row_view,
                move |state: &mut State, action: RowClickAction| {
                    apply_row_click(
                        state,
                        RowClick { shift: action.shift, action_mod: action.action_mod },
                        pos,
                        &items,
                        selection_lens.as_ref(),
                        &id_source,
                    );
                },
            )
        }
    });

    CollectionBodyView { child, scroll, item_count, lazy }
}

struct CollectionBodyView<V, State> {
    child: V,
    scroll: ScrollState,
    item_count: u64,
    lazy: Option<Lazy<State>>,
}

struct CollectionBodyViewState<S> {
    child_state: S,
    applied_generation: u64,
}

impl<V, State> ViewMarker for CollectionBodyView<V, State> {}

impl<State, V> View<State, (), ViewCtx> for CollectionBodyView<V, State>
where
    State: 'static,
    V: View<State, (), ViewCtx, Element = Pod<VirtualScrollWidget>>,
{
    type Element = Pod<CollectionBodyWidget>;
    type ViewState = CollectionBodyViewState<V::ViewState>;

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child_element, child_state) = self.child.build(ctx, app_state);
        (
            Pod::new(CollectionBodyWidget::new(child_element.new_widget)),
            CollectionBodyViewState { child_state, applied_generation: 0 },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if self.scroll.generation() != view_state.applied_generation {
            view_state.applied_generation = self.scroll.generation();
            if let Some(idx) = clamp_scroll_index(self.scroll.index(), self.item_count) {
                let mut vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
                VirtualScrollWidget::overwrite_anchor(&mut vs, idx);
            }
        }
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        self.child.rebuild(&prev.child, &mut view_state.child_state, ctx, vs, app_state);
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        self.child.teardown(&mut view_state.child_state, ctx, vs);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        if message.remaining_path().is_empty()
            && let Some(lazy) = self.lazy.as_ref()
        {
            let end = scroll_range_end(self.item_count);
            let threshold = i64::try_from(lazy.threshold).unwrap_or(i64::MAX);
            message.maybe_take_message::<VirtualScrollAction>(|action| {
                if end - action.target.end <= threshold {
                    (lazy.callback)(app_state);
                }
                false
            });
        }
        let vs = CollectionBodyWidget::virtual_scroll_mut(&mut element);
        self.child.message(&mut view_state.child_state, message, vs, app_state)
    }
}
```

Remove the `#[cfg_attr(not(test), expect(dead_code))]` added to `CollectionBodyWidget` in Task 5. Export the new API from `collection/mod.rs`:

```rust
pub(crate) use body::{CollectionBodyParams, CollectionBodyWidget, Lazy, RenderRow, collection_body};
```

- [ ] **Step 2: Add a scroll-to-generation view test**

In a `#[cfg(test)] mod view_tests` in `body.rs`, add a test that builds a `collection_body` over 100 items in a `TestHarness`, drives it to fixpoint, issues a `ScrollState::scroll_to_index(50)` snapshot via rebuild, and asserts the materialized range now includes index 50. Model the harness driver on `CollectionBodyWidget`'s test `drive_to_fixpoint`. (If a full harness test proves heavy, assert the narrower invariant: after a generation bump, `rebuild` calls `overwrite_anchor` with the clamped index — verify via a `VirtualScroll` anchor query helper.)

- [ ] **Step 3: Build, test, lint**

Run: `cargo test --lib collection`
Expected: PASS.

Run: `cargo clippy --all-targets --all-features`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/collection
git commit -m "collection: add collection_body view (scroll-to + lazy-load + central click)"
```

---

## Task 7: Migrate `data_grid` onto `collection_body`

**Files:**
- Modify: `src/components/data_grid/view.rs` (rewrite `build_body_view`)
- Remove: `src/components/data_grid/scroll.rs` (`ScrollToView` superseded)
- Modify: `src/components/data_grid/mod.rs`, `src/components/data_grid/demo.rs` (if it references `ScrollToView`)
- Test: existing `data_grid` suite + one new grid arrow-nav test

- [ ] **Step 1: Rewrite `build_body_view` to delegate to `collection_body`**

Replace the body of `build_body_view` (`view.rs` ~990–1097) so it builds the shared `widths: Arc<Vec<f64>>` once, then constructs `CollectionBodyParams` with `render_row` producing the cell strip. The selection-background and click logic are deleted here (the substrate owns them). `render_row` receives `(row, _selected, theme)` — cells ignore `selected`:

```rust
let widths: Arc<Vec<f64>> = Arc::new(render_slots.iter().map(|s| s.width).collect());
let render_slots = Arc::new(render_slots);
let render_row: RenderRow<State, R> = {
    let render_slots = Arc::clone(&render_slots);
    let widths = Arc::clone(&widths);
    Arc::new(move |row: &R, _selected: bool, theme: &Theme| -> Box<AnyWidgetView<State>> {
        let cells: Vec<Box<AnyWidgetView<State>>> = render_slots
            .iter()
            .map(|slot| aligned_cell((slot.render)(row, theme), slot.width, slot.align))
            .collect();
        Box::new(column_strip(Arc::clone(&widths), row_height, cells))
    })
};
collection_body(CollectionBodyParams {
    item_count: row_count,
    items: rows,
    id_source: row_id,
    selection_lens,
    scroll,
    lazy: None,
    render_row,
    theme,
})
```

Wire `scroll` into `build_body_view` (it currently lives on `ScrollToView` one level up — thread the `ScrollState` snapshot down into the params and drop the `ScrollToView` wrapper at the call site). Add `use crate::collection::{collection_body, CollectionBodyParams, RenderRow};`. Note `column_strip` already wraps the strip; the substrate adds its own `sized_box(...).background_color(...)` around it — confirm the double `sized_box` is acceptable (it is: background on the outer, geometry on the strip).

- [ ] **Step 2: Delete `ScrollToView` and its file**

`data_grid/scroll.rs` now only held `ScrollToView`/`ScrollToViewState` (the `ScrollState`/`clamp` moved in Task 2). Delete the file; remove `mod scroll;` from `data_grid/mod.rs` and any `use self::scroll::ScrollToView;`. The `pub use crate::collection::ScrollState;` re-export added in Task 2 stays.

- [ ] **Step 3: Run the full data_grid suite (the guardrail)**

Run: `cargo test --all-features data_grid`
Expected: PASS — selection click (shift/toggle/replace), scroll-to-row, sort, filter, copy/TSV all still green. If any selection test fails, the central-click extraction changed behavior — diff `apply_row_click` against the original inline logic at `view.rs:1061–1095`.

- [ ] **Step 4: Add a grid arrow-key navigation test**

`data_grid` now gains arrow-key row nav via `CollectionBodyWidget`. Add a test (in `data_grid/view.rs` tests or a small integration test) that builds a grid, focuses the first body row, sends `ArrowDown`, and asserts focus moves to the second row. Reuse the `arrow_key` helper shape from `collection/body.rs` tests.

Run: `cargo test --all-features data_grid`
Expected: PASS including the new arrow-nav test.

- [ ] **Step 5: Run the whole suite + lint + gallery smoke**

Run: `cargo test --all-features`
Expected: PASS.

Run: `cargo clippy --all-targets --all-features`
Expected: clean — no dead code (`collection` fully used by `data_grid`).

Run: `cargo run -p void-ui --example gallery --features gallery` and exercise the Data Grid panel: click/shift-click/cmd-click selection, scroll-to-row buttons, sort, filter, copy, and Up/Down keys on a focused row.
Expected: behavior identical to before, plus working arrow-key nav.

- [ ] **Step 6: Commit**

```bash
git add src/components/data_grid src/collection
git rm src/components/data_grid/scroll.rs
git commit -m "data_grid: build on the collection substrate body seam"
```

---

## Task 8: Baseline measurement for per-row build cost

**Files:**
- Create: `benches/row_build.rs` (Criterion) **or** `src/collection/bench.rs` behind a `bench` cfg — choose Criterion if the workspace already has a `[dev-dependencies] criterion`; otherwise an instrumented `#[ignore]`d test that prints timing.
- Modify: `Cargo.toml` (add `[[bench]]` if using Criterion)

- [ ] **Step 1: Check for an existing bench harness**

Run: `grep -n "criterion\|\\[\\[bench\\]\\]" Cargo.toml`
Expected: shows whether Criterion is available. If absent, use the instrumented-test form (Step 2b).

- [ ] **Step 2a: Criterion bench (if available)**

Create `benches/row_build.rs` benchmarking the per-row build path: construct a `collection_body` over a 10k-item synthetic dataset with a non-trivial `render_row` (a `column_strip` of 8 cells), drive it to a fixpoint of ~40 visible rows in a `TestHarness`, then benchmark a full rebuild triggered by toggling one row's selection. Record the median.

- [ ] **Step 2b: Instrumented test (fallback)**

Add `#[ignore] #[test] fn bench_row_rebuild()` in `collection/body.rs` that does the same setup, times 1000 rebuilds with `std::time::Instant`, and `println!`s total/median. Run with `cargo test --all-features bench_row_rebuild -- --ignored --nocapture`.

- [ ] **Step 3: Record the baseline**

Run the bench/test, capture the number, and write it into the spec's "Performance" section (or a short `docs/notes/` entry) as **baseline (central clicks, no memoization)**. This is the before-number for Task 9.

- [ ] **Step 4: Commit**

```bash
git add benches Cargo.toml src/collection docs
git commit -m "collection: add per-row rebuild measurement + baseline"
```

---

## Task 9: Opt-in row memoization (prototype-gated)

> DEFERRED: This task is deferred to the `list` rebuild branch, where `list` is a
> real consumer that can supply `row_key` and the win can be measured against a
> real workload. `data_grid` on this branch does not consume memoization — only
> the unconditional central-click win (Task 8 baseline) is realized here.

**Files:**
- Modify: `src/collection/body.rs`, `src/collection/mod.rs`
- Test: memoization behavior test in `body.rs`

The flagged risk: `xilem_core::memoize(data, |&data| view)` gives the view closure only `&Data` (no `&mut State`), so a memoized row must reconstruct its content from owned data. Prototype against a real row before locking the API.

- [ ] **Step 1: Prototype one memoized row**

In a scratch test, wrap a single `column_strip` row in `memoize((row_snapshot, selected, theme), |(snap, sel, theme)| build_content(snap, *sel, theme))` where `row_snapshot` is an owned `Clone + PartialEq` projection of the row. Confirm: (a) it compiles within the `virtual_scroll` callback, (b) rebuilding with an unchanged snapshot does **not** re-run `build_content`, (c) a changed `selected` flag **does**. Record what `Data` had to contain — this determines the seam.

- [ ] **Step 2: Add the opt-in seam based on the prototype**

Add to `CollectionBodyParams` an optional projector. Based on the prototype, the most likely shape is:

```rust
/// Optional memoization key: when `Some`, each row is memoized on
/// `(key(item), selected)` so an unchanged row skips rebuild. Requires
/// the content to be reconstructable from `item` + `selected` + theme,
/// so `render_row` must already hold everything else by capture.
pub(crate) row_key: Option<Arc<dyn Fn(&Item) -> RowKey + Send + Sync>>,
```

where `RowKey` is a `Clone + PartialEq + 'static` owned value (e.g. `u64` id when content depends only on identity, or a small owned view-model when content depends on field values). If the prototype shows content depends on more than `(key, selected, theme)`, widen `RowKey` to carry the needed owned fields. Implement: when `row_key` is `Some`, wrap the row content in `memoize((key, is_selected), move |_| render_row(...))` — capturing an owned snapshot per the prototype; when `None`, the existing path.

- [ ] **Step 3: Test memoization behavior**

Add a test that supplies a `row_key`, rebuilds with identical inputs, and asserts the `render_row` closure ran fewer times (instrument with an `Arc<AtomicUsize>` counter incremented inside `render_row`); then change one row's selection and assert only that row's `render_row` re-ran.

Run: `cargo test --lib collection`
Expected: PASS.

- [ ] **Step 4: Re-measure and compare**

Re-run Task 8's bench/test with `row_key` supplied. Record the **after** number next to the baseline. If memoization does not measurably reduce rebuild cost on the selection-toggle scenario, note that finding honestly and keep the seam (it is still correct and useful for content-heavy rows) — do not claim a win the measurement doesn't show.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --all-targets --all-features`
Expected: clean.

```bash
git add src/collection docs
git commit -m "collection: opt-in row memoization via row_key + measured comparison"
```

---

## Done criteria (verify before opening the stacked branches)

- `cargo test --all-features` — green.
- `cargo clippy --all-targets --all-features` — no warnings.
- `grep -rn "RowIdSource\|ScrollToView" src/components/data_grid` — no hits (fully migrated).
- `data_grid` selection/scroll/sort/filter/copy behavior unchanged in the gallery; arrow-key row nav works.
- `SelectionState` / `ScrollState` reachable at the same public paths as before (`void_ui::SelectionState`, `void_ui::components::data_grid::SelectionState`).
- Baseline + after measurement recorded.

The `list` rebuild (delete its duplicated `ItemIdSource`/`visual_range_ids`/`clamp`/`ListBodyView`/`ListBodyWidget`, rebuild `list` on `collection_body` with `render_row = |item, selected, theme| item view`, `lazy = Some(...)`, optional `row_key`) is the **next branch**, stacked on this one, with its own plan.
