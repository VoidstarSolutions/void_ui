# void-ui

A general-purpose xilem/Masonry component library. Theme-driven, product-agnostic, focused on a tight catalog of well-built primitives rather than a sprawling kitchen sink.

Every component reads its colors, sizes, and type stack from a `Theme` value owned by the host application; swapping themes is a single state change, not a tree walk.

## Status

Shipped components, grouped by role:

**Forms & inputs**

- **`Button`** — variants (Default/Danger), active toggle, disabled, leading icon, focus ring
- **`ButtonGroup`** — segmented control; plain and toggle-group variants
- **`Checkbox`** — tri-state-ready boolean control
- **`Radio`** — radio group with single-selection state
- **`Toggle`** — on/off switch
- **`Input`** — themed single-line text input (prefix/suffix, placeholder, disabled)
- **`Slider`** — single-thumb and dual-thumb range, theme-driven
- **`Label`** — form label with alignment control

**Overlays & menus** (all on the shared overlay primitive — see below)

- **`Popover`** — anchored floating panel
- **`DropdownButton`** — button + anchored menu
- **`ContextMenu`** — right-click rich-item menu
- **`Autocomplete`** — text input + filtered suggestion list
- **`Tooltip`** — anchored, dismiss-aware
- **`Dialog`** — modal header/body/footer over the outermost overlay scope
- **`Notification`** — toast queue with auto-dismiss

**Navigation, layout & structure**

- **`SidebarItem`** / **`Sidebar`** — full-width nav row (accent on active) + collapsible panel
- **`Breadcrumb`** — segmented path trail
- **`Tabs`** — tab bar + panels
- **`Collapsible`** — animated disclosure section
- **`Separator`** — horizontal/vertical divider, optional label
- **`GroupBox`** / **`Card`** — titled and untitled containers
- **`Resizable`** — split panel with drag handles
- **`ScrollContainer`** — themed viewport over masonry's `Portal`

**Data display & feedback**

- **`DataGrid`** — virtualized rows over very large/append-only streams,
  declarative columns, stable-`row_id` selection (anchor + shift-range +
  ctrl/cmd-toggle), TSV copy-to-clipboard, **host-side single + multi-column
  (tiebreak) sort**, **per-column filtering**, **conditional cell
  formatting**, **horizontal scroll**, **drag-to-resize**, and **column
  show/hide + reorder** — all column state keyed by a stable `ColumnId`.
  Two demos exercise it: a streaming tick blotter (capability) and a
  NASDAQ-style stock-quote board (the product "value lens"). See
  [`docs/DATA_GRID_HOST_CONTRACT.md`](docs/DATA_GRID_HOST_CONTRACT.md).
