# `meter`: track + heat-tinted fill component

## Addendum (post-implementation): built-in label removed

After this spec was implemented and reviewed (see the implementation plan
and its progress ledger), the user asked to move the label from a centered
overlay to trailing text after the bar. Trailing-placement was already
identified during the original design conversation (see "Label placement"
in the Decisions below) as the one case that's trivially composable by the
caller via `flex_row((meter(...).render(theme), label(text).render(theme)))`
— it was exactly the case the team chose *not* to build in, precisely
because the centered-overlay case was the one that couldn't be replicated
externally. With the requirement now trailing rather than centered, that
tradeoff no longer favors a built-in feature, so `Meter::label`,
`MeterWidget`'s label child, and all associated accessibility/layout code
were removed rather than repositioned. `meter` is now purely the track+fill
primitive described below, minus every "Label"-tagged decision, field, and
test — those sections are kept below as the historical record of why the
overlay design was chosen originally, not as the current state of the code.

## Context

Issue #107. `citadel-ui`'s trade dashboard renders a SCORE column as a
horizontal bar — a faint track with a proportional, heat-tinted fill — and
the same shape is wanted for MFE/MAE, rung distribution, and filter
pass-rate readouts. Today citadel hand-rolls this from two nested
`sized_box`es (track + fill) with a `Background::Color` and a width
fraction; void_ui has no such primitive.

There is no existing component quite like it. The closest precedents,
read directly from this repo:

- `skeleton` (`src/components/skeleton/widget.rs`) is the closest
  structural match: a leaf masonry widget (no children), custom `paint`,
  plain `WidgetMut` setters, no interaction. Its `Wave` animation already
  paints a `peniko::Gradient` across a rect (`widget.rs:174-207`,
  `Gradient::new_linear(...).with_stops([...])`), proving that pattern
  works in this codebase without new dependencies.
- `slider` (`src/components/slider/widget.rs`) is the closest *visual*
  match (track + proportional fill) but is fully interactive
  (drag/keyboard/accesskit `SetValue`/`Increment`/`Decrement`) — `meter`
  needs none of that; it's presentation only.
- `button` (`src/components/button/widget.rs:72-74`) shows the convention
  for an optional text child on a custom-painted widget: an
  `Option<WidgetPod<Label>>` field, built from `masonry::widgets::Label`
  at the view layer and laid out/painted by the owning widget — not raw
  text drawn inline in `paint`.
- `status_dot` (`src/components/status_dot/view.rs`) shows the "no custom
  widget" end of the spectrum (pure `sized_box` composition) — ruled out
  here because a fixed-position gradient window and an optional centered
  label both need real `measure`/`layout`/`paint` control.
- `accesskit::Role::ProgressIndicator` exists in the `accesskit` crate and
  is unused anywhere in this codebase today (confirmed by grepping every
  `Role::*` used under `src/components`) — it is the correct role for this
  widget and free to adopt.
- `node.set_value(String)` is already used for exposing a widget's
  free-text value to assistive tech (`code_view/widget.rs:494`) —
  precedent for exposing `meter`'s optional caller-supplied label as the
  accessible value text.

## Goals

- A `void_ui::meter` component: a themed track with a fill proportional to
  a host-supplied `0.0..=1.0` fraction.
- Fill is either a flat color or a two-stop gradient, so callers (citadel)
  can drive a "heat" appearance (e.g. green→coral across the value range)
  without void_ui knowing anything about what the fraction means.
- An optional caller-supplied text label, centered over the bar.
- Follows the standard two-layer component pattern
  (`src/components/meter/{view,widget,demo,mod}.rs`) and ships a gallery
  demo panel per the `with_source!` convention.

## Non-goals

- No indeterminate/loading animation — that's `spinner`'s and
  `skeleton`'s job, not this component's.
- No vertical orientation. Every citadel use case cited in the issue is a
  horizontal dashboard bar; `slider` already owns the one place void_ui
  needed a vertical track.
- No contrast-aware/adaptive label color. The label always paints in
  `theme.palette.text`. A gradient fill could in principle make that hard
  to read at some stop combination; that's a real but separate concern,
  deferred rather than blocking v1 (no existing component in this codebase
  does adaptive text-on-fill contrast either).
- No built-in fraction→color ("heat scale") helper. void_ui stays
  domain-agnostic — the caller computes whatever color(s) their score
  scale implies and passes them in per render, exactly like `status_dot`'s
  `color: Color` parameter.
- No independent corner-radius knob. The bar is always a full pill
  (`radius = height / 2`) — standard meter/progress-bar affordance, not
  worth a second radius API alongside `height`.
- No multi-segment/stacked meters.

## Decisions

