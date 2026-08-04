# ColorPicker Design

**Issue:** #226 — ColorPicker: HSL/RGB/hex picker with palette (Phase 6 specialized component, Size L).

## Goal

A theme-driven color picker component for `void-ui`: a graphical saturation/value
field, hue slider, alpha slider, host-supplied preset palette, and HSL/RGB/hex
numeric entry. Emits color-change events. Presentation-only, product-agnostic,
two-layer (view + widget) split, with a gallery demo before the API is final.

## Public API

Follows the void_ui host-managed callback convention (same shape as `slider`,
`tabs`, `date_picker`): the host stores the value and passes an `on_change`
callback that updates its own state.

```rust
use void_ui::components::color_picker::color_picker;

color_picker(
    state.color,                       // masonry::peniko::Color (carries alpha)
    |s: &mut State, c| s.color = c,    // on_change(Color)
)
.palette(vec![c1, c2, c3])             // optional host swatches; none shown if unset
.disabled(false)                       // optional
.render(&theme)
```

- The host holds a single `peniko::Color`. Every edit (SV drag, hue/alpha move,
  numeric field, hex box, swatch click) fires `on_change(Color)`.
- No HSV/HSL/alpha representation leaks into the host API — the host only ever
  sees `Color`.
- `.palette(Vec<Color>)` is optional. If unset, no swatch row is shown (keeps the
  component product-agnostic per CLAUDE.md — the host owns brand swatches).
- `.disabled(bool)` greys out and inerts all sub-controls.

## State ownership

This is the crux of the design.

An SV field + hue slider require HSV state that **cannot** be recovered from an
sRGB `Color` alone: hue is lost at grayscale, and both hue and saturation are
undefined at pure black. Round-tripping the host's `Color` through RGB on every
rebuild would collapse the picker's position.

Therefore:

- **Canonical working state lives in the widget:** `hsva = (h, s, v, a)` (hue in
  degrees, s/v/a in `0.0..=1.0`), plus `active_tab: ColorTab`.
- The widget derives a `Color` from `hsva` and emits it; it never destructively
  reads its own hue back from that `Color`.
- **Prop-sync guard:** on rebuild, the View syncs the incoming `Color` prop into
  the widget's `hsva` **only when that `Color` differs from the widget's own
  currently-derived color** (byte-equal on sRGBA8 comparison). This is the same
  guard `slider` uses to avoid clobbering an in-progress drag: an external
  `state.color = ...` change flows in, but the widget echoing its own change back
  through the host does not reset the field. When an external color is grayscale,
  the existing hue is preserved (only s/v/a updated) so the SV field keeps a
  meaningful hue axis.

## Layout

Reconciles "tabs switch the entire body" with the issue's required always-on SV
field + hue slider: the graphical core and palette are persistent; only the
**numeric editing body below the tabs** swaps wholesale per tab.

```
┌────────────────────────────┐
│  [   SV square   ] │ hue │  │   ← graphical core, ALWAYS visible
│                    │ bar │  │
│  [ alpha bar over checker ] │
│  [ swatches row ]           │   ← palette, always visible (only if host set)
├────────────────────────────┤
│  ( RGB ) ( HSL ) ( hex )    │   ← tabs
│  [   numeric body swaps   ] │   ← ENTIRE numeric body swaps per tab:
│                             │       RGB = 3 labeled 0–255 fields
└────────────────────────────┘       HSL = H (0–360) + S/L (0–100%) fields
                                      hex = single 8-digit #RRGGBBAA box
```

- Graphical core (SV field, hue slider, alpha slider) and palette stay put across
  tab switches.
- The numeric body below the tabs is a full region (multiple rows of fields), not
  a compact readout row — it swaps wholesale when the active tab changes.
- All surfaces drive the same `hsva`. Moving the hue slider live-updates the SV
  field's gradient, the alpha bar's tint, and every visible numeric field.

## Architecture

Directory `src/components/color_picker/`, following the multi-file precedent of
`date_picker` (which splits `calendar_body`/`calendar_grid`/`calendar_math`
beyond the base `view`/`widget` pair).

