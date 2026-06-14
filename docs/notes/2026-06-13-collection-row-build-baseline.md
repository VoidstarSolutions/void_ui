# Collection per-row build baseline (reference)

Reference "before" measurement for the per-row memoization optimization
**deferred to the list rebuild branch**. Numbers are relative and
hardware-dependent — use only for before/after comparison on the same
machine and build profile.

## What is measured

One rebuild of a window of **40 visible rows** through the per-row work
`collection_body`'s `virtual_scroll` closure performs (`src/collection/body_view.rs`):
resolve the item, compute `is_selected` via the selection lens, build the
per-row content view (a `flex_row` of 3 labels, mirroring `data_grid`),
wrap it in the selection-background `sized_box`, and set up the
`clickable_row` click closure (cloning the items accessor, id source, and
selection lens `Arc`s).

## Method / harness

Instrumented `#[ignore]`d test `row_build_baseline` in
`src/collection/body_view.rs` (`std::time::Instant`, median of 2000
windowed builds after a 50-iter warmup; `std::hint::black_box` defeats
dead-code elimination). Criterion is **not** a dev-dependency and was not
added, to avoid dependency churn on this refactor branch.

Run:

```sh
cargo test --all-features --release -- --ignored --nocapture row_build_baseline
```

## Baseline (this machine: Apple Silicon, darwin)

| profile         | per 40-row window | per row |
| --------------- | ----------------- | ------- |
| release         | ~5.2–5.4 µs       | ~131 ns |
| debug (unopt.)  | ~17–22 µs         | ~430–550 ns |

## Caveats / honest scope

- This **replicates the body** of `collection_body`'s per-row closure; it
  does not drive the closure through xilem's View machinery. No
  app/view-level rebuild harness exists on this xilem rev (only masonry's
  widget-level `TestHarness`).
- View values are lazy, so this measures **construction of the per-row
  view tree + the `Arc` clones** — NOT xilem's diff/`rebuild` traversal or
  any widget mutation. The memoization win targets exactly this
  construction/clone cost, so it is the right "before" reference.
- Prefer the **release** numbers; debug is included only for context.