**1. Gradient is anchored to the full track, not the filled portion.**
When `fraction < 1.0` with a two-stop gradient, the gradient's color at a
given x-coordinate must be identical regardless of `fraction` — i.e. the
gradient spans `x ∈ [0, track_width]` unconditionally, and the fill rect
(`width = track_width * fraction`) is a window onto it. Concretely:

```rust
let gradient = Gradient::new_linear(Point::new(0.0, 0.0), Point::new(size.width, 0.0))
    .with_stops([from, to]);
let fill_rect = RoundedRect::from_origin_size(Point::ORIGIN, Size::new(size.width * fraction, size.height), radius);
painter.fill(fill_rect, &gradient).draw();
```

This gives a fixed "good→bad" scale a consistent meaning: a 30%-full bar
and a 90%-full bar both show *the same color* at, say, the 20%-of-track
mark. The rejected alternative — stretching the two stops across just the
filled width — would mean a barely-filled meter and a mostly-full one both
show the *full* color transition, compressed or stretched; that reads as
decorative rather than meaningful for a score/rate readout, which is what
every cited citadel use case actually is. (User-confirmed during design.)

**2. Label is an optional `WidgetPod<Label>` child, not text drawn in
`paint`.** Matches `button`'s existing convention (`widget.rs:72-74`)
rather than inventing inline text layout in a leaf widget. `Meter::label`
builds a `masonry::widgets::Label` at the view layer (colored
`theme.palette.text`), and `MeterWidget` measures/lays it out
centered over the full border box, painting it last (on top of track +
fill).

**3. No custom `Widget` avoided via `sized_box` composition (unlike
`status_dot`).** Two things `status_dot`'s pure-composition approach can't
express: (a) the fixed-full-track gradient window from Decision 1, whose
math depends on the widget's own resolved size at paint time, not a
value known at view-build time; (b) an optional centered overlay child,
which needs real `layout` placement, not `sized_box` nesting. Both need a
custom `Widget`, so `meter` gets full `widget.rs` treatment like `skeleton`
and `slider`, not `status_dot`'s shortcut.

## Design

### Public API (`view.rs`)

```rust
#[derive(Clone, Copy, PartialEq)]
pub enum MeterFill {
    Solid(Color),
    Gradient(Color, Color), // left = 0.0, right = 1.0, spans the full track (Decision 1)
}

#[must_use = "Meter does nothing until rendered with .render(&theme)"]
pub struct Meter {
    fraction: f64,
    fill: Option<MeterFill>,   // None -> Solid(theme.palette.accent) at render time
    label: Option<ArcStr>,
    height: Option<f32>,       // None -> DEFAULT_HEIGHT
    width: Option<f32>,        // None -> fill available width
}

pub fn meter(fraction: f64) -> Meter

impl Meter {
    pub fn fill(mut self, color: Color) -> Self;                    // -> Some(Solid(color))
    pub fn fill_gradient(mut self, from: Color, to: Color) -> Self;  // -> Some(Gradient(from, to))
    pub fn label(mut self, text: impl Into<ArcStr>) -> Self;
    pub fn height(mut self, px: f32) -> Self;
    pub fn width(mut self, px: f32) -> Self;

    pub fn render<State, Action>(self, theme: &Theme) -> MeterView<State, Action>
    where State: 'static, Action: 'static;
}
```

`fraction` is stored as given; clamping to `0.0..=1.0` happens once, in
`MeterWidget`, so the widget is the single source of truth for the
clamped value used by both paint and accesskit reporting.

Defaults resolved in `render`:
- `fill.unwrap_or(MeterFill::Solid(theme.palette.accent))`
- `height.unwrap_or(DEFAULT_HEIGHT)` — a fixed px constant (proposed
  `8.0`, alongside `slider`'s `TRACK_HEIGHT = 4.0` for comparison; `meter`
  is the primary visual rather than `slider`'s chrome accent, so a bit
  thicker reads better as a standalone bar). Not density-scaled, same
  reasoning `slider::TRACK_HEIGHT`'s doc comment gives: this is a visual
  sizing decision, not a spacing token.
- track color is *not* configurable: always `theme.palette.surface_2`.

Track/fill construction and the `Label` child (when `.label(...)` was
called) both happen in `render`, mirroring `ButtonView`'s pattern of
building children at the view layer and handing a `NewWidget` down into
the widget constructor.

### `MeterWidget` (`widget.rs`)

```rust
pub struct MeterWidget {
    fraction: f64,        // clamped 0.0..=1.0 on every set
    fill: MeterFill,
    label: Option<WidgetPod<Label>>,
    /// The raw text behind `label`, kept alongside the built child so
    /// `accessibility` can hand it to `node.set_value` without reading
    /// text back out of a `Label` widget.
    label_text: Option<ArcStr>,
    track_color: Color,
    height: f64,
    width: Option<f64>,
}
```

