# Consolidate materialized-window index arithmetic between the two collection substrates

Addresses #213 ("Consider promoting collection virtualization into a public
`VirtualList` component"), scoped down after research: `list`/`data_grid` are
already virtualized (via `collection_body`/`CollectionBodyWidget`,
`src/collection/body.rs`/`body_view.rs`) and #98 added a second, separate
substrate for the overlay lists (`CollectionListWidget`,
`src/collection/imperative_list.rs`/`overlay_list.rs`) because overlay-list
keyboard nav needs imperative `&mut self` access with no View-level state
channel — a real constraint, not an oversight. This issue does **not**
introduce a public `VirtualList`, and does **not** unify the two widgets'
navigation/selection models (`RowClickable`'s per-row real focus vs
`CollectionListWidget`'s single-focus roving-highlight are different, equally
valid ARIA patterns — confirmed with the user to keep them separate). It
targets one specific, real duplication between the two: both independently
convert between a global item index and its position among the currently
materialized `VirtualScroll` children, using the same
`.contains()`/`checked_sub()` arithmetic, hand-rolled separately in each file.

## Background

`body.rs`'s `CollectionBodyWidget::refresh_row_nav` and
`imperative_list.rs`'s `CollectionListWidget` (four call sites:
`refresh_highlight_row`, both branches of `set_highlight`, and the `Enter`
key handler in `on_text_event`) each implement the same "is global index
`idx` currently materialized, and if so at which child slot" check:

```rust
(active_start..active_start + materialized_count).contains(&idx)
    && let Some(k) = idx.checked_sub(active_start)
```

and the inverse (`body.rs`'s `refresh_row_nav`: `active_start + k`).

Two recent post-#98 fixes (`7304f65`, `11e59b1`) were bugs in this
"materialized window" bookkeeping's *call ordering* inside
`imperative_list.rs`/`overlay_list.rs` (e.g. resolving a clamp against a
stale `active_start` because `set_item_count` ran before `set_active_start`).
**This design does not address those** — they're already fixed, and a shared
arithmetic helper wouldn't have prevented an ordering bug on its own (see
"Non-goals").

## Design

New crate-internal file `src/collection/window.rs`:

```rust
/// Global-index <-> materialized-slot conversion for a `VirtualScroll`'s
/// currently materialized window, described by its starting global index.
/// Shared by `CollectionBodyWidget` (body.rs) and `CollectionListWidget`
/// (imperative_list.rs) so both stop hand-rolling the same bounds check.
pub(crate) struct MaterializedWindow {
    active_start: usize,
}

impl MaterializedWindow {
    pub(crate) fn new(active_start: usize) -> Self {
        Self { active_start }
    }

    pub(crate) fn set_active_start(&mut self, active_start: usize) {
        self.active_start = active_start;
    }

    /// The materialized slot for global index `idx`, given how many rows
    /// are currently materialized (`VirtualScrollWidget::children_ids().len()`),
    /// or `None` if `idx` isn't currently materialized.
    pub(crate) fn slot_for(&self, materialized_count: usize, idx: usize) -> Option<usize> {
        (self.active_start..self.active_start + materialized_count)
            .contains(&idx)
            .then(|| idx - self.active_start)
    }

    /// The global index at materialized slot `slot`.
    pub(crate) fn index_for_slot(&self, slot: usize) -> usize {
        self.active_start + slot
    }
}
```

### `imperative_list.rs`

`CollectionListWidget`'s `active_start: usize` field becomes `window:
MaterializedWindow`. `set_active_start` delegates to
`self.window.set_active_start(...)`. The four duplicated bounds-check sites
(`refresh_highlight_row`; both branches of `set_highlight`; the `Enter` key
handler in `on_text_event`) call `self.window.slot_for(materialized_count,
i)` instead of the inline `.contains()`/`checked_sub()` expression. No
change to call order, no change to `set_item_count`/`move_highlight`/
`set_highlight`'s external behavior or signatures — this is an internal
representation swap.

### `body.rs`

`refresh_row_nav(this, active_start: usize)` keeps its existing parameter
(not converted to a field — it's the only use site, and `body_view.rs`'s
call convention doesn't change). Internally, it constructs a
`MaterializedWindow::new(active_start)` and replaces `let idx = active_start +
k;` with `window.index_for_slot(k)`.

### Non-goals

- **Does not fix or touch the `7304f65`/`11e59b1` ordering bugs.** Those were
  about *when* `set_active_start`/`set_item_count`/highlight-resolution run
  relative to each other in `overlay_list.rs`'s `rebuild`, not about the
  index arithmetic itself. Already fixed; this refactor preserves the
  existing (correct) call order verbatim.
- **Does not unify `active_start`'s ownership model.** `body.rs` keeps
  passing it as a parameter; `imperative_list.rs` keeps it as a field. Only
  the arithmetic is shared.
- **No public API.** `MaterializedWindow` is `pub(crate)`, lives in
  `src/collection/`, not re-exported.
- **No change to `list`/`data_grid`/`autocomplete`/`dropdown_button`'s
  external behavior, nav model, or selection model.**

## Testing plan

- New unit tests for `MaterializedWindow` (`window.rs`): `slot_for` returns
  `Some(k)` for an index inside `[active_start, active_start +
  materialized_count)` with the correct slot, `None` just below
  `active_start` and just at/above `active_start + materialized_count`;
  `index_for_slot` round-trips with `slot_for`; `materialized_count == 0`
  always yields `None` regardless of `active_start`.
- No new tests for `body.rs`/`imperative_list.rs` — this is a pure internal
  refactor (same inputs produce the same outputs), so the existing test
  suites for both modules (`cargo test --lib collection::body`, `cargo test
  --lib collection::imperative_list`) must keep passing unmodified as the
  acceptance bar.
- `cargo clippy --all-targets --all-features` clean.
- No gallery/manual verification needed — no observable behavior change in
  any component.

## Acceptance criteria

- [ ] `src/collection/window.rs` exists with `MaterializedWindow` and passing
      unit tests covering the boundary cases above.
- [ ] `CollectionListWidget` (`imperative_list.rs`) uses `MaterializedWindow`
      in place of its raw `active_start` field and inline bounds checks; all
      three duplicated call sites converted.
- [ ] `CollectionBodyWidget::refresh_row_nav` (`body.rs`) uses
      `MaterializedWindow::index_for_slot` in place of the inline
      `active_start + k`.
- [ ] Full existing test suite (`cargo test --all-features`) passes
      unmodified — no behavior change.
- [ ] `cargo clippy --all-targets --all-features` clean.
