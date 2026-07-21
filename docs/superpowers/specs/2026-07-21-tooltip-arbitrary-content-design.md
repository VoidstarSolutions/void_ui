# Tooltip: arbitrary content, off masonry's window Layer (issue: none yet filed)

## Problem

`tooltip`'s popup content is a closed `TooltipContent` enum (`Text(ArcStr)` or
`Rows(Vec<TooltipRow>)`, where a row is hard-coded to a colored dot + label),
built directly as raw masonry widgets inside `TooltipHost::build_layer`. There
is no way for a caller to put arbitrary content — an icon, a multi-line
composition, anything not text-or-dot-legend — into a tooltip, unlike every
other void_ui component, which takes a real child `View`.

This shape isn't an oversight: `TooltipHost` pops its content via masonry's
window-level `Layer` (`ctx.create_layer`), which only accepts a freshly-built
masonry `Widget`, not a live xilem `View`. Layer-hosted widgets are not
reachable by `View::message` (confirmed during the popover z-order
investigation — see `project_popover_zorder` memory), so a real interactive
View placed there would silently stop receiving rebuilds/actions. The
`TooltipContent` enum exists specifically to route around that limitation by
never needing View-level rebuild/message routing for the popup.

The `Rows` variant (`tooltip_rows`, `TooltipRow`) was built speculatively and
is not exercised anywhere — not the gallery demo, not any other call site in
this crate.

## Decision

Migrate `tooltip` off masonry's `Layer` onto the `overlay_scope` /
`overlay_portal` mechanism `popover`, `dropdown_button`, and `dialog` already
use. That mechanism supports hosting arbitrary `View` content with full
build/rebuild/message semantics today — it exists precisely because `Layer`
can't. This isn't a new primitive; it's applying an existing, tested one to
the one remaining overlay-shaped component still on the old path.

Like `dialog`, `tooltip` will **require** an `overlay_scope` ancestor (no
in-tree fallback) — mirroring `dialog`'s `root_portal_lookup(...).or_panic(...)`
pattern. A tooltip has no trigger rect it *needs* to stay confined to in the
z-order sense the in-tree `AnchoredOverlay` fallback exists for (unlike
popover/dropdown/autocomplete's fallback, which trades scope-independence for
possible occlusion by later siblings) — for tooltip, "no scope" should just be
a clear build-time panic, the same tradeoff dialog already made.

## API surface

```rust
pub fn tooltip<ChildV, ContentV>(content: ContentV, child: ChildV) -> Tooltip<ChildV, ContentV>
where
    ContentV: WidgetView<State, Action>,
    ChildV: WidgetView<State, Action>;
```

One builder. `content` is the popup content, composed the normal void_ui way:

```rust
tooltip(
    label("Reset the chart to defaults").render(theme),
    button(...).render(theme),
)
.render(theme)
```

A legend/rows-style tooltip is just a richer `content` view built the same
way every other composition in this crate is:

```rust
tooltip(
    flex_col((
        flex_row((status_dot(Color::GREEN).render(theme), label("Ready").render(theme))),
        flex_row((status_dot(Color::RED).render(theme), label("Error").render(theme))),
    )).render(theme),
    icon(IconName::Info).render(theme),
)
```

**Deleted:** `TooltipContent`, `TooltipRow`, `tooltip_rows`. `.delay(...)`
stays unchanged.

Because content registers into the scope's typed `OverlayPortal<State,
Action>` (same as dialog), its `State`/`Action` must match the root
`overlay_scope`'s exactly — an existing constraint, not a new one introduced
here.

## Widget-side behavior

`TooltipHost` keeps its current shape almost entirely: it still wraps `child`
inline in the tree (measure/layout/paint delegate to it, unchanged), still
tracks `delay`, `last_pointer_move`, and a cursor/anchor point, and still runs
the same hover-idle / keyboard-focus-idle state machine
(`on_pointer_event`/`on_anim_frame`/`update`). Only *how it shows/hides the
popup* changes:

- **Show** (idle threshold elapsed, from either the hover or the
  keyboard-focus arming path): `binding.open_at_point(ctx, last_cursor_pos_window
  + CURSOR_OFFSET)`. `PortalBinding::open_at_point` already exists for
  exactly this shape (built for `context_menu`'s cursor-anchored popup) — no
  new binding API needed. Both the hover path and the keyboard-focus path
  already converge on "an anchor point in window coordinates" today (focus
  explicitly computes the child's bottom-left corner via `ctx.to_window`), so
  one call site covers both, same as today.
- **Hide**: `binding.close(ctx)`, called from the same three places that
  clear state today — a pointer `Move` while currently visible,
  `HoveredChanged(false)` / `ChildHoveredChanged(false)`, and
  `ChildFocusChanged(false)`.
- **Click-elsewhere dismissal**: free from the portal's existing
  `dismiss_outside` + owner/dismiss-hook mechanism (the same one `popover`
  uses for `mark_closed`) — `open_at_point` already registers an owner, no
  new mechanism needed.
- A `visible: bool` field replaces `layer_id: Option<WidgetId>` as the local
  "currently showing" bookkeeping, with a `tooltip_dismiss_hook` (mirroring
  `popover_dismiss_hook`) that flips it back via `mutate_later` when an
  outside press dismisses it.

`build_layer`, `build_text_surface`, `build_rows_surface`, and `description()`
are deleted along with the `content`/`layer_id` fields.

## Chrome

Content is wrapped in the shared `OverlaySurface` (the same chrome primitive
`popover`/`dialog` use) via a new `SurfaceStyle::Tooltip` — sharp corners
(`corner_radius = 0.0`, matching today's unrounded look; today's tooltip
applies no `CornerRadius` property at all).

**Known visual change:** `OverlaySurface` always pads with `theme.density.pad`,
whereas today's tooltip uses a fixed 6px regardless of density. Adopting the
shared surface makes tooltip padding density-scaled like every other overlay
surface, rather than staying fixed-small. This is an intentional consistency
tradeoff, not an oversight.

## Accessibility

No separate accessible-description argument. The popup content's own
accessibility nodes (e.g. a `Label`'s text) are what a screen reader sees —
same as any other portal-mounted content. This means the description is
structurally detached from the trigger in the accessibility tree (portal
content mounts under the scope's slot, not under the trigger) — a known,
already-documented limitation of the portal mechanism (see
`overlay_portal.rs`'s "Known v1 limitations"), now inherited by tooltip too.

## Testing impact

- `TooltipHost`'s widget tests (currently asserting on `layer_id`) get
  rewritten against `visible` / `PortalBinding`, following the same
  scope-backed harness pattern `dialog`'s and `popover`'s widget tests use.
- Tests exercising the `Rows` variant and its accessibility-description
  flattening are deleted along with the variant.

## Migration impact

- No in-repo call sites besides the gallery demo. `src/components/tooltip/demo.rs`'s
  five `tooltip("...", child)` calls become `tooltip(label("...").render(theme),
  child)`. The gallery already wraps its root in `overlay_scope`
  (`examples/gallery.rs`), so no gallery-level scope change is needed.
- Breaking change for any downstream consumer of `tooltip`/`tooltip_rows`/
  `TooltipContent`/`TooltipRow` — consistent with this crate's existing
  pre-1.0 stance on breaking changes without compatibility shims (see the
  export-surface-uniformity PR, issue #164).