| File | Responsibility |
|------|----------------|
| `view.rs` | `ColorPicker<State, Action>` builder + xilem `View` impl. Holds the `Color` prop, `on_change`, palette, disabled. Runs the prop-sync guard on rebuild. |
| `widget.rs` | `ColorPickerWidget` — composite masonry container. Owns canonical `hsva` + `active_tab`, holds child `WidgetPod`s, routes child actions into `hsva` updates, re-syncs all children, emits one `ColorChanged(Color)` action upward. |
| `sv_field.rs` | `SvFieldWidget` — 2D saturation/value square. Paints an S×V gradient for the current hue, draws the thumb, handles pointer drag + arrow-key nudge. Emits `(s, v)`. |
| `hue_slider.rs` | `HueSliderWidget` — vertical hue bar (0–360 gradient), thumb, drag + keyboard. Emits `h`. |
| `alpha_slider.rs` | `AlphaSliderWidget` — alpha bar painted over a themed checkerboard, tinted by current color. Emits `a`. |
| `numeric_body.rs` | Per-tab numeric editing surface: RGB fields, HSL fields, hex box. Reuses the existing `input` component for text entry; parses/validates on edit. Emits an `hsva` (or the changed channel). |
| `color_math.rs` | Pure conversions and parsing: HSV↔RGB, RGB↔HSL, RGBA↔hex (6- and 8-digit), channel clamping, gamut handling. No masonry deps beyond `Color` construction. Unit-tested. |
| `demo.rs` | Gallery panel via `with_source!`, exercising every surface. |
| `mod.rs` | Re-exports (`color_picker`, `ColorPicker`, public types). |

### Data flow

1. Host renders `color_picker(state.color, ...)`.
2. View builds `ColorPickerWidget`, seeding `hsva` from the initial `Color`.
3. User interacts with a child (SV drag / hue / alpha / numeric field / hex /
   swatch). The child emits its channel(s) as a masonry action.
4. `ColorPickerWidget` folds the change into `hsva`, re-derives dependent visuals
   (SV gradient hue, alpha tint, numeric readouts), pushes fresh values into every
   child, and emits `ColorChanged(derive_color(hsva))`.
5. View translates that into `on_change(Color)`; host updates `state.color`.
6. On the resulting rebuild, the prop-sync guard sees the incoming `Color` equals
   the widget's derived color and does nothing — no clobber.

### Tab switching

`active_tab` lives in the widget. Clicking a tab mutates it via `mutate_later`
and swaps the mounted `numeric_body` child. Switching tabs never changes `hsva`,
so the displayed color is identical across tabs.

## Theming

All chrome reads from the passed `Theme` — no hardcoded colors:

- SV field / hue / alpha borders and focus/thumb rings from `Palette`.
- Numeric field backgrounds and text via the same tokens the `input` component
  uses.
- Tab styling reuses the `tabs` component's theming.
- The alpha checkerboard uses two themed neutral tones (light/dark cells) rather
  than fixed greys, so it reads correctly in every `ThemeVariant`.
- Sizes/spacing/radii from `Density` / `Radii`.

## Events

A single widget-level action, `ColorChanged(Color)`, travels from
`ColorPickerWidget` up to the View, which invokes the host `on_change`. No other
public events. Internal child actions (SV/hue/alpha/numeric/tab) are consumed by
`ColorPickerWidget` and never surface to the host.

## Testing

**`color_math` unit tests (pure):**
- HSV↔RGB and RGB↔HSL round-trips within tolerance across sampled space.
- Hex parse: valid 6- and 8-digit, `#`-optional, case-insensitive; rejects
  malformed input without panicking.
- Grayscale hue preservation: converting a grayscale RGB back does not force hue
  to 0 when an existing hue is supplied.
- Gamut/clamp: out-of-range channel inputs clamp rather than wrap.

**Widget tests:**
- SV drag updates `hsva.s`/`hsva.v` and emits the expected `Color`.
- Hue move changes `hsva.h` while preserving `s`/`v`.
- Alpha move changes only `a`.
- External `Color` prop change syncs into `hsva` (prop-sync guard path).
- Self-echo (`on_change` result fed back as prop) does **not** reset the field.
- Tab switch preserves the derived color.
- Swatch click sets the full color.
- `disabled` inerts all sub-controls.

**Gallery:** `demo.rs` renders the picker with a host-supplied palette and a live
color readout, wrapped in `with_source!`, added to `src/gallery.rs`.

## Non-goals (YAGNI)

- No OKLCH/LCH editing tab (theme authoring uses `theme::color::oklch` directly;
  the picker targets end-user sRGB selection).
- No eyedropper / screen sampling (platform-specific, out of a presentation-only
  library).
- No color history/recents (that is host application state, not component state).
- No named-color autocomplete.

## Build order (informs the implementation plan)

1. `color_math.rs` + its unit tests (foundation, no UI).
2. `sv_field.rs`, `hue_slider.rs`, `alpha_slider.rs` (graphical child widgets).
3. `numeric_body.rs` (reuses `input`).
4. `widget.rs` composite container wiring children + `hsva` + tab switching.
5. `view.rs` builder + prop-sync guard.
6. `demo.rs` + gallery registration.
7. Widget-level tests.
