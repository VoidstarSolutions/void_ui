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

# Frame profiling (see "Profiling" below). Always --release: a debug build's
# frame times are dominated by unoptimized paint/encode and tell you nothing.
cargo run --release -p void-ui --example gallery --features gallery,profiling
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
- `src/overlay/` holds the shared overlay vocabulary (`OverlayAnchor` placement, `OverlaySurface`/`SurfaceStyle` chrome) and the `PortalBinding` open/close plumbing that `popover`, `dropdown_button`, `autocomplete`, and `dialog` all build on.

Build new overlay-flavored components on these rather than reimplementing positioning/dismissal. Masonry's window-level `Layer` system is the intended long-term replacement once xilem grows view-layer integration for it.

### Profiling

The `profiling` cargo feature streams frame timings to [Tracy](https://github.com/wolfpld/tracy).
Use it instead of `cargo flamegraph` for interaction latency: Tracy attaches to a
running process and lets you select an arbitrary time range after the fact, so you
can isolate "the 800 ms after I clicked" rather than aggregating a whole run.

There is nothing to instrument by hand. `masonry_winit::app::run` calls
`masonry_core::app::try_init_tracing()`, which installs an unfiltered `TracyLayer`
when `masonry_core/tracy` is on. That gives, for free, a zone per pass —
`layout`, `paint`, `compose`, `update_anim`, `update_pointer`, `update_focus`, …
— from the `info_span!`s in `masonry_core/src/passes/`.

The one place this needs care: the gallery's own text-based frame tracer
(`VOID_UI_TRACE_FRAMES=1`, `examples/support/frame_trace.rs`) finalizes the
global `tracing` subscriber itself, before `try_init_tracing()` ever runs — and
`try_init_tracing()` is a no-op once a subscriber is set. Run both together
without accounting for that and Tracy silently never connects. The fix is in
`frame_trace.rs`'s `install_if_requested`: when the `profiling` feature is on,
it composes `tracing_tracy::TracyLayer` into its own registry (`Cargo.toml`'s
`profiling` feature pulls in `dep:tracing-tracy` for exactly this), so
`VOID_UI_TRACE_FRAMES=1 cargo run --release --example gallery --features
gallery,profiling` gets both the text trace and a live Tracy connection.

To make our own widget code show up as named zones alongside those, add
`info_span!`/`#[tracing::instrument]` in the relevant `widget.rs` — no extra
wiring needed, the layer picks it up.

**Do not switch this to `masonry_winit/tracy`.** That feature is the more complete
one — it adds a `non_continuous_frame!("Masonry")` marker per redraw plus GPU zones
via `vello/wgpu-profiler` — but it also makes `masonry_winit/src/vello_util.rs`
request `GpuProfiler::ALL_WGPU_TIMER_FEATURES` on the device, and on macOS/Metal
that hangs the app before the window ever appears: the unconditional
`device.poll(wgpu::PollType::wait_indefinitely())` in `present_surface`
(`event_loop_runner.rs:802`) never returns on the first frame, with 100% of
main-thread samples parked in `Device::poll`. `masonry/tracy` avoids the
wgpu-profiler path entirely and comes up fine. Counting frames is unaffected: the
`paint` span fires once per redraw, so `paint` zones *are* frames. Worth an
upstream issue asking for masonry_winit's `tracy` feature to be splittable into
CPU and GPU halves.

Version coupling: `tracing-tracy` 0.11 speaks the Tracy 0.11.x protocol, so the
Tracy GUI must be a 0.11.x release or it will refuse the connection.

**What to look for.** Neither masonry nor vello does damage tracking, so per-frame
main-thread cost is proportional to the *total* painted content of the window, not
to what changed:

1. `masonry_core/src/passes/paint.rs` copies every widget's cached scene fragment
   into one flat `Scene` every frame (`append_transformed`, unconditional — the
   per-widget cache only skips re-running `Widget::paint`). It has a
   `// TODO - Handle damage regions` ([xilem#789]) and a `// TODO: We could skip
   painting children outside the parent clip path`, so scrolled-off content is
   included too.
2. `imaging_vello::encode_source` allocates a fresh `vello::Scene` and re-encodes
   that whole thing, after a full `validate()` walk.
3. `vello::Renderer::render_to_texture` resolves the entire encoding on the CPU
   again before submitting.

So a profile dominated by `render_to_texture` is expected, and the useful question
is always *frame count* vs *per-frame cost*. Check the Tracy frame markers first:
a steady stream of frames while the UI is visually idle means an animator is
re-arming. Widgets that call `ctx.request_anim_frame()` unconditionally in
`on_anim_frame` (spinner, skeleton) hold the whole window at refresh rate for as
long as they exist anywhere in the tree — masonry's anim pass has no `is_stashed`
check, so even a hidden one keeps ticking.

[xilem#789]: https://github.com/linebender/xilem/issues/789

### Linebender dep tracking

`masonry`, `xilem`, and `xilem_masonry` track the `main` branch of our fork <https://github.com/VoidstarSolutions/xilem> in the workspace `Cargo.toml` — the fork follows linebender upstream but lets us carry local patches ahead of upstream merge, and the lockfile is the only thing holding the resolved revision steady between builds. Per the comment in `Cargo.toml`: **do not depend on `peniko`/`kurbo`/`parley`/`vello`/`imaging` directly** — masonry and xilem re-export them, and pulling them in standalone causes diamond-dep version skew. Also do not disable masonry's default features without a deliberate re-enable: the default-feature chain (`masonry/default → masonry_winit/imaging_vello → masonry_imaging/imaging_vello`) is what selects the Vello backend at compile time, and disabling it panics at startup with `backend=""`.

Because we follow the fork's `main`, a change landing there can break our build the next time `cargo update` runs. When that happens, migrate the consumer code rather than pinning to an older rev — the goal is to stay current. Always bump all three crates together (`cargo update -p masonry -p xilem -p xilem_masonry`) so they stay on a single commit.

### Gallery and `with_source!`

The `gallery` example is the canonical visual test surface. Component demos use the `with_source!` macro (see `src/gallery.rs` / `src/components/*/demo.rs`) to display rendered output alongside its source. New components should ship a demo panel before the API is considered final.
