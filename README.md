# void-ui

Xilem/Masonry component library for Voidstar UIs. Components are theme-driven, product-agnostic, and display-only — analysis logic lives server-side per the workspace IP boundary.

The visual language is sourced from the Tessera P&F prototype and lives in `theme`. Each component reads its colors, sizes, and type stack from a `Theme` value owned by the host application; swapping themes is a single state change, not a tree walk.

## Status

Shipped components:

- **`Button`** — variants (Default/Danger), active toggle, disabled, leading icon, focus ring
- **`DataGrid`** — virtualized rows, declarative columns, selection model, copy-to-clipboard shortcut, overflow detection
- **`SidebarItem`** — full-width nav row with teal accent on active
- **`Chart`** — xilem wrapper around `citadel_chart::ChartWidget`

Layout primitives: **`FlexWrap`** (left-to-right wrapping row), **`PointerInert`** (event-transparent wrapper).

A live gallery exercises every component end-to-end with source snippets via the `with_source!` macro:

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
| `Tooltip`        | M    | First overlay consumer; validates the design.                |
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
| `Resizable`              | L    | Split-panel drag handles for the chart workspace.  |
| `GroupBox` / `Card`      | S    |                                                    |

### Phase 5 — Data display

| Component         | Size | Notes                                            |
| ----------------- | ---- | ------------------------------------------------ |
| `List`            | M    | Extract DataGrid's virtualization for a flat list. |
| `Tree`            | L    | Hierarchical with expand/collapse.               |
| `Pagination`      | S    |                                                  |
| `DescriptionList` | S    | Label/value pairs — useful for instrument context. |
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

- **IDE-style dockable panels.** A `Resizable` split is enough for the chart workspace; full dock + drag/drop + persisted layouts is a project, not a component.
- **Built-in inspector / debug overlay.** Internal tooling, not a consumer component.
- **Code editor / syntax highlighter.** Tree-sitter + LSP is a separate product surface.
- **Rich-text / Markdown / HTML rendering.** Build narrow text helpers as specific surfaces need them.
- **Generic chart suite (Area/Line/Bar/Candlestick/Pie).** `citadel_chart` is the product chart; void_ui does not ship a competing plotting library.
- **Exotic input variants (OTP, code-cell inputs).** Not citadel use cases.
- **Avatar / Rating / similar consumer-app patterns.** Not citadel use cases.

## Architecture notes

- **Two layers.** Xilem `View`s coordinate state and rebuild logic; masonry `Widget`s own paint, layout, and event handling. Each component ships both: a builder + `View` in `view.rs`, a widget in `widget.rs`, a gallery panel in `demo.rs`.
- **Theme at the render boundary.** `Theme` is passed when materializing a view (`.render(&theme)`), copied into the widget, and re-applied on rebuild when the value differs. No global theme state.
- **No analysis imports.** UI crates may import data types (`Tick`, `ColumnDelta`, `ChartSnapshot`) but never analysis primitives (`pf::ColumnBuilder`, `Analysis::process`). The architecture's IP boundary is enforced at the import level.

## Acknowledgments

The component catalog draws inspiration from the Zed editor's `gpui-component` library, which provides a mature reference for what a complete UI component surface looks like in a Rust framework. void-ui is not a port — the underlying runtime (xilem/Masonry vs. gpui) differs enough that abstractions don't transfer — but the scoping decisions on which components are worth shipping owe a debt to that work.
