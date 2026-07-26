# Data Grid gallery screen consolidation — Design

> **Revision note:** this spec originally proposed stacking all three
> panels in one scrollable column with no switcher. The user reversed that
> decision after reviewing it and asked for a tabbed switcher instead, with
> state preserved across tab switches. This revision reflects that
> decision; the "stacked" approach and why it was set aside are kept below
> for context since the underlying problem (avoid tearing down panel state)
> is the same one that ruled out a naive switcher the first time.

## Context

Issue #165 (open, not yet fixed) observes that `ComponentKind::StockQuotes`
and `ComponentKind::TreeGrid` aren't real components — they're two extra
demo panels for `data_grid`, both dispatching into `data_grid::demo`
functions, but listed in the gallery's left nav as if they were peer
components alongside `DataGrid` itself. `ComponentKind`'s own doc comment
says "one entry per component"; these two variants violate that, and they
also sit out of alphabetical/logical order in `all()` (between `StatusDot`
and `Tabs`, not next to `DataGrid`).

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

## Decision: tabbed switcher, state preserved across switches

The user chose the issue's first suggested option — an internal mode
switcher (grid / stock quotes / tree) — with one hard requirement: **each
tab's state must survive switching away and back.**

This rules out the simplest implementation. xilem's `OneOf` view
combinator (the natural way to swap between three differently-typed panel
views) tears down and rebuilds the inactive branch's `ViewState` on every
switch (confirmed by reading `xilem_core/src/views/one_of.rs`'s
`rebuild` — a variant change runs `teardown` on the old branch, then a
fresh `build`). For the main grid panel specifically, that would mean
regenerating its 100k-row synthetic tick dataset and losing
sort/filter/selection/scroll state every time the user tabbed away and
back.

An earlier revision of this spec avoided the problem by never switching at
all (stacking all three, always visible, page-scrolled). The user reversed
that in favor of an actual tabbed switcher, so the state-preservation
problem has to be solved directly instead of sidestepped: **all three
panels stay mounted at all times — none are ever torn down — and only the
active one is visually shown.**

There's no existing precedent in this crate for "hide a xilem `View`'s
content without unmounting it." `Collapsible`'s `AnimatedClip`
(`src/animated_clip.rs`) solves an adjacent problem — it's a masonry
*widget*-level primitive that keeps a child laid out at full size and
animates a clip mask over 250ms — but it's built for smooth slide
animations, not instant tab switching, and using it here would mean
building a whole new custom masonry widget for no visual benefit (no
animation was requested). The simpler mechanism below reuses purely
layout-level tools already used throughout this file.

## Architecture

### `ComponentKind` (`src/components/mod.rs`)

Unchanged from the original decision: remove the `StockQuotes` and
`TreeGrid` variants entirely — from the enum definition, the `label()`
match, and the `all()` list. The gallery nav goes from three
data-grid-related entries down to one (`Data Grid`).

No other code references these two variants outside `components/mod.rs`
and `examples/gallery.rs` (confirmed by grep across `src/` and
`examples/`), and no test asserts an exact count or exact contents of
`ComponentKind::all()`.

### Composition (`src/components/data_grid/demo.rs`)

`panel()` — the function `ComponentKind::DataGrid` dispatches to — changes
from returning `DataGridDemoPanel` directly to a new wrapping panel that
owns a small private "which tab is active" state and composes all three
existing panels as always-mounted children, mirroring the same
component-local-state pattern this file (and `ButtonDemoPanel`/
`GroupBoxDemoPanel` elsewhere) already uses — just with the private state
being a tab index instead of e.g. `disabled`/`selected` booleans.

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
enum DataGridMode {
    Grid,
    StockQuotes,
    Tree,
}

impl DataGridMode {
    const ALL: [Self; 3] = [Self::Grid, Self::StockQuotes, Self::Tree];

    fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap()
    }

    fn from_index(i: usize) -> Self {
        Self::ALL[i]
    }
}

#[derive(Clone, Copy)]
struct DataGridScreenState {
    mode: DataGridMode,
}

type ScreenInnerView = Box<AnyWidgetView<DataGridScreenState>>;
type ScreenInnerViewState = <ScreenInnerView as View<DataGridScreenState, (), ViewCtx>>::ViewState;

/// Opaque state owned by the combined data-grid screen panel.
pub struct DataGridScreenPanelState {
    state: DataGridScreenState,
    inner_view: ScreenInnerView,
    inner_state: ScreenInnerViewState,
}

/// The combined Data Grid gallery screen, returned by [`panel`].
pub struct DataGridScreenPanel {
    theme: Theme,
}

/// Renders the combined Data Grid gallery screen: a tabs() switcher over
/// the main grid, stock quotes, and tree grid. All three panels stay
/// mounted regardless of which tab is active, so switching tabs never
/// loses state or regenerates the main grid's synthetic dataset.
#[must_use]
pub fn panel(theme: &Theme) -> DataGridScreenPanel {
    DataGridScreenPanel { theme: *theme }
}

