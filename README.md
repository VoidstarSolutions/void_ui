# void-ui

A general-purpose xilem/Masonry component library. Theme-driven, product-agnostic, focused on a tight catalog of well-built primitives rather than a sprawling kitchen sink.

Every component reads its colors, sizes, and type stack from a `Theme` value owned by the host application; swapping themes is a single state change, not a tree walk.

## Status

Shipped components:

- **`Button`** — variants (Default/Danger), active toggle, disabled, leading icon, focus ring
- **`Checkbox`** — tri-state-ready boolean control
- **`DataGrid`** — virtualized rows over very large/append-only streams,
  declarative columns, stable-`row_id` selection (anchor + shift-range +
  ctrl/cmd-toggle), TSV copy-to-clipboard, **host-side single + multi-column
  (tiebreak) sort**, **per-column filtering**, **conditional cell
  formatting**, **horizontal scroll**, **drag-to-resize**, and **column
  show/hide + reorder** — all column state keyed by a stable `ColumnId`.
  Two demos exercise it: a streaming tick blotter (capability) and a
  NASDAQ-style stock-quote board (the product "value lens"). See
  [`docs/DATA_GRID_HOST_CONTRACT.md`](docs/DATA_GRID_HOST_CONTRACT.md).
- **`Radio`** — radio group with single-selection state
- **`ScrollContainer`** — themed viewport over masonry's `Portal`
- **`SidebarItem`** — full-width nav row with accent on active
- **`Tooltip`** — anchored, dismiss-aware

Layout primitives: **`FlexWrap`** (left-to-right wrapping row), **`PointerInert`** (event-transparent wrapper), **`FloatingOverlay`** (the shared overlay primitive — see below).

A live gallery exercises every component end-to-end. Most panels show their
source inline via the `with_source!` macro; the interactive `Data Grid` and
`Stock Quotes` panels are full working demos (a streaming blotter and a
quote board) wired to live app state.

```sh
cargo run -p void-ui --example gallery
```

## Roadmap

The component surface grows in dependency order. Anything overlay-shaped (popover, dropdown, menu, tooltip, dialog, toast) shares a single primitive — building that primitive well first unlocks roughly a third of the eventual surface in one go.

Each phase ends with an API review pass before the next begins. Component scope and naming draw on conventions from mature Rust UI component libraries; credit is owed (see Acknowledgments).

### Foundations

The cross-cutting work that unblocks the rest of the roadmap.

| Component        | Size | Notes                                                        |
| ---------------- | ---- | ------------------------------------------------------------ |
| `Separator`      | S    | Horizontal/vertical divider, optional label.                 |
| `Overlay`/Portal | L    | Anchored positioning, dismiss-on-outside, keyboard trap.     |
| `Icon` registry  | M    | Named-icon system generalizing the ad-hoc `BezPath` in Button. |

Implementation order:

1. **Spike: `Tooltip` end-to-end.** Forces the overlay design and proves the model.
2. **`Separator` + `Icon` registry** in parallel — small, polish-unlocking.
3. **Primary form inputs** (Phase 1 below).
4. **`Select`** to validate the overlay design at higher complexity before committing to Menu / DatePicker.

### Phase 1 — Form inputs

| Component               | Size | Notes                                              |
| ----------------------- | ---- | -------------------------------------------------- |
| `Checkbox`              | S    |                                                    |
| `Switch`                | S    | Toggle.                                            |
| `Radio` / `RadioGroup`  | M    | Group state management.                            |
| `Label`                 | S    | Form label, optional required marker.              |
| `TextInput`             | L    | Caret, selection, IME — wrap masonry primitives.   |
| `NumberInput`           | M    | TextInput + step + parse/format.                   |
| `Slider`                | M    | Single-thumb first; dual-thumb later.              |
| `Form`                  | S    | Layout container, label/control pairing.          |

### Phase 2 — Selection & navigation

Depends on the overlay primitive.

