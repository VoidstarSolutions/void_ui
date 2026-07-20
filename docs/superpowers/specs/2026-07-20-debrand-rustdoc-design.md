# De-brand public rustdoc (issue #160)

## Problem

`CLAUDE.md` declares void_ui "Theme-driven, product-agnostic" and "Reusable and
product-agnostic." In practice, 26 doc comments across the crate name a
specific downstream product ("Tessera") and quote its CSS surface directly
(`.tb-btn`, `.tb-btn.active`, `data-density="balanced"`, `data-theme="dark"`).
Since this repo's rustdoc is public, these comments are the crate's public
face and directly contradict the product-agnostic claim — a Tessera CSS class
name tells an external reader nothing, because Tessera is never defined
anywhere in this crate.

A related, smaller issue in the same audit: `status_dot`'s
`const DEFAULT_SIZE: f32 = 8.0` (`src/components/status_dot/view.rs:27`) is a
free-standing magic number whose own doc comment names a second product
("citadel-ui") as the source of the value, instead of deriving from
`theme.density` the way the rest of the library's component sizes do.

## Scope

**In scope:**
1. Remove all "Tessera" references and product-specific CSS syntax from doc
   comments in the 9 affected files, replacing them with descriptions of what
   each item *is* in this crate's own vocabulary.
2. Replace `status_dot::DEFAULT_SIZE`'s hardcoded, product-attributed constant
   with a value derived from `theme.density.control`, following the same
   density-token pattern `radio` and `slider` already use for their glyph
   diameters.

**Out of scope:**
- Renaming any Rust identifiers (structs, fields, enum variants, functions).
  This is a doc-comment and one-constant fix, not an API change.
- The `citadel::ChartWindow` string literal in `src/overlay_scope.rs` test
  code — that's a fictional example type name used to exercise message
  routing, not a real product-branding leak, and isn't part of the issue's
  file list.
- Any other magic-number/hardcoded-value audit beyond `status_dot` — issue
  #160 only flags this one instance.

## Design

### 1. Doc comment de-branding (find-and-replace, no behavior change)

Files: `src/components/mod.rs`, `src/components/button/{mod,view,widget}.rs`,
`src/components/sidebar/{view,widget}.rs`, `src/theme/{mod,palette,density,
typography,color}.rs`, `examples/gallery.rs`.

Rules applied per occurrence:
- Drop "Tessera" as a proper noun; describe the component/token by its role
  (e.g. "the default button style" not "the default Tessera `.tb-btn`
  style").
- Drop CSS-attribute-flavored syntax that only makes sense in the source
  product's markup (`.tb-btn`, `.tb-btn.active`, `data-density="balanced"`,
  `data-theme="dark"`) — replace with this crate's own Rust-facing vocabulary
  (the actual enum variant, field, or method name being documented).
- Where a comment cites a concrete pixel value or ratio as originating from
  Tessera (e.g. typography sizes, radii, OKLCH color values), keep the
  factual technical content (the number, the color space) and drop only the
  product attribution — the numbers themselves aren't the branding leak.
- `examples/gallery.rs:126` subtitle `"Tessera-styled widget library"` →
  `"A themeable Masonry/xilem widget library"`.

No functional code changes in this part — pure doc-comment text edits.

### 2. `status_dot` default size

Replace:
```rust
/// Default diameter in px, matching the size `citadel-ui`'s hand-rolled
/// version used at every call site.
const DEFAULT_SIZE: f32 = 8.0;
```

with a diameter derived from `theme.density.control` (the token whose own
doc comment already claims this role: "radio diameter and slider thumb
diameter read it directly; derived marks scale from it"). The ratio is
chosen so **Balanced density reproduces today's 8px value exactly**
(`14.0 * 4.0 / 7.0 == 8.0`), consistent with how every other token in
`density.rs` was calibrated to match its pre-token hardcoded constant
pixel-for-pixel at Balanced (see `balanced_tokens_match_pre_token_constants`
in `density.rs`'s test module). At Compact this yields ~6.86px, at Airy
~9.14px.

This requires `StatusDot::render` to compute its fallback from `theme`
instead of a bare constant:
```rust
let default_size = theme.density.control * 4.0 / 7.0;
let size_px = f64::from(self.size.unwrap_or(default_size));
```
`DEFAULT_SIZE` as a named constant goes away since it's no longer
density-independent; the ratio is documented inline instead.

Consequence: `status_dot(...).render(theme)` with no explicit `.size()` now
varies slightly with density instead of being flat 8px at every step — a
small behavior change, but visually identical at the default (Balanced)
density, so no existing snapshot/visual output changes unless a caller has
switched density away from Balanced.

**Demo update:** `src/components/status_dot/demo.rs:91` labels a section
`"Default (8px)"`. Since the default is now density-dependent, reword to
`"Default (density-driven)"` so the label doesn't go stale at Compact/Airy.

### Rejected alternatives (status_dot)

- **New dedicated `Density::dot` field**, hand-tuned per step (e.g. compact
  6 / balanced 8 / airy 10). Gives cleaner round numbers, but works against
  `control`'s own docstring, which already claims the "glyph-like indicator"
  role — adding a second field for the same conceptual purpose duplicates
  intent instead of reusing it.
- **Comment-only de-branding, keep the flat constant.** Satisfies the
  literal branding complaint but not the issue's explicit ask that the value
  "come from `theme.density`" instead of being a free-standing number.

## Testing

- `cargo doc --no-deps` (or `cargo doc --all-features`) and spot-check
  rendered output for the touched files to confirm no leftover "Tessera"/
  `.tb-btn`/`data-*` text and no broken intra-doc links from the rewording.
- `cargo test --all-features` — existing `density.rs` tests are unaffected
  (no `Density` struct fields change); add/confirm a `status_dot` test (or
  extend an existing one) asserting the Balanced-density default size is
  still `8.0`, to lock in the pixel-for-pixel parity claim.
- `grep -rni "tessera\|tb-btn\|citadel-ui" --include="*.rs" .` returns no
  matches when done.
- Run the gallery (`cargo run -p void-ui --example gallery --features
  gallery`) and visually check the Status Dot panel's default section still
  renders the same size, and the gallery subtitle no longer says "Tessera."
