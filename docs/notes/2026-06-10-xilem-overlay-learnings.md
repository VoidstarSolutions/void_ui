# Xilem overlay learnings — raw material for an upstream proposal

Findings from building `void_ui`'s popover overlay portal (branch
`popover-overlay-portal`, June 2026). Every claim below was empirically hit
during that work and verified against the pinned sources: the linebender
xilem repo at rev `4eae66d` (`masonry_core`, `masonry`, `masonry_winit`,
`xilem_core`, `xilem_masonry` paths refer to that tree). The end product in
this repo is `src/overlay_portal.rs` + `src/overlay_scope.rs` +
`src/components/popover/` — a fully userspace portal that mounts stateful
popover content into an always-last-painted slot. This note is the seed for
an upstream xilem proposal: what the platform already has, what xilem is
missing, and what a first-class portal/layer view would have to subsume.

## 1. The problem: paint order is structure, and structure is fixed

Masonry paints strict depth-first in `children_ids()` registration order
(`masonry_core/src/passes/paint.rs`). There is no z-order, no reparenting,
and no paint-order override — `PaintLayerMode::IsolatedScene` /
`PaintLayerMode::External` (`masonry_core/src/core/paint_layer.rs`) affect
*compositing* of a subtree's scene, not where in the order it paints.
Consequently an in-tree overlay (e.g. `src/anchored_overlay.rs`) is occluded
by ANY later-painting sibling of any of its ancestors — in the gallery, a
popover opened inside a demo block was painted under the code block that
followed it.

Hit-testing is the mirror image: `find_widget_under_pointer`
(`masonry_core/src/core/widget.rs:578`) walks children in **reverse**
registration order (line 602, "increasing z-order, picking the last child"),
and stashed widgets are skipped by both hit-testing
(`widget.rs:587-589`) and paint (`masonry_core/src/passes/paint.rs`, the
`is_stashed` checks). So "registered last + stash to hide" is the only
native vocabulary for overlays inside the tree — which is exactly what
`OverlayScope`/`PortalSlot` are built from.

## 2. What masonry already has: the `Layer` system

Masonry (at `4eae66d`) already ships a window-level layer mechanism:

- `LayerStack` at the `RenderRoot`
  (`masonry_core/src/app/layer_stack.rs`) — "Other layers can represent
  tooltips, menus, dialogs, etc."
- `create_layer` / `create_attached_layer` / `remove_layer` /
  `reposition_layer` on the widget context methods
  (`masonry_core/src/core/contexts.rs:1993,2029,2076,2097`). These take a
  `NewWidget` **directly** — no `Send` closure involved, unlike
  `mutate_later`.
- The `Layer` trait (`masonry_core/src/core/layer.rs`) with
  `capture_pointer_event`, which is "called for every layer for all pointer
  events, even those outside the layer's root widget" — i.e. exactly the
  global-dismissal primitive a popover backdrop wants.
- Driver support in `masonry_winit`
  (`masonry_winit/src/event_loop_runner.rs:671-685` renders
  `overlay_layers()` above the root layer).
- Reference consumers: the `Selector` widget's menu
  (`masonry/src/layers/selector_menu.rs`) and the `Tooltip` layer
  (`masonry/src/layers/tooltip.rs`).

But **xilem has zero integration with any of it**. No view creates a layer;
no element path can reach into one.

## 3. The xilem gaps this project proved

### 3.1 `WidgetMut` navigation is structural — cross-subtree elements are unreachable

xilem's rebuild/teardown/message all thread `WidgetMut`s structurally,
parent element → child element. A widget mounted in *another* subtree —
whether an `OverlayScope` slot reached via `mutate_later`, or a masonry
layer reached via `create_layer` — has no element path, so xilem can never
rebuild it, tear it down, or deliver messages (button callbacks) into it.
**Interactive stateful content in a layer is impossible today.** This single
fact forced the entire userspace portal: the only way to keep full xilem
semantics was to make popover content a real view child *of the scope's own
view* (`OverlayScopeRootView` in `src/overlay_scope.rs`), so its element
path runs through the scope.

### 3.2 `mutate_later` requires `Send`; widgets aren't