- `measure`: horizontal — `width.map_or_else(|| available, Length::px)`
  (same branch shape as `SkeletonWidget::measure`,
  `skeleton/widget.rs:151-166`); vertical — fixed `height`.
- `layout`: if `label` is `Some`, measure and center it over the full
  border box (standard center-both-axes math, no new pattern needed).
- `paint`:
  1. Track: `RoundedRect` over the full border box, radius
     `height / 2.0`, filled `track_color`.
  2. Fill: a second pill rect, width `size.width * fraction`, same
     radius, painted per Decision 1 (solid color, or a
     `Gradient::new_linear` spanning the *full* `size.width` with the
     fill rect as a window onto it).
  3. Label (if present): painted via its child pod, on top.
- `WidgetMut` setters: `set_fraction`, `set_fill`, `set_track_color`,
  `set_height`, `set_width`, plus `attach_label`/`detach_label` (mirroring
  `ButtonWidget::attach_icon`/`remove_icon`, `button/widget.rs:253-268`)
  for the optional child — each updates both `label` (the child pod) and
  `label_text` (the accessibility copy) together, so they can never
  diverge. All request repaint-only except `height`/`width`/label
  attach-detach, which request layout.
- `register_children`/`children_ids`: registers the label pod when
  present, matching `SkeletonWidget`'s empty-children shape when absent.

### Accessibility

```rust
fn accessibility_role(&self) -> Role {
    Role::ProgressIndicator
}

fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
    node.set_numeric_value(self.fraction);
    node.set_min_numeric_value(0.0);
    node.set_max_numeric_value(1.0);
    if let Some(text) = &self.label_text {
        node.set_value(text.clone());
    }
}
```

No actions are added (`node.add_action(...)`) — the widget is read-only,
unlike `slider`'s `SetValue`/`Increment`/`Decrement`.

### View wiring (`view.rs`, `MeterView`)

No custom action type, no `message` handling beyond `MessageResult::Stale`
— same shape as `SkeletonView::message`
(`skeleton/view.rs:278-287`). `build` calls `ctx.create_pod(widget)`
directly (no `with_action_widget` — nothing to register as an action
source). `rebuild` diffs each field and calls the matching `WidgetMut`
setter, following `SkeletonView::rebuild`'s field-by-field diff exactly
(`skeleton/view.rs:239-268`).

### Module layout

```
src/components/meter/
  mod.rs    — re-exports (pub mod demo behind #[cfg(feature = "gallery")], pub use view::{...})
  view.rs   — Meter builder, MeterFill, MeterView
  widget.rs — MeterWidget
  demo.rs   — gallery panel
```

`src/components/mod.rs` gains `pub mod meter;`; `src/lib.rs`'s
`components::{...}` re-export block gains `meter::{Meter, MeterFill, meter}`.

## Testing plan

Widget-level unit tests (`widget.rs`, following `SkeletonWidget`'s test
module shape):
- Mounting/laying out/painting doesn't panic across `fraction` = 0.0, 0.5,
  1.0, and out-of-range inputs (-0.5, 1.5) — asserts the clamp.
- Fill-rect width is exactly `size.width * fraction.clamp(0.0, 1.0)` at a
  few fraction values (measured directly, not just "doesn't panic").
- Gradient stop positions are fixed to the *track* width regardless of
  `fraction` (Decision 1) — construct at two different fractions and
  assert the underlying `Gradient`'s stop geometry (start/end points) is
  identical between them, only the fill rect's clipped width differs.
- Label child is present/centered when `.label(...)` is set, absent when
  it isn't; `attach_label`/`detach_label` toggle it via `WidgetMut` and
  trigger `children_changed`.
- Accessibility node reports `numeric_value`/`min`/`max` correctly, and
  `set_value` only when a label is present.

View-level unit tests (`view.rs`, following `SkeletonView`'s test module
shape):
- `.fill(color)` / `.fill_gradient(a, b)` produce the expected `MeterFill`
  variant; omitting both defaults to `Solid(theme.palette.accent)`.
- `.height(px)` / `.width(px)` override the defaults; omitting them
  resolves to `DEFAULT_HEIGHT` / fill-available-width respectively.

Gallery (`demo.rs`, gated on `feature = "gallery"`):
- A solid-fill row and a `fill_gradient(green, coral)` row at a few
  fractions, plus one with `.label("72%")`, wrapped in `with_source!` per
  every other demo panel.

## Open questions

None — the two real forks (gradient anchoring behavior, label
placement/ownership) were resolved during design with the user; see
Decisions 1 and 2.
