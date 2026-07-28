# Input latency investigation — 2026-07-27

Reported symptom: clicking interactive elements in the gallery (sidebar items,
button-group options) takes up to ~1s to visibly land, and hover feedback
freezes while it happens. An Instruments run showed ~65% of main-thread time in
vello's `render_to_texture`.

**Outcome: the reported latency is not a void-ui bug.** It reproduces in stock
xilem with none of our code, and its root cause is that the process stops
receiving mouse events for 250–780ms at a time while the pointer is moving.

Two genuine void-ui performance bugs were found and fixed along the way. They
were real, but they were not the reported problem.

---

## 1. Root cause of the reported latency

**The app receives no `CursorMoved` events for 250–780ms while the mouse is
moving.** Positions are perfectly current when delivery resumes, so this is a
delivery *gap*, not staleness or a backlog.

Measured by patching `masonry_winit::MasonryState::handle_window_event` to log,
for every `CursorMoved`, both the position carried by the event and the live OS
cursor position (`NSEvent::mouseLocation()`) at that instant. Only `CursorMoved`
was instrumented this way — other `WindowEvent` variants (button presses, etc.)
were not logged or measured, so nothing here establishes whether they were also
delayed or delivered normally during these gaps.

| Measurement | Value |
| --- | --- |
| Event position vs live cursor | median divergence **0.0px**, p90 0.6px |
| Cross-correlation lag of the two series | **0ms** (corr 0.947) |
| `CursorMoved` delivery intervals | p50 17ms, p90 34ms, **p99 317ms, max 784ms** |
| Live cursor displacement during those stalls | up to **356px** |
| App activity during a stall | zero `CursorMoved` events, zero redraws |

The displacement figure is the important one: the pointer moved hundreds of
pixels during windows in which the process was handed nothing. The mouse was
demonstrably in motion; the events simply did not arrive.

### Why this produces exactly the reported symptom

- **Hover freezes.** No `CursorMoved` arrives, so `update_pointer` cannot run
  and no hover state can change.
- **It then "catches up" all at once.** The first event after the gap carries a
  current position, so the UI jumps to the correct state in one frame.
- **Only while moving.** A stationary pointer generates no events anyway, so a
  delivery gap is invisible.
- **Clicks land late.** Consistent with the reported symptom: a `MouseDown`
  inside a gap would not be delivered until the gap ends, and once delivered,
  the app reacts in ~30ms. (Button events themselves were not instrumented —
  see above — so this is inferred from the `CursorMoved` gaps, not measured
  directly.)

### Confirmed not the cause

Each of these was tested and eliminated:

| Hypothesis | How it was eliminated |
| --- | --- |
| Debug build | Reproduces in `--release` |
| External display / refresh mismatch | Reproduces on the built-in display |
| Unconditional GPU wait (`device.poll(wait_indefinitely)`) | Patched out — no change |
| Swapchain depth | `desired_maximum_frame_latency` 1, 2, and 3 — no change |
| Stuck pointer capture | Masonry auto-releases on `Up` (`passes/event.rs:255`) |
| Spinner/skeleton frame storm | Fixed (see §2); latency unchanged |
| View rebuild cost | 63µs worst case (`examples/perf_probe.rs`) |
| Slow masonry pass | Every pass <2.6ms during interaction |

### It is not our code

Stock `xilem`'s `calc` example shows the same motion-dependent input problems on
the same machine, with no void-ui code involved. Other (non-winit) macOS
applications track hover correctly under the same pointer motion, so the OS has
fresh events available — this process is not getting them.

That brackets the defect to somewhere between AppKit delivering an event and
masonry acting on it, i.e. upstream of this repository.

### Suspected mechanism (unverified)

Not established, and deliberately flagged as a hypothesis: macOS mouse-moved
delivery to a continuously-presenting app, possibly involving the window
server's frame pacing (`com.apple.FramePacing.LayerStateSyncQueue` appeared in
main-thread samples) or the AppKit tracking-area path. Confirming this needs a
winit-only reproduction with no masonry in the picture.