`mutate_later`'s closure is `impl FnOnce(WidgetMut<…>) + Send + 'static`
(`masonry_core/src/core/contexts.rs:1795-1800`). Widgets are not `Send`, so
a pre-built widget cannot be captured and pushed across the tree — only
plain data can. This is why the legacy `OverlayScope::set_overlay` path
(used by `dropdown_button`) is restricted to stateless content rebuilt from
data inside the closure, and why the portal's show/hide/placement protocol
(`OverlayScope::set_portal_visible` / `set_portal_placement`) is pure data.

### 3.3 The `Send + Sync` split: view values vs. resources/ViewState

`WidgetView` is `View<…> + Send + Sync`
(`xilem_masonry/src/widget_view.rs:13-17`), while the bare `View` trait has
no such bound. So view *values* must be `Send + Sync` (this is why
`AnyWidgetView` carries the bounds), but Environment resources
(`pub trait Resource: AnyDebug {}`, `xilem_core/src/environment.rs:139`) and
`ViewState` do not. The consequence pair we hit:

- Erased portal content must be
  `Arc<dyn AnyView<…> + Send + Sync>` (`PortalContentView` in
  `src/overlay_portal.rs`) because `PopoverView` carries it as a field and
  must itself satisfy `WidgetView`.
- The non-`Send` registry (`Rc<RefCell<…>>` inside `OverlayPortal`) must be
  constructed **inside** the `provides` closure rather than captured —
  capturing it would make the provider view value non-`Send`. See the
  comment block in `overlay_scope()` (`src/overlay_scope.rs`).

### 3.4 `provides` build-once semantics are load-bearing (and subtle)

`Provides::build` (`xilem_core/src/environment.rs:216`) calls the closure
once, stores the value in the provider's `ViewState`, and thereafter only
*swaps* it into the environment slot for the duration of every child
build/rebuild/message. Closure results from later view-value recreations are
**ignored**. That is exactly what gives the portal registry stable identity
for the scope's lifetime — and it is subtle enough that it deserves explicit
upstream documentation: a provider whose closure captures per-pass values
will silently keep publishing the first pass's value forever.

### 3.5 Pointer/focus acceptance flags are cached once at `WidgetAdded`

masonry caches `accepts_pointer_interaction`,
`propagates_pointer_interaction`, `accepts_focus`, and `accepts_text_input`
ONCE, right after delivering `Update::WidgetAdded` in the update-tree pass
(`masonry_core/src/passes/update.rs:191-194`). Dynamic returns are silently
ignored afterwards. Our first `PortalSlot` returned
`self.children.iter().any(|c| c.visible)` from
`accepts_pointer_interaction` and the dismiss test failed. The v1 fix was an
explicit invisible `Backdrop` child whose `accepts_pointer_interaction` was
statically `true` and whose *stashing* was the dynamic hit-test switch
(stashed widgets are skipped by hit-testing, §1). The backdrop has since
been removed entirely — it occluded the whole scope while open, eating
scroll/hover/clicks for everything beneath. Dismissal now observes
pointer-downs *bubbling* through `OverlayScope::on_pointer_event` (the scope
is an ancestor of everything inside it, so no occluding hit-target is
needed and the caching pitfall doesn't arise), deferring the decision to
`PortalSlot::dismiss_outside` via `mutate_child_later`. The caching lesson
stands for any widget that wants a *dynamic* hit-test area.

### 3.6 A keyed portal diff must iterate to a fixpoint

A hand-rolled keyed portal cannot single-pass diff its registry: an entry's
build/rebuild/teardown can register or deregister **nested** popovers
mid-diff (a popover inside another popover's content), and an owner's
rebuild can `update()` a later-keyed entry after the pass-start snapshot was
taken. Single-pass snapshot diffing therefore both misses mounts and
rebuilds from stale content. The fix (`OverlayScopeRootView::rebuild`,
`src/overlay_scope.rs`): loop until no progress, re-snapshot each iteration,
re-fetch each entry from the live registry at processing time, and count
**unmounts as fixpoint progress too** — a teardown can cascade
deregistrations of nested entries, and without the unmount arm counting as
progress the cascade stalls half-unmounted.

### 3.7 Compose-based re-anchoring is a userspace polling loop

Both `dropdown_button` (`src/components/dropdown_button/widget.rs`,
`compose`) and `PopoverHost` (`src/components/popover/widget.rs`, `compose`)
keep an open popup glued to its scrolling trigger via the same idiom:
compare the current window-space rect against the last pushed one in
`compose`, push the new placement if it moved, and re-arm via
`mutate_self_later(|w| w.ctx.request_compose())`. This relies on a fragile
contract: masonry runs a widget's `compose` when its transform changed *or*
compose was requested (`masonry_core/src/passes/compose.rs:28`,
`transformed || needs_compose`) — but a scrolled trigger's own transform
change doesn't propagate a fresh placement to the structurally-separate
slot, so the widget must keep re-requesting; the layout pass also re-arms
it (`masonry_core/src/passes/layout.rs:366-371`). It is, in effect, a
frame-by-frame polling loop that first-class *anchored* layers
(`create_attached_layer` + framework-maintained positioning) would
eliminate.

### 3.8 `mutate_later` against a removed `WidgetId` is safely skipped

The mutate pass drops callbacks whose target has been removed
(`masonry_core/src/passes/mutate.rs:69-78`, "Skip callbacks whose target was
removed since they were emitted"). This is load-bearing for any
cross-subtree owner-notification protocol: `PortalSlot::dismiss_outside`
dismisses by `mutate_later(owner, … PopoverHost::mark_closed …)`, and if the
owning popover was torn down in the same frame nothing explodes. Worth
guaranteeing (and documenting) upstream, because every userspace portal will
end up depending on it.

## 4. What the userspace portal had to build

The checklist a first-class `xilem` portal/layer view should subsume —
everything below exists in this repo only because the framework has no
primitive for it:

1. **Typed registry resource** — `OverlayPortal<State, Action>`
   (`src/overlay_portal.rs`): `Rc<RefCell<…>>` keyed registry of
   `Arc`-erased content views, published via `provides`.
2. **Keyed `ViewId` message routing inside a custom `View` impl** —
   `OverlayScopeRootView` (`src/overlay_scope.rs`) hand-routes
   build/rebuild/teardown/message per entry under
   `…scope path… / ViewId(key)`.
3. **Surface wrapping** — `wrap_in_surface` mirrors `PopoverHost::new`'s
   chrome so portal and in-tree popovers look identical.
4. **Outside-press dismissal** — `OverlayScope::on_pointer_event` observes
   every pointer-down bubbling through the scope and defers to
   `PortalSlot::dismiss_outside` (light dismiss with pass-through; see
   §3.5 for the occluding-backdrop design this replaced).
5. **Compose re-anchoring** — the polling idiom of §3.7, duplicated in two
   widgets.
6. **Stash-while-open cleanup** — `PopoverHost` handles
   `Update::StashedChanged(true)` (`src/components/popover/widget.rs`) to
   close its portal child when the trigger itself gets stashed (e.g.
   scrolled into a stashed region), since the content wouldn't be stashed
   with it.
7. **Fixpoint diffing** — §3.6.
8. **Owner-notification protocol** — `dismiss_outside` → `mutate_later` →
   `PopoverHost::mark_closed`, relying on §3.8.

## 5. v1 limitations that platform layers would fix

All intentional in the userspace version; all fall out for free with real
layers:

- **Outside-scope dismissal.** Dismissal observes pointer-downs bubbling
  through the scope; clicks beyond its bounds never pass through it and
  don't dismiss. `Layer::capture_pointer_event` sees every pointer event
  window-wide.
- **Down-consuming descendants block dismissal.** A widget that
  `set_handled`s the *down* half of a press stops it bubbling to the scope,
  leaving the popover open for that press (none of ours do — they consume
  ups and scrolls). `Layer::capture_pointer_event` runs before hit-testing,
  so it can't be shadowed.
- **A11y placement.** Portal content lands in the accessibility tree under
  the scope's slot, not its trigger. Layers have their own roots.
- **Window-edge overflow.** Portal content is clipped to the scope
  (`set_clip_path`); layers aren't clipped by anything in the tree and can
  extend to (or, someday, past) the window edge.
- **Tab-away dismissal asymmetry.** In-tree popovers close on
  `ChildFocusChanged(false)` (content is a descendant); portal popovers
  can't (content isn't), so Tab-away doesn't dismiss them
  (`src/components/popover/widget.rs` module docs).
- **Environment context preservation.** Our content mounts as a view child
  of the *scope*, so a `provides` published between the scope and the
  popover call site is invisible inside portal content. A portal *view*
  whose element lives elsewhere but whose build/rebuild runs at the call
  site could re-enter the environment stack at the right depth — scope-level
  mounting structurally cannot. (React portals preserve context for exactly
  this reason.)

## 6. Sketch of the upstream ask

Offered as a discussion-starter, not a decree: a `layer` / `portal` view in
`xilem_masonry` whose **element lives in a masonry `Layer`** (via
`create_layer` / `create_attached_layer`) but whose Mut-path navigation
knows how to jump the layer boundary — e.g. elements addressable by layer
root rather than only by structural parent — so that build/rebuild/teardown/
message and the Environment stack are preserved for content inside the
layer. Anchored-to-widget placement would be maintained by the framework
(the attached-layer machinery) instead of the compose polling loop of §3.7.
Such a view would subsume the entire checklist in §4, fix every limitation
in §5, and make `void_ui`'s `overlay_portal` deletable.