| Component                                        | Size | Notes                          |
| ------------------------------------------------ | ---- | ------------------------------ |
| `Select` / `Dropdown`                            | M    | Popover + list + filter.       |
| `Menu` / `ContextMenu` / `DropdownMenu`          | M    | Popover + keyboard nav.        |
| `Tabs` / `TabBar`                                | M    |                                |
| `Breadcrumb`                                     | S    |                                |
| `Accordion` / `Collapsible`                      | M    |                                |

### Phase 3 — Feedback

| Component        | Size | Notes                                |
| ---------------- | ---- | ------------------------------------ |
| `Alert`          | S    | Info/success/warning/error variants. |
| `Spinner`        | S    |                                      |
| `Progress`       | S    | Linear.                              |
| `ProgressCircle` | M    |                                      |
| `Notification`   | M    | Toast queue + auto-dismiss.          |
| `Badge`          | S    | Count/dot overlay.                   |
| `Tag` / `Chip`   | S    | Semantic colored pill.               |
| `Skeleton`       | S    | Shimmer placeholder.                 |

### Phase 4 — Modal & structure

| Component                | Size | Notes                                              |
| ------------------------ | ---- | -------------------------------------------------- |
| `Dialog` / `AlertDialog` | M    | Header/body/footer on the overlay primitive.       |
| `Resizable`              | L    | Split-panel drag handles.                          |
| `GroupBox` / `Card`      | S    |                                                    |

### Phase 5 — Data display

| Component         | Size | Notes                                            |
| ----------------- | ---- | ------------------------------------------------ |
| `List`            | M    | Extract DataGrid's virtualization for a flat list. |
| `Tree`            | L    | Hierarchical with expand/collapse.               |
| `Pagination`      | S    |                                                  |
| `DescriptionList` | S    | Label/value pairs.                               |
| `HoverCard`       | M    | Tooltip with richer content.                     |

### Phase 6 — Specialized

| Component     | Size | Notes                                |
| ------------- | ---- | ------------------------------------ |
| `DatePicker`  | L    | Calendar + range.                    |
| `ColorPicker` | L    | HSL/RGB/hex tabs + palette.          |
| `Stepper`     | M    | Multi-step wizard.                   |
| `Kbd`         | S    | Keyboard shortcut chip.              |

## Descope

Explicitly out of scope, even if they appear in inspiration libraries:

- **IDE-style dockable panels.** A `Resizable` split covers the realistic cases; full dock + drag/drop + persisted layouts is a project, not a component.
- **Built-in inspector / debug overlay.** Internal tooling, not a consumer component.
- **Code editor / syntax highlighter.** Tree-sitter + LSP is a separate product surface.
- **Rich-text / Markdown / HTML rendering.** Build narrow text helpers as specific surfaces need them.
- **Charts and plotting.** Charting is its own design space; consumers can compose a chart widget as a peer crate.
- **Exotic input variants (OTP, code-cell inputs).**
- **Avatar / Rating / similar consumer-app patterns.**

## Architecture notes

- **Two layers.** Xilem `View`s coordinate state and rebuild logic; masonry `Widget`s own paint, layout, and event handling. Each component ships both: a builder + `View` in `view.rs`, a widget in `widget.rs`, a gallery panel in `demo.rs`.
- **Theme at the render boundary.** `Theme` is passed when materializing a view (`.render(&theme)`), copied into the widget, and re-applied on rebuild when the value differs. No global theme state.
- **Presentation only.** Components do not own business logic, network, or persistence. State is passed in; events flow out.

## Acknowledgments

The component catalog draws inspiration from the Zed editor's `gpui-component` library, which provides a mature reference for what a complete UI component surface looks like in a Rust framework. void-ui is not a port — the underlying runtime (xilem/Masonry vs. gpui) differs enough that abstractions don't transfer — but the scoping decisions on which components are worth shipping owe a debt to that work.
