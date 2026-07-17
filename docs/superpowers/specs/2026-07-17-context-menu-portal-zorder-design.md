# Context menu z-order: host the menu in the overlay portal

## Context

Issue #77. The context menu (#41) is hosted in-tree — the menu is a
descendant of `ContextMenuArea`, painted via normal child registration
order — so it paints in tree order and can be occluded by later siblings
(e.g. a panel rendered after the area). For a right-click menu, painting
above everything is table-stakes, so this is the component's main open
limitation.

#41's own spike found this wasn't plumbing: decoupling the menu into the
portal breaks close-on-select and Escape, because the menu's single
`MenuAction` bubbles to the generic scope, not the trigger, and a masonry
widget can only emit its one declared action type — it can't also emit a
generic "dismiss me" the scope understands. The spike was reverted rather
than risk the working menu, and #77 was filed to track porting once the
blocking machinery landed.

That machinery has since landed, built by #48 (dialog) and #44
(notification): `PortalPlacement::BareTrigger` (`overlay_portal.rs:126`,
anchored like a popover but unwrapped — no `OverlaySurface` chrome, since
menu-shaped content already paints its own) and the generic
`PortalOwner`/`DismissHook` dismiss-routing (`overlay_portal.rs:347-358`,
`overlay/binding.rs`'s `PortalBinding`). `dropdown_button`, `autocomplete`,
and `date_picker` all already register `BareTrigger` content and drive it
through `PortalBinding`/`DismissHook` — confirmed by reading
`dropdown_button/view.rs` and `dropdown_button/widget.rs`'s `Hosting`
enum, `PortalBinding`, and `dropdown_dismiss_hook`. So the open question
#77 posed to the team — whether `PortalPlacement` needs a new variant for
"anchored + bare" — is already answered: `BareTrigger` *is* that
combination. What's left is porting `context_menu_area` onto the same
proven pattern, plus two gaps porting exposes that the existing consumers
don't hit:

1. **No viewport-edge clamping anywhere in `PortalSlot::layout`.** The
   `Trigger | BareTrigger` branch (`overlay_portal.rs:654-688`) computes
   `child.anchor.child_offset(...)` with no bound against the slot's own
   `size` (the scope's box, effectively the window). Today's in-tree
   `ContextMenuArea::clamp` (`area.rs:92-104`) does exactly this
   clamping locally; porting to the portal without it would regress menus
   opened near the screen edge to silently overflow the window.
2. **`PortalBinding` only knows how to anchor to a host widget's own
   border box**, re-derived every frame for scroll-tracking
   (`binding.rs:159-167`, `249-263`). A context menu anchors to the
   *cursor point* captured at click time, not the host widget's box, and
   that point is fixed in window space — it never needs re-deriving, so
   the existing reanchor loop is both the wrong shape and unnecessary
   plumbing to reuse as-is.

Both were resolved during design (see Decisions below) rather than left
open.

## Goals

- `context_menu_area`, when a `overlay_scope` ancestor exists, mounts its
  `MenuPanel` in the scope's `PortalSlot` instead of as a widget
  descendant, so it always paints above every other in-scope content
  regardless of sibling registration order — the actual bug #77 exists to
  fix.
- No-scope-ancestor fallback (today's in-tree behavior) is unchanged and
  stays the only path when no `overlay_scope` wraps the area.
- Close-on-select, Escape, Tab-dismiss, and outside-click dismiss all
  continue to work identically to today's in-tree behavior, from the
  user's perspective, in portal mode.
- Submenus (`item_node.rs` fly-outs) are unaffected — verified
  self-contained (positioned entirely within `MenuPanel`'s own layout),
  so relocating `MenuPanel` doesn't change their behavior.
- `PortalSlot` gains generic viewport-edge clamping for all
  `Trigger`/`BareTrigger` placements (not just context menu) — see
  Decision 1.

## Non-goals

- No dismiss-on-scroll for an open context menu. It keeps a static
  window-space position if the user scrolls content underneath it — see
  Decision 2. Native-feeling scroll-tracking is not part of this spec.
- No change to `popover`, `dropdown_button`, `autocomplete`, or
  `date_picker`'s own anchoring/reanchor behavior — the new
  `PortalBinding::open_at_point` entry point is additive, and the
  viewport clamp is a no-op for any placement that already fits on
  screen (see Decision 1).
- No change to `MenuPanel`/`item_node.rs`'s row model, submenu fly-out
  positioning, or keyboard-nav semantics — only *where* `MenuPanel` is
  mounted changes, not how it behaves internally.

## Decisions

**1. Viewport clamping: generic, in `PortalSlot::layout`, for all
`Trigger`/`BareTrigger` placements — not scoped to context menu only.**
Add, after the existing offset computation (`overlay_portal.rs:675-686`):

```rust
let offset = Point::new(
    offset.x.clamp(0.0, (size.width - child_size.width).max(0.0)),
    offset.y.clamp(0.0, (size.height - child_size.height).max(0.0)),
);
```

This is the same shift-back-from-the-far-edge math `ContextMenuArea::clamp`
already does locally, applied once at the shared layout site. It's a no-op
for any placement that already fits inside the slot's box, and only
changes behavior for content that would have silently overflowed the
window edge — true today for popover/dropdown_button/autocomplete/
date_picker too, none of which currently clamp. Chosen over a
context-menu-only clamp because the overflow gap is real and latent in
every `BareTrigger`/`Trigger` consumer, and fixing it once in the shared
layout path is less code than reimplementing clamp math per component.

**2. Cursor anchoring: new `PortalBinding::open_at_point`, no reanchor
loop.** Add to `overlay/binding.rs`:

```rust
pub(crate) fn open_at_point(&mut self, ctx: &mut impl PortalCtx, window_point: Point) {
    self.push_visible_rect(
        ctx,
        Rect::from_origin_size(window_point, Size::ZERO),
        OverlayAnchor::BottomStart,
        0.0,
    );
    // Deliberately does not call arm_reanchor_loop: the cursor point is
    // captured once in window coordinates and never needs re-deriving.
}
```

(`push_visible` is generalized into a `push_visible_rect` taking an
explicit `Rect` rather than always deriving one from
`ctx.host_anchor_rect_window()`; `push_visible` becomes a thin wrapper
that computes that rect and calls it, so `open`/`refresh`'s existing
callers are unaffected.)

`OverlayAnchor::BottomStart`'s `child_offset` on a zero-size trigger rect
resolves to `Point::new(0.0, 0.0)` (`anchor.rs:55`) — i.e. the menu's
top-left lands exactly on the cursor point, pre-clamp, matching today's
`ContextMenuArea::clamp` placement (`cursor.x, cursor.y` before shifting
back from an overflowing edge). The zero-size rect also correctly serves
`PortalSlot::dismiss_outside`'s trigger-rect exclusion
(`overlay_portal.rs:579-598`) — `Rect::ZERO`-sized `.contains(pos)` is
practically never true, so unlike a trigger-button's real box, no click
position is spuriously excluded from dismissal.

Rejected alternative: reusing the existing reanchor loop by wiring
`ctx.host_anchor_rect_window()` (the *area widget's* full border box, not
the cursor point) — would recompute the wrong rect on every frame once
armed, snapping the menu to the area's top-left on the first
reanchor tick. Confirmed via `arm_reanchor_loop`'s
`anchor.has_trigger()` gate (`binding.rs:200-204`), which only checks
the anchor variant, not the rect's provenance — `BottomStart` would pass
that gate and arm incorrectly, so `open_at_point` must skip
`arm_reanchor_loop` unconditionally rather than relying on that gate.

Rejected: dismiss-on-scroll to compensate for the static position.
No existing portal-hosted component does this today; it's new plumbing
this issue didn't ask for. A context menu staying put at its opened
screen location while the user scrolls underneath is normal, low-cost
behavior (dialog's `ViewportQuarter` anchor already forgoes reanchoring
entirely, for the same reason: nothing under it needs live tracking).

## Design

### `ContextMenuArea` (`area.rs`)

Replace the single `menu: WidgetPod<MenuPanel>` field with a `Hosting`
enum, mirroring `dropdown_button::widget::Hosting`:

```rust
enum Hosting {
    InTree { menu: WidgetPod<MenuPanel> },   // today's behavior, unchanged
    Portal { binding: PortalBinding },
}
```

`content: WidgetPod<dyn Widget>` and `open`/`cursor` stay top-level
fields (unlike `dropdown_button`, there's no separate trigger widget to
fold into `Hosting` — the whole wrapped `content` is the right-click
target in both modes).

`open_at` (shared by the right-click handler and the `ShowContextMenu`
a11y action, `area.rs:82-88`) dispatches on `hosting`: `InTree` keeps
today's direct field mutation; `Portal` additionally calls
`binding.open_at_point(ctx, ctx.to_window(cursor))`.

`on_text_event`'s key forwarding (`area.rs:154-184`) gains a `Portal`
arm mirroring `dropdown_button::ThemedDropdownButton::set_highlight`'s
Portal branch (`widget.rs:553-568`) exactly: `mutate_later` into the
scope, `OverlayScope::portal_slot_mut`, `PortalSlot::child_mut(key)`,
downcast through `Passthrough` (the generic `PortalContentView` erasure
box every registered entry is wrapped in — confirmed via
`overlay_portal.rs:98-99`'s `PortalContentView` type alias, unrelated to
placement) to `MenuPanel`, then call `MenuPanel::handle_menu_key`
exactly as today.

New `context_menu_dismiss_hook` (parallel to `dropdown_dismiss_hook`,
`dropdown_button/widget.rs:505-508`), registered via
`PortalBinding::new(scope, key, context_menu_dismiss_hook)`: downcasts to
`ContextMenuArea` and clears `open`, invoked when the scope's
`dismiss_outside` fires for an outside press.

`Update::FocusChanged(false)` handling (`area.rs:214-229`) is unchanged
in both modes — the area still requests and holds real focus itself on
open, in `Portal` mode exactly as in `InTree` mode, so this path keeps
working identically. Porting adds `dismiss_outside` as a second,
independent dismiss path (the same mechanism popover/dropdown_button/
autocomplete already rely on for outside-press dismissal) — the two
coexist safely since both just clear `open` idempotently. This is a
likely correctness improvement over today's focus-loss-only dismissal
for clicks on non-focus-stealing background content, though not the
primary goal of this spec.

### View (`view.rs`)

`ContextMenuAreaView::build` (`view.rs:481-491`) calls
`portal_from_env::<State, Action>(ctx)`; when `Some`, registers a new
`ContextMenuContentView` (parallel to `dropdown_button::view::
MenuContentView`, `view.rs:364-440`) wrapping
`MenuPanel::new(self.rows.iter().cloned(), &self.theme).hosted()` with
`PortalPlacement::BareTrigger` and `SurfaceStyle::Popover` (matching
`dropdown_button`'s and `autocomplete`'s registration calls), and builds
`ContextMenuArea::new_portal(content_pod, scope, key)`. When `None`,
today's `ContextMenuArea::new(content_pod, menu_pod)` path is unchanged.

`ContextMenuContentView::message` handles both variants of `MenuAction`
(the standalone `menu()`'s `MenuView::message`, `view.rs:367-382`,
already shows this exact dispatch shape for the non-portal case):
- `MenuAction::Selected(index)`: `mutate_later` into the area (via a new
  `ContextMenuHandle`, `widget_id_handle!`-based like
  `DropdownButtonHandle`) to clear `open`, then `dispatch_selection`.
- `MenuAction::Dismissed` (Escape/Tab forwarded into the slot-mounted
  panel): `mutate_later` close only, `MessageResult::Nop` — mirrors
  `MenuView::message`'s existing `MenuAction::Dismissed` arm.

`rebuild`/`teardown` follow `DropdownButtonView`'s existing Portal-arm
shape (`view.rs:290-327`): re-`portal.update(...)` when rows/theme
change, `portal.deregister(key)` on teardown.

### Submenus

No changes. `item_node.rs`'s fly-out panels
(`submenu: Option<WidgetPod<MenuPanel>>`, laid out and placed entirely
within the owning row's own `layout`, `item_node.rs:540-548`) have no
dependency on where the top-level `MenuPanel` itself is mounted —
confirmed by reading their layout code, which only references the row's
own bounds, never the window or an ancestor outside `MenuPanel`.

## Testing plan

- Portal-mode regression test for the actual bug: build inside
  `overlay_scope` with a sibling rendered *after* the context-menu area,
  right-click to open, and assert the menu's paint/hit-test order places
  it above that later sibling (today's in-tree version would fail this;
  it's the reason #77 exists).
- Portal-mode versions of the existing `area.rs` tests
  (`right_click_opens_focuses_menu_and_keyboard_selects`,
  `show_context_menu_access_action_opens_the_menu`,
  `tab_dismisses_and_closes_the_area`,
  `area_advertises_has_popup_menu_and_expanded_state`) run inside an
  `overlay_scope`, asserting identical externally-observable behavior to
  the existing no-scope versions.
- Cursor-point positioning: right-click near each screen edge/corner and
  assert the clamp keeps the menu fully on-screen (parallel to the
  existing `clamp` unit tests, now exercised through
  `PortalSlot::layout` end-to-end rather than the standalone `clamp` fn).
- Outside-click dismiss via the scope's `dismiss_outside` path (distinct
  from the existing `FocusChanged(false)` coverage) — click elsewhere in
  the scope, not on the menu or a focusable widget, and assert the menu
  closes.
- Existing no-scope `InTree`-mode tests are kept unchanged as the
  fallback-path regression suite.
- `PortalSlot` clamp is also unit-testable directly (mirroring
  `overlay/anchor.rs`'s existing `child_offset` test style): a
  `BareTrigger` child whose computed offset would overflow the slot's
  `size` ends up clamped to the far edge, for each anchor variant already
  covered by `OverlayAnchor`'s own tests.

## Open questions

None. The two gaps porting exposes (viewport clamping, cursor vs.
trigger-rect anchoring) were resolved during design — see Decisions.