fn build_screen(theme: &Theme, state: &DataGridScreenState) -> impl WidgetView<DataGridScreenState> + use<> {
    let switcher = tabs(
        vec![
            TabItem::label("Grid"),
            TabItem::label("Stock Quotes"),
            TabItem::label("Tree"),
        ],
        state.mode.index(),
        |s: &mut DataGridScreenState, i: usize| {
            s.mode = DataGridMode::from_index(i);
        },
    )
    .render(theme);

    flex_col((
        switcher,
        tab_content(state.mode == DataGridMode::Grid, DataGridDemoPanel { theme: *theme }),
        tab_content(state.mode == DataGridMode::StockQuotes, stock_quotes_panel(theme)),
        tab_content(state.mode == DataGridMode::Tree, tree_grid_panel(theme)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(12.0))
}

/// The active panel fills available height exactly like it does standalone
/// today (`.flex(1.0)`); inactive panels collapse to zero height rather
/// than being torn down, so their internal state survives.
fn tab_content<S: 'static, V: WidgetView<S> + 'static>(
    active: bool,
    content: V,
) -> Box<AnyWidgetView<S>> {
    if active {
        Box::new(sized_box(content).flex(1.0))
    } else {
        Box::new(sized_box(content).fixed_height(Length::px(0.0)))
    }
}

// build/rebuild/teardown/message impl for DataGridScreenPanel mirrors
// GroupBoxDemoPanel's existing View impl in group_box/demo.rs verbatim,
// substituting DataGridScreenState for GroupBoxDemo and build_screen for
// build_inner.
```

Key points:

- `DataGridDemoPanel`, `StockQuotesDemoPanel` (via `stock_quotes_panel()`),
  and `TreeGridDemoPanel` (via `tree_grid_panel()`) are used completely
  unchanged — each already implements `View<S, (), ViewCtx>` generically
  for any `S: 'static`, so they nest directly as children typed against
  the new `DataGridScreenState`, exactly the way `group_box/demo.rs`'s
  sections nest `Box<AnyWidgetView<GroupBoxDemo>>` children today. No
  changes needed inside any of the three panels' own `build_inner`/
  `build_stock_inner`/`build_tree_inner` — this is why the "make the
  dynamic height / `with_source!` behavior the same across all three"
  requirement is automatically satisfied: each panel already ends with
  `flex_col((toolbar, sized_box(grid).flex(1.0)))` (after the `.flex(1.0)`
  fix already on this branch), and `tab_content`'s `.flex(1.0)` wrapper
  around the *active* panel composes with that identically for all three,
  regardless of which one is showing.
- All three panels are *always* present in the `flex_col` tuple on every
  build/rebuild — none are ever conditionally omitted — so their
  `ViewState` (and the `Demo`/`StockDemo`/`TreeDemo` state living inside
  it) is never torn down. Only their rendered height changes.
- `tab_content` unifies the `if active { .flex(1.0) } else { .fixed_height(0.0) }`
  branches into `Box<AnyWidgetView<S>>` because the two branches produce
  different concrete types — the same type-unification technique
  `group_box/demo.rs`'s section functions already use for their own
  differently-shaped branches.
- `DataGridMode`/`DataGridScreenState`/`DataGridScreenPanel`/
  `DataGridScreenPanelState` are new, but the `View`/`ViewState`
  build/rebuild/teardown/message boilerplate is a direct structural copy
  of `GroupBoxDemoPanel`'s existing impl in this same crate (same file
  even, `group_box/demo.rs`) — not a new pattern being invented, just
  applied one level higher (managing a tab index instead of e.g. checkbox
  booleans).
- `tabs()`'s `on_select` callback signature is
  `Fn(&mut State, usize) -> Action` (confirmed against
  `src/components/tabs/view.rs:160-166`) — matches the closure shown
  above directly, no adapter needed.

### `examples/gallery.rs`

Unchanged from the original decision: remove the
`ComponentKind::StockQuotes` and `ComponentKind::TreeGrid` match arms.
The `ComponentKind::DataGrid` arm is unchanged:
`Box::new(void_ui::components::data_grid::demo::panel(theme))` still
compiles, since `panel()`'s return type (`DataGridScreenPanel`, a concrete
struct implementing `View<S, (), ViewCtx>` for any `S`) boxes into
`Box<AnyWidgetView<S>>` exactly the way the old `DataGridDemoPanel` did.

## Testing

Gallery-only demo/nav code, no new component logic — same verification
posture as the `with_source!` work earlier on this branch:
`cargo build -p void-ui --example gallery --features gallery`, the
existing `cargo test --all-features` suite (confirmed no test references
the removed `ComponentKind` variants or asserts an exact count/contents of
`ComponentKind::all()`), and `cargo clippy --all-targets --all-features`.
No new unit tests are strictly required by the plan's scope, but given
this introduces genuinely new logic (`DataGridMode::index`/`from_index`,
the always-mounted/height-toggle mechanism) rather than pure
recomposition, the implementation plan should include a couple of thin
tests: that `DataGridMode::from_index(mode.index()) == mode` for all three
modes (round-trip), and that switching `DataGridScreenState.mode` and
rebuilding doesn't drop the inactive panels' `ViewState` (mirroring the
style of `set_theme_cascades_into_both_scrollbar_children` in
`scroll_container/widget.rs`'s test module). Visual confirmation of the
tab switcher and per-tab layout is left to the user via the running
gallery.

## Out of scope

- Any change to `data_grid`'s 8-arg builder signature (issue #145).
- Any deeper unification of the three panels' toolbars, state, or data
  sources into one shared component — this is a composition of three
  already-independent, already-tested panels, not a redesign of them.
- Animated tab transitions (`AnimatedClip`-based slide/fade) — not
  requested; the switch is instant.