Related upstream issue, still open and consistent with these observations:
[rust-windowing/winit#2240 — "macOS mouse events are laggy"](https://github.com/rust-windowing/winit/issues/2240).

### On upgrading winit

Not a viable fix right now:

- We are already on **winit 0.30.13**, the newest stable (released 2026-03-02).
  Its release notes contain no mouse-delivery fixes.
- The newest published version overall, **0.31.0-beta.2** (released
  2025-11-16), is on a diverged `0.31.0` line, not a superset of `0.30.13`:
  per `gh api repos/rust-windowing/winit/compare/v0.30.13...v0.31.0-beta.2`,
  each side has commits the other lacks (0.30.13 has ~140 commits not in
  beta.2; beta.2 has ~346 not in 0.30.13, including the macOS backend split
  into `winit-appkit` — "Move AppKit (macOS) backend to `winit-appkit`
  (#4248)", 2025-05-25 — and a breaking pointer-event overhaul that renames
  `CursorMoved`/`MouseInput` to `PointerMoved`/`PointerButton`). So it is not
  ruled out by publish date alone, and its unique macOS/pointer work makes it
  worth a look — but it hasn't been tried against this bug: doing so means a
  breaking-change migration (the event renames) on a pre-release, unreleased
  API, which is why it isn't queued as a near-term fix.
- winit is not a direct dependency we control. `masonry_winit`,
  `accesskit_winit`, `ui-events-winit`, and `xilem` all depend on it and would
  have to move together.

---

## 2. Real void-ui bugs found and fixed

Both were genuine — the app never went idle before them — but neither caused the
reported latency.

### P0: hidden animators pinned the window at refresh rate

`SpinnerWidget` and `SkeletonWidget` re-armed `request_anim_frame()`
unconditionally, with no terminating condition. Masonry's anim pass has no
`is_stashed` check, so a spinner inside a closed overlay or collapsed panel kept
the **entire window** re-encoding at refresh rate forever.

Worse, the gallery's default panel (Button) shipped two permanently
`.loading(true)` buttons, so merely launching the gallery started a spinner that
never stopped — which is what put `render_to_texture` at the top of the original
profile before anyone touched anything.

Fixed by dropping out of `on_anim_frame` when stashed and restarting on
`StashedChanged(false)` — the discipline `VoidScrollBar` already followed via
`AnimationStatus::Ongoing` — plus putting the demo's loading state behind a
toggle. Commit `b41c283`.

### P1: auto-hiding scrollbars faded out on every mount

`VoidScrollBar` is constructed fully opaque, correct for `AlwaysVisible`. For
`OnActivity` it meant every mount played a 300ms fade-out of a scrollbar the
user never asked to see: ~19 full-window frames, on 16 of 36 demo panels, paid
on every single panel switch.

Fixed by snapping straight to hidden on `WidgetAdded` when `AutoHideScrollBar`
is set. Commit `334fe8b`.

**Note this is a deliberate behaviour change**: auto-hiding scroll containers no
longer flash their scrollbar on mount as a "this scrolls" hint.

---

## 3. Why the frame pipeline was never the problem

Neither masonry nor vello does damage tracking, so per-frame main-thread cost is
proportional to the *total* painted content of the window rather than to what
changed:

1. `masonry_core/src/passes/paint.rs` copies every widget's cached scene into
   one flat `Scene` every frame (`append_transformed`, unconditional). It
   carries a `// TODO - Handle damage regions` ([xilem#789]) and a
   `// TODO: We could skip painting children outside the parent clip path`.
2. `imaging_vello::encode_source` allocates a fresh `vello::Scene` and
   re-encodes all of it, after a full `validate()` walk.
3. `vello::Renderer::render_to_texture` resolves that encoding on the CPU again.

This makes a profile dominated by `render_to_texture` *expected*, and it is why
that reading was consistent with several unrelated problems. But the actual
numbers are small:

| Measurement (release, 1400x900) | Value |
| --- | --- |
| Scene flatten after invalidating one widget | 10–190µs |
| xilem view rebuild | 0.4–64µs |
| Full frame (`redraw`, incl. acquire + present) | p50 **8.1ms**, p99 10.6ms |
| Any masonry pass during interaction | <2.6ms |

Main-thread time is dominated by *waiting*, not working: 54% in
`CAMetalLayer nextDrawable`, 10% in `device.poll` (a sleep-polling loop on
Metal), and only 5% in actual render work. Removing both waits changed nothing,
which is consistent with them being ordinary vsync pacing rather than a defect.

[xilem#789]: https://github.com/linebender/xilem/issues/789

---

## 4. Tooling added

- **`examples/perf_probe.rs`** — headless per-panel cost probe. Reports scene
  command count, view-rebuild time, repaint time, and whether a panel ever lets
  the app go idle. Run with `--release`.
- **`examples/support/frame_trace.rs`** — opt-in frame tracing in the gallery
  (`VOID_UI_TRACE_FRAMES=1`). Prints click-to-repaint latency and slow passes.
- **`profiling` cargo feature** — streams CPU timings to Tracy. Deliberately
  routed through `masonry/tracy` rather than `masonry_winit/tracy`; the latter
  requests GPU timer features that hang the app on macOS before the window
  appears.

---

## 5. Instrumentation traps encountered

Recorded because each produced a confident, wrong conclusion before being
caught. All four share a signature: a number that was too clean.

1. **`update_new_widgets` "taking 1.2–4.4s".** Masonry stores a per-widget span
   created *inside* that pass (`passes/update.rs:195`); in `tracing` a child
   span keeps its parent alive, so the pass does not *close* until those widgets
   are dropped. Timing on close measures widget teardown. Fix: time enter→exit.
   Tell: seven 1.19s spans closing 4ms apart.
2. **`input -> frame` "8ms".** Measured the last event before each frame, which
   under continuous pointer motion is always ~one frame regardless of how stale
   the content is. It cannot see the reported bug at all.
3. **`YAVG`/`YMAX` video "big changes".** Reported `YAVG=9.7` with `YMAX=1.0` —
   a mean cannot exceed a max. Artifacts of duplicate/dropped capture frames.
4. **A constant "301.0ms click delay".** Two different clocks: the masonry patch
   started its timer on the first window event, the frame tracer at layer
   install, ~301ms earlier. Tell: identical to 0.1ms across eight samples.

The measurements that actually worked compared the app against **external ground
truth** — the live OS cursor position, a stock xilem app, other native apps —
rather than timing the app's internals and inferring.
