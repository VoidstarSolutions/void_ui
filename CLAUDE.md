# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## void_ui (`void-ui`)

A general-purpose xilem/Masonry component library. Theme-driven, product-agnostic.

- **Presentation only.** No business/domain logic, no network, no persistence. Components receive state as input and emit events; they do not reach for ambient app state. If a change would couple a component to a specific product's data model, push it back to the consumer.
- **Reusable and product-agnostic.** If a primitive only makes sense in one downstream app, it belongs in that app, not here.

## Commands

```sh
# Build the library
cargo build

# Run the live component gallery (exercises every shipped component end-to-end).
# The `gallery` cargo feature gates the demo panels + with_source! proc-macro
# so library consumers don't compile them; the example requires it.
cargo run -p void-ui --example gallery --features gallery

# Lints (workspace denies clippy::pedantic — fix don't allow)
cargo clippy --all-targets --all-features

# Tests
cargo test --all-features                   # all (incl. gallery-gated demo tests)
cargo test --lib <name>                     # a single test by substring
```

## Architecture

### Two-layer component pattern

Each component ships **both** a xilem View and a masonry Widget. The convention is one folder per component under `src/components/<name>/` containing:

- `view.rs` — builder + xilem `View` impl. Coordinates state and rebuild logic.
- `widget.rs` — masonry `Widget` impl. Owns paint, layout, event handling.
- `demo.rs` — gallery panel exercising the component.
- `mod.rs` — re-exports.

When adding a component, follow this split. Don't merge view + widget into a single file.

### Theme propagation

`Theme` is **not** global state. It is passed in when materializing a view via `.render(&theme)`, copied into the widget at build time, and re-applied on rebuild only when the value differs. Theme swapping is a single state change at the host application root, not a tree walk. Components read colors/sizes/type from the `Theme` they were rendered with — never reach for ambient state.

Theme primitives live in `src/theme/`: `Palette` (colors), `Typography` (font stack), `Density` (sizes/spacing), `Radii`, and `ThemeVariant`.

### Overlay primitive

Anything overlay-shaped (popover, dropdown, menu, tooltip, dialog, toast) builds on two pieces:

- `floating::FloatingOverlay` (`src/floating.rs`) for cursor-anchored, pointer-inert chrome positioned inside a pre-sized container.
- `overlay_scope` (`src/overlay_scope.rs`) + `overlay_portal` (`src/overlay_portal.rs`) for popups that must paint above everything inside a region: the scope publishes a typed `OverlayPortal<State, Action>` Environment resource; components register erased content views into it; the scope's view mounts them in an always-last-painted, scope-clipped `PortalSlot`. Open/close/placement are plain-data widget mutations (`mutate_later`). `popover`, `dropdown_button`, and `autocomplete` use this when a scope ancestor exists (each falling back to in-tree `AnchoredOverlay` otherwise); `dialog` and `notification_layer` require a scope ancestor outright (no fallback — `dialog` always targets the outermost scope via `root_portal`). Host apps should wrap their root (or each independent region) in `overlay_scope`.

Build new overlay-flavored components on these rather than reimplementing positioning/dismissal. Masonry's window-level `Layer` system is the intended long-term replacement once xilem grows view-layer integration for it.

### Linebender dep tracking

`masonry`, `xilem`, and `xilem_masonry` track the `main` branch of <https://github.com/linebender/xilem> in the workspace `Cargo.toml` — we intentionally follow upstream rather than pin a rev, and the lockfile is the only thing holding the resolved revision steady between builds. Per the comment in `Cargo.toml`: **do not depend on `peniko`/`kurbo`/`parley`/`vello`/`imaging` directly** — masonry and xilem re-export them, and pulling them in standalone causes diamond-dep version skew. Also do not disable masonry's default features without a deliberate re-enable: the default-feature chain (`masonry/default → masonry_winit/imaging_vello → masonry_imaging/imaging_vello`) is what selects the Vello backend at compile time, and disabling it panics at startup with `backend=""`.

Because we follow `main`, an upstream API change can break our build the next time `cargo update` runs. When that happens, migrate the consumer code rather than pinning to an older rev — the goal is to stay current. Always bump all three crates together (`cargo update -p masonry -p xilem -p xilem_masonry`) so they stay on a single upstream commit.

### Gallery and `with_source!`

The `gallery` example is the canonical visual test surface. Component demos use the `with_source!` macro (see `src/gallery.rs` / `src/components/*/demo.rs`) to display rendered output alongside its source. New components should ship a demo panel before the API is considered final.