- **`List`** — flat virtualized list (DataGrid's virtualization, single-column)
- **`Meter`** — linear progress/fill bar, optional gradient
- **`Badge`** — count/dot pill
- **`StatusDot`** — semantic status indicator
- **`Alert`** — info/success/warning/error banner, optional dismiss
- **`Skeleton`** — shimmer loading placeholder
- **`Spinner`** — indeterminate activity indicator
- **`Icon`** — named-icon registry
- **`CodeView`** — read-only highlighted text
- **`DatePicker`** — calendar popover
- **`Clipboard`** — copy-to-clipboard button

Layout primitives: **`FlexWrap`** (left-to-right wrapping row, `src/layout/flex_wrap/`), **`PointerInert`** (event-transparent wrapper, `src/pointer_inert.rs`), **`FloatingOverlay`** (the shared overlay primitive, `src/floating.rs` — see below).

A live gallery exercises every component end-to-end. Most panels show their
source inline via the `with_source!` macro; the interactive `Data Grid` and
`Stock Quotes` panels are full working demos (a streaming blotter and a
quote board) wired to live app state.

```sh
cargo run -p void-ui --example gallery --features gallery
```

## Roadmap

The component surface grew in dependency order. Anything overlay-shaped (popover, dropdown, menu, tooltip, dialog, toast) shares a single primitive — building that primitive well first unlocked roughly a third of the surface in one go.

Each phase ended with an API review pass before the next began. Component scope and naming draw on conventions from mature Rust UI component libraries; credit is owed (see Acknowledgments).

The overlay primitive and Foundations are shipped, as is most of Phases 1–4 (only `NumberInput`, `Form`, `ProgressCircle`, and `Tag` / `Chip` remain), plus `List` and `DatePicker` from Phases 5–6. What remains is the long tail of specialized surfaces, called out in **Remaining** below. Names in parentheses are the shipped module names where they differ from the original roadmap label.

### Foundations — shipped

| Component        | Size | Status | Notes                                                        |
| ---------------- | ---- | ------ | ------------------------------------------------------------ |
| `Separator`      | S    | ✓      | Horizontal/vertical divider, optional label.                 |
| `Overlay`/Portal | L    | ✓      | `overlay_scope` + `overlay_portal` + `FloatingOverlay`.      |
| `Icon` registry  | M    | ✓      | Named-icon system (`IconName`).                              |

### Phase 1 — Form inputs

| Component               | Size | Status | Notes                                              |
| ----------------------- | ---- | ------ | -------------------------------------------------- |
| `Checkbox`              | S    | ✓      |                                                    |
| `Switch` (`toggle`)     | S    | ✓      |                                                    |
| `Radio` / `RadioGroup`  | M    | ✓      | Group state management.                            |
| `Label`                 | S    | ✓      |                                                    |
| `TextInput` (`input`)   | L    | ✓      | Wraps masonry's `TextInput`, themed chrome.        |
| `Slider`                | M    | ✓      | Single-thumb and dual-thumb range.                 |
| `NumberInput`           | M    | —      | TextInput + step + parse/format.                   |
| `Form`                  | S    | —      | Layout container, label/control pairing.           |

### Phase 2 — Selection & navigation — shipped

| Component                                        | Size | Status | Notes                          |
| ------------------------------------------------ | ---- | ------ | ------------------------------ |
| `Select` (`dropdown_button` / `autocomplete`)    | M    | ✓      | Popover + list + filter.       |
| `Menu` / `ContextMenu` (`context_menu`)          | M    | ✓      | Popover + keyboard nav.        |
| `Tabs` / `TabBar`                                | M    | ✓      |                                |
| `Breadcrumb`                                     | S    | ✓      |                                |
| `Accordion` (`collapsible`)                      | M    | ✓      |                                |

### Phase 3 — Feedback

| Component            | Size | Status | Notes                                |
| -------------------- | ---- | ------ | ------------------------------------ |
| `Alert`              | S    | ✓      | Info/success/warning/error variants. |
| `Spinner`            | S    | ✓      |                                      |
| `Progress` (`meter`) | S    | ✓      | Linear.                              |
| `Notification`       | M    | ✓      | Toast queue + auto-dismiss.          |
| `Badge`              | S    | ✓      | Count/dot overlay.                   |
| `Skeleton`           | S    | ✓      | Shimmer placeholder.                 |
| `ProgressCircle`     | M    | —      |                                      |
| `Tag` / `Chip`       | S    | —      | Semantic colored pill.               |

### Phase 4 — Modal & structure — shipped

| Component                | Size | Status | Notes                                              |
| ------------------------ | ---- | ------ | -------------------------------------------------- |
| `Dialog` / `AlertDialog` | M    | ✓      | Header/body/footer on the overlay primitive.       |
| `Resizable`              | L    | ✓      | Split-panel drag handles.                          |
| `GroupBox` / `Card`      | S    | ✓      |                                                    |

### Phase 5 — Data display

| Component         | Size | Status | Notes                                            |
| ----------------- | ---- | ------ | ------------------------------------------------ |
| `List`            | M    | ✓      | DataGrid's virtualization for a flat list.       |
| `Tree`            | L    | —      | Hierarchical with expand/collapse.               |
| `Pagination`      | S    | —      |                                                  |
| `DescriptionList` | S    | —      | Label/value pairs.                               |
| `HoverCard`       | M    | —      | Tooltip with richer content.                     |

### Phase 6 — Specialized

| Component     | Size | Status | Notes                                |
| ------------- | ---- | ------ | ------------------------------------ |
| `DatePicker`  | L    | ✓      | Calendar popover.                    |
| `ColorPicker` | L    | —      | HSL/RGB/hex tabs + palette.          |
| `Stepper`     | M    | —      | Multi-step wizard.                   |
| `Kbd`         | S    | —      | Keyboard shortcut chip.              |

### Remaining

The open surface, all `—` above: `NumberInput`, `Form`, `ProgressCircle`, `Tag`/`Chip`, `Tree`, `Pagination`, `DescriptionList`, `HoverCard`, `ColorPicker`, `Stepper`, `Kbd`.

Also shipped beyond the original roadmap: `ButtonGroup`, `Clipboard`, `CodeView`, `StatusDot`, `Popover` (standalone), and the collapsible `Sidebar` panel (beyond `SidebarItem`).

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
