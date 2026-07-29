# Virtualize overlay suggestion/menu lists on the shared collection substrate

Closes #98. Approach C considered during design (promote virtualization into a
public `VirtualList` spanning `list`/`data_grid` too) was parked as #213 —
out of scope here.

> **Revision note (post-research correction):** the first version of this
> spec proposed reusing `RowClickable`/`CollectionBodyWidget` (the
> `list`/`data_grid` substrate). Research done while writing the
> implementation plan found that premise wrong — see "Key insight" below.
> This revision replaces that architecture; the Problem/scope stays the same.

> **Revision note 2 (found while implementing Task 2):** the revised design
> below still assumed `CollectionListWidget` could own `masonry::VirtualScroll`
> directly as a plain widget and answer its `Fetch` action from
> `Widget::on_action`. That doesn't compile: `VirtualScrollFetchAction` isn't
> `Clone`, and `on_action` only ever lends a pass-scoped `&ErasedAction` —
> there is no way to get ownership of it outside xilem's `View::message`
> mechanism. See "Key insight" and "Design" below for the corrected
> architecture: item content now flows through real, rebuild-diffed View
> props (mirroring how `list`/`data_grid` already work), and
> `xilem::view::virtual_scroll` is used directly rather than reimplemented.
> This also required moving `compute_filtered` out of `AutocompleteWidget`
> and into `AutocompleteView`, since only the View layer sees fresh
> `contents`/`suggestions` on every rebuild in a way a plain widget's
> `on_action` handler cannot durably act on. Confirmed with the user before
> proceeding, given the size of the change.

## Problem

`autocomplete`'s `SuggestionList`/`LabelList` and `dropdown_button`'s
`MenuContent` (`src/components/autocomplete/widget.rs`,
`src/components/dropdown_button/menu_layer.rs`) each materialize one widget per
item with no virtualization — `LabelList::set_items` and
`MenuContent::set_items` drain and rebuild every child from the full
`Vec<ArcStr>` on every call. The two widgets are also near-duplicates: both
are a single always-focused container tracking hover/highlight state over a
flat `Vec` of bare label rows, hit-tested via a parallel `item_rects: Vec<Rect>`
built at layout time — hand-synchronized hover/keyboard-highlight/scroll-into-view/
click-to-select logic that will drift apart if changed independently.

**Scope correction found during research:** `autocomplete`'s
`compute_filtered` (`widget.rs:905-916`) already caps *both* the empty-query
and filtered branches at `MAX_SUGGESTIONS = 20` — its own doc comment names
this as issue #98's stopgap. So today, `SuggestionList` never sees more than
20 items; virtualizing it alone produces no observable benefit unless this
cap is also removed. `dropdown_button` has no equivalent cap, so unbounded
item lists there hit the real problem today. **This issue also removes
`MAX_SUGGESTIONS` entirely** (decision confirmed with the user), relying on
the new substrate to bound actual widget count instead of a hard item-count
ceiling.

`dropdown_button`'s `MenuContent` also has **no height cap or scrolling at
all** today (`measure()` sizes vertically to `item_height * item_count`,
unbounded) — a menu with many items just grows arbitrarily tall. Virtualizing
via masonry's `VirtualScroll` requires a **bounded viewport**: `VirtualScroll`
materializes what's visible within a fixed-height viewport, so an
unbounded-height container defeats the point (it would just ask for
everything). This issue therefore also gives `MenuContent` a capped,
scrollable viewport for the first time, matching `SuggestionList`'s existing
`MAX_LIST_HEIGHT` (200px) constant/value for consistency between the two.

## Key insight (revised)

The original plan for this issue proposed reusing the `list`/`data_grid`
substrate — `CollectionBodyWidget` + `RowClickable` (`src/collection/`) —
imperatively. Research done before writing the implementation plan found this
doesn't fit:

- **`RowClickable`'s model is per-row real focus** — `accepts_focus() == true`
  on every row, keyboard focus physically moves between rows via
  `ctx.set_focus(target)` (a roving-tabindex ARIA pattern). `SuggestionList`/
  `LabelList` and `MenuContent` use a **different, equally-valid ARIA
  pattern**: a single always-focused container (`LabelList` itself, or the
  dropdown's trigger button) plus a virtual `highlighted: Option<usize>`
  index, surfaced to assistive tech via accesskit's active-descendant
  relationship (`node.set_active_descendant(...)`, `LabelList::widget.rs:755-766`).
  Migrating onto `RowClickable` would move real keyboard focus between rows —
  a legitimate but *different*, more invasive pattern — and would require
  rewriting (not adapting) `tab_into_listbox_and_arrow_keys_set_active_descendant`
  (`autocomplete/widget.rs:2423-2518`), since what it asserts (Tab lands on a
  `Role::ListBox` container; active-descendant tracks the highlight) would no
  longer hold. **Confirmed with the user: preserve the existing single-focus
  pattern instead.**
- **Wrap-around isn't a gap.** `LabelList::move_highlight`
  (`widget.rs:404-424`) already wraps via `rem_euclid` over the *full* item
  count, not just the materialized window — this ports forward almost
  verbatim; no new wrap-around logic or tests are needed for the mechanism
  itself, only for the widget doing the materializing/scrolling underneath it.
- **`CollectionBodyWidget`/`apply_row_click`/`SelectionState` don't apply
  either** — those exist to serve `list`/`data_grid`'s multi-select,
  tree-nav, per-row-focus model. None of that machinery serves a
  single-highlighted-index, single-focus-target list.

**Why `on_action` can't work at all for `Fetch`** (verified against the pinned
masonry/xilem commit, `c5950bc`): `VirtualScrollFetchAction`
(`masonry/src/widgets/virtual_scroll.rs:57`) derives only `Debug`, not
`Clone`. `Widget::on_action` hands out `action: &ErasedAction` — a single
shared reference bubbled through every ancestor in one pass
(`masonry_core/src/passes/action.rs`'s `handle_action`), never ownership.
Every `ActionCtx` deferred-mutation entry point (`mutate_child_later`,
`mutate_self_later`, `mutate_later`) requires `'static` closures, and
`VirtualScroll::will_handle_action` (the only thing that updates its private
`active_range`) requires the *exact* action instance masonry handed it — not
a reconstructed equivalent from the two `Range<usize>` values alone, since its
fields are private with no public constructor. There is no synchronous
workaround either: `ActionCtx` has no `get_mut` (that's `MutateCtx`-only) and
`VirtualScroll` doesn't implement `AllowRawMut`. This is a hard gap, not a
local bug — confirmed by an implementer hitting it while transcribing the
first version of this spec's design verbatim.

**The fix xilem already provides, already proven by `list`/`data_grid`:**
`xilem_masonry::VirtualScroll`'s own `View` implementation
(`xilem_masonry/src/view/virtual_scroll.rs`) receives `Fetch` with real
ownership via `View::message` (`message.take_message::<VirtualScrollAction>()`,
a completely different, higher-level mechanism from masonry's `on_action` —
confirmed independent of masonry's action-bubbling/handled status by reading
the actual dispatch: `xilem`'s driver (`xilem/src/driver.rs`) routes an
action to whichever `View` registered the action's *source* `WidgetId` via
`with_action_widget`/`record_action_source` at that widget's own `build()`,
regardless of what any ancestor `Widget::on_action` does). It stores the
owned action in its `ViewState` and returns `MessageResult::RequestRebuild`;
the actual `will_handle_action`+`add_child`/`remove_child` work happens
inside its own `rebuild()`, which has a real `ViewCtx`. `RequestRebuild`
triggers a full window-view-tree `rebuild()` call (`self == prev`, no
`app_logic` re-invocation, confirmed in `xilem/src/driver.rs`'s
`handle_message_result`) — this is exactly the mechanism
`src/collection/body_view.rs`'s `collection_body` already relies on today for
`list`/`data_grid`'s virtualization, in production.

**The catch, and why this isn't a drop-in:** `virtual_scroll(len, func)`'s
`func: Fn(&mut State, usize) -> ChildrenViews` must return a real composed
`ChildrenViews: WidgetView<State, Action>` for each visible row — not a plain
masonry `Widget`. There is no existing helper in this codebase or in
`xilem`/`xilem_masonry` to lift an already-built widget into a trivial
`WidgetView` — every `WidgetView`-typed slot in this codebase is filled by an
actual `View` impl (confirmed by exhaustive search). And `func`'s row content
needs *something* to resolve `pos -> item text`, which pulls `items` into
`State`-diffed View territory — but autocomplete's filtered suggestions are
currently computed inside `AutocompleteWidget`'s own widget-level event
handling (`compute_filtered`, driven by `self.contents`/`self.all_suggestions`,
both widget-owned fields), then pushed to the mounted list imperatively via
`mutate_later`/`mutate_child_later` — bypassing `SuggestionListView`'s
`rebuild()` entirely (confirmed: its `rebuild()` doc says exactly this,
`view.rs:174`, and its `build()` constructs an empty shell). Feeding real,
diffed `items` into `virtual_scroll`'s closure requires the filtering to move
from widget-event-time to View-build-time — a genuine, if contained,
restructuring, not just a new file.

The good news, found while working this out: `AutocompleteView`'s `contents`
and `suggestions` fields **already** flow as real, rebuild-diffed View props
today (`view.rs:324-341`: `AutocompleteWidget::set_contents`/
`set_all_suggestions` are called from `rebuild()`, not `mutate_later`) — the
text is host-controlled by contract (`view.rs:20-22`'s module doc) and the
full candidate list is a normal constructor argument. So `compute_filtered`
can move into `AutocompleteView::rebuild()`/`build()` using data that's
*already there*; nothing new needs to be invented to get its inputs. What
does need building: a small View wrapper around `OverlayListItem` (mirroring
the existing `RowClickable`/`ClickableRow` pair in `row_click.rs` — a masonry
`Widget` that submits an action, paired with a `View` that catches it via
`message()` with real ownership, no re-emission needed since the row's own
id is what gets registered), and a crate-internal function wrapping
`xilem::view::virtual_scroll` directly (mirroring `collection_body`'s shape,
much simplified — no selection/tree-nav/lazy-load/click-routing machinery).

Confirmed with the user before proceeding, given this changes more of
`AutocompleteWidget`'s existing architecture than the original task scope
assumed (moving `compute_filtered` out of the widget, and reversing an
existing "don't re-register the portal on keystroke" optimization in
`AutocompleteView::rebuild()`, since items must now flow through that same
registration path).

What genuinely is reusable: **masonry's `VirtualScroll` widget itself**
(`masonry_core::widgets::VirtualScroll`, re-exported via `masonry::widgets`),
used directly rather than through `collection`'s `CollectionBodyWidget`
wrapper. Its real imperative API (verified against the pinned commit,
`masonry/src/widgets/virtual_scroll.rs` at the `c5950bc` checkout
`Cargo.lock` resolves to):

- `VirtualScroll::new(initial_anchor: usize, len: usize) -> Self`; runtime
  resize via `VirtualScroll::set_len(this: &mut WidgetMut<'_, Self>, len: usize)`
  — no reconstruction needed when the item count changes.
- `VirtualScroll::add_child(this: &mut WidgetMut<'_, Self>, idx: usize, child: NewWidget<dyn Widget>)` /
  `remove_child(this: &mut WidgetMut<'_, Self>, idx: usize)` — both
  `debug_assert!` that `will_handle_action` was already called for the
  in-flight `Fetch` action; calling either outside that reaction is a
  contract violation.
- `VirtualScroll::will_handle_action(this: &mut WidgetMut<'_, Self>, action: &VirtualScrollFetchAction)`,
  `scroll_to(this: &mut WidgetMut<'_, Self>, idx: usize)`.
- `VirtualScrollAction` has exactly two variants: `Fetch(VirtualScrollFetchAction)`
  (`.old_active() -> &Range<usize>`, `.target() -> &Range<usize>`) and
  `Scroll(VirtualScrollScrollAction)` (`.range_in_viewport() -> &Range<usize>`).
- `children_ids(&self) -> ChildrenIds` and
  `child_mut(this: &mut WidgetMut<'_, Self>, idx: usize) -> WidgetMut<'_, dyn Widget>`.
- **The critical mechanic**: `VirtualScroll` decides materialization itself
  from viewport/overscan geometry during its own `layout()`, and — only when
  the computed range differs from its stored active range — submits
  `VirtualScrollAction::Fetch` as a widget action via `ctx.submit_action`. A
  driver **cannot** proactively call `add_child`/`remove_child` from
  `set_items`; it must react to `Fetch`. As established above ("Why
  `on_action` can't work at all for `Fetch`"), that reaction cannot happen
  through masonry's `Widget::on_action` bubbling — `VirtualScrollFetchAction`
  isn't `Clone` and `on_action` only ever lends a shared, pass-scoped
  reference. `Fetch` is instead delivered with real ownership through Xilem's
  `View::message`, to whichever `View` registered the action's source
  `WidgetId` via `with_action_widget`/`record_action_source` — the same
  mechanism `xilem_masonry::VirtualScroll`'s own `View` impl and
  `collection_body` already rely on in production.
- Resizing alone (`set_len`) does **not** itself add/remove anything or
  refresh already-materialized rows' *content* — only a `Fetch` reaction does
  that for rows entering/leaving the window. A same-length `set_items` call
  (new keystroke, same filtered count) triggers no `Fetch` at all, so
  `CollectionListWidget::set_items` must explicitly refresh every
  currently-materialized row's content itself in that case — nothing else
  will.

## Design

### `src/collection/item_row.rs` — `OverlayListItem` (already built, Task 1 — gains one addition)

Already implemented and reviewed: a pointer-interactive row widget (hover,
highlight-ring paint, submits its own text on click as `Widget::Action =
ArcStr`), replacing the former paint-only `SuggestionItem`. This revision
adds one thing not anticipated originally: `set_text(this: &mut WidgetMut<'_,
Self>, text: ArcStr)`. Reason: with `virtual_scroll` (below) rebuilding
already-materialized rows' *content* on every ordinary rebuild pass (its own
internal contract, not new machinery we're adding), an existing row can be
asked to show different text at the same index — e.g. a same-length filtered
result on a new keystroke — without being torn down and rebuilt from scratch.
`set_theme`/`set_highlighted` (already built) are unchanged.

### New: `src/collection/item_row_view.rs` — `OverlayListItemView`

A small `View` wrapper around `OverlayListItem`, mirroring the existing
`RowClickable`/`ClickableRow` pair in `src/collection/row_click.rs` (a
masonry `Widget` that submits an action, paired with a `View` that catches it
via `message()` with real ownership — the same shape, at a smaller scale,
since `OverlayListItem` needs no click-modifier/selection-state translation,
just "the row was clicked, run this callback with the text"):

- `pub(crate) fn overlay_list_item<State, Action>(text: ArcStr, highlighted: bool, theme: &Theme, role: Role, on_select: impl Fn(&mut State, ArcStr) -> Action + Send + Sync + 'static) -> impl WidgetView<State, Action>`
- `build()`: constructs `OverlayListItem::new(text, highlighted, theme, role)`, registers via `ctx.with_action_widget(...)` (exactly like `ClickableRow::build`).
- `rebuild()`: diffs `text`/`highlighted`/`theme` against `prev`, calling `OverlayListItem::set_text`/`set_highlighted`/`set_theme` on change. (`role` never changes post-construction — no setter needed.)
- `message()`: `message.take_message::<ArcStr>()` → `MessageResult::Action((self.on_select)(app_state, text))`. No re-emission needed anywhere else in the chain — `OverlayListItem`'s own registered id is exactly its action's source id, same as `RowClickable`'s.

### New: `src/collection/overlay_list_body.rs` — `overlay_list_body`

A crate-internal function wrapping `xilem::view::virtual_scroll` directly
(the same mechanism `collection_body` already uses for `list`/`data_grid`,
confirmed via its own `View::message`/`rebuild` split — `message()` stores an
owned `Fetch` action and returns `RequestRebuild`; `rebuild()` applies
`will_handle_action`+`add_child`/`remove_child` with a real `ViewCtx`, no
`Clone` or ownership problem since this is xilem's own, already-working
implementation, not something this crate reimplements) — heavily simplified
relative to `collection_body`: no `Item`/`State` generic beyond what
`virtual_scroll` itself needs, no selection lens, no tree metadata, no
lazy-load, no click-routing helpers (`apply_row_click`/`apply_row_activate`
don't apply — see "Key insight"):

- `pub(crate) fn overlay_list_body<State, Action>(items: Arc<Vec<ArcStr>>, highlighted: Option<usize>, theme: &Theme, item_role: Role, on_select: impl Fn(&mut State, ArcStr) -> Action + Send + Sync + Clone + 'static) -> impl WidgetView<State, Action>`
- Internally: `virtual_scroll(items.len(), move |_state, pos| { let text = items[pos].clone(); overlay_list_item(text, highlighted == Some(pos), &theme, item_role, on_select.clone()) })`.
- `items`/`highlighted` are captured directly in the closure (not read from `State` via an accessor) — they're plain constructor arguments to `overlay_list_body`, supplied fresh by its caller (`SuggestionListView`/`MenuContentView`) on every rebuild where they've changed. This is what makes the "no App-state round-trip per keystroke, filtering is void_ui's own concern" property hold: `State` here is only present because `WidgetView<State, Action>` needs a type for eventual action routing, not because item content is derived from it.

### Repurposed: `src/collection/imperative_list.rs` — `CollectionListWidget`

No longer owns items, no longer builds `VirtualScroll` itself, no longer
needs any `Fetch` handling — `overlay_list_body`'s own `virtual_scroll`
child does all of that now. Becomes purely keyboard-nav/highlight/focus
bookkeeping wrapping a View-supplied `VirtualScroll`, structurally parallel
to `CollectionBodyWidget` wrapping its own `VirtualScroll` child:

- `pub(crate) fn new(child: NewWidget<VirtualScroll>, item_count: usize, container_role: Role) -> Self` — takes an *already-built* `VirtualScroll` (from `overlay_list_body`'s element), not constructing one itself.
- Fields: `child: WidgetPod<VirtualScroll>`, `item_count: usize` (kept in sync via a setter — needed for `move_highlight`'s wrap bound, since items themselves no longer live here), `active_start: usize`, `highlighted: Option<usize>`.
- `set_item_count(this: &mut WidgetMut<'_, Self>, count: usize)`: clamps `highlighted` past the new end (reusing `clamp_scroll_index`'s pattern).
- `move_highlight`/`set_highlight`: unchanged in spirit from the previous revision — `rem_euclid` wrap over `self.item_count` (not `items.len()`, since there's no `items` field anymore); scroll-into-view is index-based only (`VirtualScroll::scroll_to` when the target isn't materialized — this widget sits above `VirtualScroll`, so it can't request a descendant's minimal reveal the way `RowClickable` does elsewhere in `collection`). Pushes `highlighted` onto materialized rows via `VirtualScroll::child_mut`+`OverlayListItem::set_highlighted` — wait, rows are now `OverlayListItemView`'s elements, still concretely `OverlayListItem` widgets underneath, so this direct `WidgetMut`-based push still works exactly as before.
- `accepts_focus() -> true`; keyboard handling (ArrowUp/Down/Home/End) — unchanged in spirit, using `item_count` instead of `items.len()`. Enter/click-selection are no longer this widget's concern at all — `OverlayListItemView`'s `on_select` callback (wired per-row, at `overlay_list_body`'s construction) handles selection directly; `CollectionListWidget` has nothing left to catch or re-emit.
- Accessibility: `accessibility_role() -> container_role` (parameterized — `Role::ListBox` for autocomplete, `Role::Menu` for a dropdown); active-descendant tracks `highlighted`.

### New: `src/collection/overlay_list.rs` — the View wrapping `CollectionListWidget`

Structurally parallel to `CollectionBodyWidget`/`CollectionBodyView`'s
existing split: a thin `View` whose `Element = Pod<CollectionListWidget>`,
built by wrapping `overlay_list_body`'s own element:

- `pub(crate) fn overlay_list<State, Action>(items: Arc<Vec<ArcStr>>, highlighted: Option<usize>, theme: &Theme, container_role: Role, item_role: Role, on_select: ...) -> impl WidgetView<State, Action>`
- `build()`: `let (child_element, child_state) = overlay_list_body(...).build(ctx, state); Pod::new(CollectionListWidget::new(child_element.new_widget, items.len(), container_role))` (mirrors `CollectionBodyView::build`'s exact shape).
- `rebuild()`: forwards to the child's `rebuild()` (via a `virtual_scroll_mut`-style accessor on `CollectionListWidget`, mirroring `CollectionBodyWidget::virtual_scroll_mut`); then, if `items.len()` changed, calls `CollectionListWidget::set_item_count`.
- `message()`: forwards to the child's `message()` (mirrors `CollectionBodyView::message`) — this is where `Fetch` gets caught (by `virtual_scroll`, several layers down) and where a row's selection (`OverlayListItemView`'s own `Action`) resolves, entirely within the forwarded call chain — no interception needed at this level.

### Rewritten: `SuggestionList`/`SuggestionListView` (`src/components/autocomplete/widget.rs`, `view.rs`)

`LabelList`/`SuggestionItem` are deleted (superseded by the above).
`SuggestionList` (the masonry widget) keeps only its chrome (rounded-rect
background/border paint, `MAX_LIST_HEIGHT` measure cap) wrapping a
`WidgetPod<CollectionListWidget>` — no more `ScrollView` (redundant,
`VirtualScroll` is itself a scrollable viewport), no more `on_action`
re-emission (nothing left to re-emit; selection resolves at the
`OverlayListItemView`/`overlay_list_body` layer).

`SuggestionListView` changes from an empty-shell-plus-`mutate_later` view to
one that builds `overlay_list(...)` directly, wrapping its element the same
way `SuggestionList`'s chrome needs. **`compute_filtered` moves from
`AutocompleteWidget::handle_text_changed` (widget-event-time) into
`AutocompleteView`** (view-build-time), computed from `self.contents`/
`self.suggestions` — both already real, rebuild-diffed fields (`view.rs:324-341`)
— and threaded down into `SuggestionListView`'s construction as `items:
Arc<Vec<ArcStr>>`. `on_select` is wired here too: selecting a suggestion
calls the host's `on_changed` with the selected text (the same outcome
`SuggestionSelected` produces today, just resolved one layer differently).

**Reverses an existing optimization, deliberately:** `AutocompleteView::rebuild()`
currently re-registers the portal content (`portal.update(...)`) only on
theme change, with an explicit comment that suggestion/keystroke changes
"reach the mounted list directly through the widget layer... so
re-registering for them would be pure churn" (`view.rs:343-347`). That
comment's premise (items are pushed imperatively, so View-level
re-registration would be redundant work) no longer holds once items are
real View props — `portal.update` must now also fire when `contents`/
`suggestions` (hence the filtered result) changes. This was an accepted,
deliberate cost of moving to real View props, not an oversight.

`AutocompleteWidget` loses `all_suggestions`/`all_suggestions_lower`/
`filtered`/`compute_filtered` and the `mutate_child_later`/
`queue_portal_repopulation` machinery that pushed items to `SuggestionList` —
none of that has a job left once `AutocompleteView` computes and passes
`items` directly. It keeps `contents`/cursor/open-state and whatever else is
unrelated to suggestion filtering.

### Rewritten: `MenuContent`/`MenuContentView` (`src/components/dropdown_button/menu_layer.rs`, `view.rs`)

Same shape, smaller delta: `MenuContentView` **already** computes
`item_labels` reactively via normal `rebuild()` diffing (`Arc::ptr_eq`
against `self.items`, `view.rs:391-406`) — no filtering-relocation needed
here, just building `overlay_list(...)` directly instead of wrapping a
plain `MenuContent` widget that owned `VirtualScroll` imperatively.
`MenuContent` (the masonry widget) keeps its background/border chrome
wrapping `WidgetPod<CollectionListWidget>`, plus the new `MAX_LIST_HEIGHT`
cap (previously unbounded — `measure()` sized to `item_height *
item_count`). `MenuItemSelected(usize)` stays index-based; `on_select`
resolves the index against `MenuContentView`'s own `items` at selection
time (mirroring `apply_row_activate`'s "resolve at the moment of the click"
rationale elsewhere in `collection`). `ThemedDropdownButton`'s in-tree
fallback path (no portal scope ancestor) needs the equivalent restructuring
— it currently constructs `MenuContent` directly and calls `set_items`
imperatively; that becomes hosting `overlay_list(...)` too, or (if the
in-tree path is meaningfully harder to convert) an explicit, separately-flagged
decision to leave it non-virtualized for now, not a silent gap.

### Not touched

`collection_body`, `CollectionBodyWidget`/`CollectionBodyView`, `RowClickable`
(pattern *mirrored*, not reused directly), `SelectionState`,
`apply_row_click`/`apply_row_activate`, `list`, `data_grid`.

## Data flow

1. **Population.** `AutocompleteView`/`MenuContentView` compute the full
   filtered/static `Vec<ArcStr>` from their own real, rebuild-diffed fields
   (`compute_filtered(contents, suggestions)` for autocomplete; direct
   `item_labels` diffing for the dropdown, unchanged) and pass `items:
   Arc<Vec<ArcStr>>` into `overlay_list(...)`'s construction — a normal View
   prop, no imperative push.
2. **Virtualization.** `overlay_list_body`'s `virtual_scroll` child computes
   its own materialized range in `layout()` and submits `Fetch` only when it
   changes; xilem's View-message system (not masonry `on_action`) delivers it
   with ownership to `virtual_scroll`'s own `message()`/`rebuild()` — the
   proven mechanism `list`/`data_grid` already rely on.
3. **Same-length content change** (a new keystroke's filtered result at the
   same count). No `Fetch` fires (masonry's `VirtualScroll::set_len` is a
   no-op for an unchanged length), but `virtual_scroll`'s own `rebuild()`
   unconditionally rebuilds every currently-materialized row's *View* each
   pass anyway (its own existing contract, not new machinery) — reaching
   `OverlayListItemView::rebuild()`, which diffs `text` and calls
   `OverlayListItem::set_text` if it changed.
4. **Keyboard nav.** `CollectionListWidget` is the single focus target;
   ArrowUp/Down/Home/End call `move_highlight`/`set_highlight`, wrapping via
   `rem_euclid` over `item_count`. Highlight change scroll-into-view is
   index-based (`VirtualScroll::scroll_to` when the target isn't
   materialized).
5. **Click/select.** `OverlayListItem` submits its own text on click;
   `OverlayListItemView::message()` catches it with ownership (no
   re-emission anywhere in the chain) and runs the `on_select` callback
   wired at `overlay_list_body`'s construction, translating to the host's
   real `Action`.

## Edge cases / error handling

- **Empty item list.** `items.len() == 0`: `virtual_scroll`'s own `Fetch`
  removes all materialized rows; `CollectionListWidget::set_item_count(0)`
  clamps `highlighted` to `None`.
- **Highlighted index past the new list's end** after items shrink. Clamp
  via the same pattern `clamp_scroll_index` already provides, in
  `set_item_count`.
- **Rapid keystrokes.** Each `AutocompleteView::rebuild()` computes a fresh
  `filtered` and passes it down; `virtual_scroll`'s own diffing/rebuild
  handles whatever changed (length, content, or both) — no stale-state risk,
  since this is now the same mechanism `list`/`data_grid` already rely on for
  arbitrarily-fast state changes.
- **`Fetch` reaction ordering.** Entirely `xilem_masonry`'s own concern now
  (`will_handle_action` before add/remove, inside its own `rebuild()`) — not
  something this crate's code needs to get right by hand.
- **Highlight moved to an unmaterialized target** (Home/End on a large
  list). `set_highlight`'s `VirtualScroll::scroll_to` call brings the target
  into the materialized window; the resulting `Fetch` (handled by
  `virtual_scroll`, not this widget) builds its row via the same
  `overlay_list_item(...)` closure, already carrying the correct
  `highlighted` flag.
- **Portal re-registration on every keystroke** (autocomplete). A real,
  accepted cost of this design — see "Rewritten: SuggestionList" above.
- **`MenuContent`'s new height cap** changes previously-unbounded-height
  dropdowns to a capped, scrollable viewport — a visible, intentional
  behavior change.

## Testing plan

- **Existing autocomplete test suite** must keep passing after this
  restructuring — `tab_into_listbox_and_arrow_keys_set_active_descendant`
  and `enter_in_listbox_selects_closes_and_returns_focus_to_input` need
  adaptation (fixture: `CollectionListWidget` is now the focus target,
  `SuggestionList` no longer holds `text_area_handle`/`autocomplete_handle`
  the same way since selection resolves at the View layer now — work out the
  exact new home for refocus-on-select/close-on-select during
  implementation, don't assume it's identical to the previous revision's
  plan). `compute_filtered`'s tests move/adapt alongside its relocation to
  `view.rs`, and update for the removed `MAX_SUGGESTIONS` cap.
- **New `OverlayListItemView`/`overlay_list_body`/`overlay_list` tests**:
  build/rebuild/message wiring (mirroring `row_click.rs`'s own test patterns
  for `ClickableRow`, and `body_view.rs`'s for `CollectionBodyView`) — a
  large list materializes a bounded window (the core virtualization
  contract), a same-length content change updates existing rows' text
  in-place (the `set_text` case this revision added), a length change
  triggers `Fetch` correctly.
- **New `CollectionListWidget` tests**: `move_highlight`/`set_highlight`
  wrap-around and clamp-on-shrink (unchanged in spirit from the previous
  revision, just against `item_count` instead of `items.len()`).
- **New `MenuContent` height-cap test**: menu with many items measures to
  `MAX_LIST_HEIGHT`, not `item_height * item_count`.
- **Manual gallery verification** (defer to the human — no claimed visual
  verification): autocomplete and dropdown_button demo panels with a large
  candidate list, confirming bounded materialized widget count and that
  keystroke responsiveness feels unchanged despite the portal-re-registration
  change.

## Acceptance criteria (from #98, revised)

- [ ] Autocomplete + dropdown_button overlay lists virtualize (bounded widget
      count regardless of item count) — verified with `MAX_SUGGESTIONS`
      removed.
- [ ] `SuggestionList` and `MenuContent` share the virtualization/highlight/
      scroll-into-view/click substrate (`overlay_list`/`overlay_list_body`/
      `CollectionListWidget`/`OverlayListItem`) rather than duplicating it.
- [ ] Existing autocomplete + dropdown_button behavior/accessibility tests
      still pass (with adaptation for the relocated focus/selection
      machinery, not a behavior change).
- [ ] Keyboard-highlight wrap-around at list ends is preserved.
- [ ] `dropdown_button`'s menu gains a bounded, scrollable viewport
      (previously unbounded height) — a new, intentional behavior change.
- [ ] Autocomplete's suggestion filtering flows as a real View prop
      (`AutocompleteView`), not an imperative widget-to-widget push —
      confirmed acceptable despite the portal-re-registration cost this adds
      on every keystroke.
