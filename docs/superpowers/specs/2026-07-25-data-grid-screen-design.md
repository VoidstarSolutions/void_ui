# Data Grid gallery screen consolidation — Design

## Context

Issue #165 (open, not yet fixed) observes that `ComponentKind::StockQuotes` and
`ComponentKind::TreeGrid` aren't real components — they're two extra demo
panels for `data_grid`, both dispatching into `data_grid::demo` functions,
but listed in the gallery's left nav as if they were peer components
alongside `DataGrid` itself. `ComponentKind`'s own doc comment says "one
entry per component"; these two variants violate that, and they also sit
out of alphabetical/logical order in `all()` (between `StatusDot` and
`Tabs`, not next to `DataGrid`).

The issue's own suggested fix (verbatim): "Either give `data_grid` a panel
with an internal mode switcher (grid / stock quotes / tree), or introduce a
separate notion of 'extra demo panels' distinct from `ComponentKind`."

This spec was triggered mid-flow on branch
`162-data_grid-and-group_box-demos-never-call-with_source` (a separate,
already-completed and tested fix for issue #162 — `with_source!` coverage
in demo panels) after the user visually reviewed that branch's gallery
output and requested this restructuring be folded in before merging. The
user was asked, and confirmed, to do this on the same branch rather than a
new one.

## Decision: no mode switcher, all three stacked

The issue's first suggested option (an internal mode switcher) was
considered and explicitly rejected by the user in favor of the simpler
option: **no switching mechanism — all three grids visible on the same
screen, stacked in one column.**

This sidesteps a real architectural cost the switcher option would have
carried: xilem's `OneOf` view combinator (the natural way to implement a
switcher over three differently-typed panel views) tears down and rebuilds
the inactive branch's `ViewState` on every switch (confirmed by reading
`xilem_core/src/views/one_of.rs`'s `rebuild` — a variant change runs
`teardown` on the old branch, then a fresh `build`). For the main grid
panel specifically, that would mean regenerating its 100k-row synthetic
tick dataset and losing sort/filter/selection/scroll state every time the
user switched away and back. Stacking avoids this entirely: nothing is
ever unmounted, so nothing is ever reset.

## Architecture

### `ComponentKind` (`src/components/mod.rs`)

Remove the `StockQuotes` and `TreeGrid` variants entirely — from the enum
definition, the `label()` match, and the `all()` list. This directly
resolves the issue: the gallery nav goes from three data-grid-related
entries (`Data Grid`, `Stock Quotes`, `Tree Grid`) down to one
(`Data Grid`).

No other code references these two variants outside `components/mod.rs`
and `examples/gallery.rs` (confirmed by grep across `src/` and
`examples/`), and no test asserts an exact count or exact contents of
`ComponentKind::all()` — removing two entries is a safe, self-contained
change.

### Composition (`src/components/data_grid/demo.rs`)

`panel()` — the function `ComponentKind::DataGrid` dispatches to — changes
from returning just the main-grid panel (`DataGridDemoPanel`) to a combined
screen composing all three existing panels as siblings:

```rust
pub fn panel<S: 'static>(theme: &Theme) -> impl WidgetView<S> + use<S> {
    let theme = *theme;
    scroll_container(
        flex_col((
            demo_section("Main Grid", &theme, DataGridDemoPanel { theme }),
            demo_section("Stock Quotes", &theme, stock_quotes_panel(&theme)),
            demo_section("Tree Grid", &theme, tree_grid_panel(&theme)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(24.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(&theme)
}

fn demo_section<S: 'static, V: WidgetView<S> + 'static>(
    title: &'static str,
    theme: &Theme,
    content: V,
) -> impl WidgetView<S> + use<S, V> {
    flex_col((
        section_header(title, theme),
        sized_box(content).fixed_height(Length::px(520.0)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(8.0))
}

fn section_header<S: 'static>(text: &'static str, theme: &Theme) -> impl WidgetView<S> + use<S> {
    label(text)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}
```

Key points:

- `DataGridDemoPanel`, `StockQuotesDemoPanel`, and `TreeGridDemoPanel` each
  already implement `View<S, (), ViewCtx>` generically for any `S: 'static`
  (the crate's established "component-local state" pattern — each owns its
  `Demo`/`StockDemo`/`TreeDemo` state internally, independent of whatever
  state the embedding app has). This means they compose as ordinary tuple
  children of `flex_col` with no new `View`/`ViewState`/`with_id` plumbing
  — xilem's tuple `ViewSequence` machinery already handles heterogeneous
  children and their id-routing internally (the same mechanism every
  `flex_col((a, b, c))` call in this codebase already relies on). This is
  *not* the manual-custom-View case the crate's `with_id` routing rule
  warns about — that rule applies when hand-writing a `View` impl that
  wraps child `WidgetView`s itself, which this composition doesn't do.
- `stock_quotes_panel()` and `tree_grid_panel()` are called exactly as
  they are today — unchanged, still `pub`, still independently
  constructible. They're just no longer reachable from the gallery nav on
  their own; `panel()`'s composition is now their only caller within the
  gallery.
- `DataGridDemoPanel { theme }` is constructed inline (its `theme` field is
  private but accessible within the same module) rather than via the old
  single-purpose `panel()` — that name is being repurposed for the
  combined screen.
- Each `demo_section` wraps its content in `sized_box(content).fixed_height(Length::px(520.0))`.
  Inside each of the three panels, `build_inner`/`build_stock_inner`/
  `build_tree_inner` already end with `flex_col((toolbar, sized_box(grid).flex(1.0)))`
  — after the `.flex(1.0)` fix landed earlier on this branch, that
  correctly distributes *whatever* height its container gives it between
  the toolbar (natural height) and the grid (flex, gets the rest). Nesting
  each panel inside a 520px fixed-height box is exactly the same
  mechanism that previously filled the full window — no changes needed
  inside any of the three panels' own build functions.
- Section headers use the same small caption-style treatment as
  `group_box/demo.rs`'s existing `section_header` helper (duplicated
  locally in `data_grid/demo.rs` rather than shared — there's no existing
  crate-level helper for this, and both files' versions are small enough
  that extracting a shared one isn't warranted by two call sites).

### `examples/gallery.rs`

Remove the `ComponentKind::StockQuotes` and `ComponentKind::TreeGrid` match
arms — they no longer exist as variants. The `ComponentKind::DataGrid` arm
is unchanged: `Box::new(void_ui::components::data_grid::demo::panel(theme))`
still compiles, since `panel()`'s new `impl WidgetView<S>` return type
still boxes into `Box<AnyWidgetView<S>>` the same way a concrete struct
did.

## Testing

Gallery-only demo/nav code, no new component logic — same verification
posture as the `with_source!` work earlier on this branch:
`cargo build -p void-ui --example gallery --features gallery`, the
existing `cargo test --all-features` suite (confirmed no test references
the removed `ComponentKind` variants or asserts an exact count/contents of
`ComponentKind::all()`), and `cargo clippy --all-targets --all-features`.
No new unit tests are warranted. Visual confirmation of the combined
screen's layout is left to the user via the running gallery, per this
crate's established convention of not claiming visual verification that
wasn't actually performed.

## Out of scope

- Any change to `data_grid`'s 8-arg builder signature (issue #145).
- Any deeper unification of the three panels' toolbars, state, or data
  sources into one shared component — this is a composition of three
  already-independent, already-tested panels, not a redesign of them.
