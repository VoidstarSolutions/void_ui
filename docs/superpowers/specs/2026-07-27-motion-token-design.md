# theme-level motion token for skeleton, spinner, and anywhere else required

## Context

Issue #142. Filed as a follow-on to the skeleton work (#127/PR #209): "Skeleton
pulse/wave animate unconditionally; the only opt-out is per-instance
`.animated(false)`. `prefers-reduced-motion` / WCAG 2.3.3 wants this
suppressible. **This is a codebase-wide gap — `spinner` has the same
behavior** — so not fair to pin on this PR; worth filing an issue for a
theme-level motion token both components honor."

`void_ui` is presentation-only (`CLAUDE.md`) — it never queries the OS or
platform toolkit itself, so there is no way for it to detect
`prefers-reduced-motion` directly. The host application detects the OS/browser
setting and maps it onto `Theme`, the same way it already owns swapping
`ThemeVariant`/`Density`.

A repo-wide grep for `request_anim_frame` turns up more self-driving-animation
widgets than just skeleton and spinner: `tooltip`, `notification`,
`scroll_container`, `clipboard`, `animated_clip`, and `overlay/binding.rs`.
Those all drive one-shot "animate until settled" interaction transitions
(fade/slide to a target, via `anim::advance_toward`), not continuous
decorative loops — a different tradeoff than what #142 describes, and not
something the issue asks for. They're listed under Non-goals rather than
silently ignored.

## Goals

- A `Motion` theme token (`theme.motion.reduced: bool`) that a host app sets
  once, at the theme root, to signal a reduced-motion preference — mirroring
  how `Density`/`ThemeVariant` are swapped today.
- `skeleton` honors it: when set, decorative animation (pulse/wave) is forced
  off, **unconditionally** — overriding any per-instance `.wave()` /
  `.animated(true)` choice. No escape hatch, because neither skeleton
  animation conveys information beyond "content is loading."
- The behavior is documented clearly enough that a future component adding a
  continuous decorative loop knows to read `theme.motion.reduced` too.

## Non-goals

- **`spinner` does not honor the token.** Its rotation is the only signal it
  is a `ProgressIndicator` — WCAG 2.3.3's own carve-out is for motion that is
  not "essential," and removing a spinner's only tell that work is ongoing
  fails that test. This is a deliberate, documented exemption
  (User-confirmed during design), not an oversight — despite the issue's
  framing that skeleton and spinner have "the same behavior," the fix
  distinguishes decorative-loop motion (skeleton) from functionally-essential
  motion (spinner).
- **No wiring for `tooltip`, `notification`, `scroll_container`, `clipboard`,
  `animated_clip`, or `overlay/binding.rs`.** These are one-shot
  interaction-triggered transitions, not unconditional decorative loops — the
  problem #142 describes. Out of scope for this change; flagged as future
  candidates if a similar issue is filed against one of them specifically.
- **No OS-level detection.** `void_ui` never queries
  `prefers-reduced-motion` itself; the host app is responsible for detecting
  it and calling `Theme::with_motion`.
- **No per-instance override of a theme's reduced-motion setting.** Once
  `theme.motion.reduced` is true, there is no builder method to force
  animation back on for one instance. If a real "essential" exception ever
  shows up, it can be added later without breaking this API (an additive
  builder method), but no current use case needs it.
- **No duration-scaling or partial-motion tokens** (e.g. "slow everything
  down 50%" instead of stopping it). `Motion` is a binary reduced/full switch,
  matching the binary nature of the CSS/OS `prefers-reduced-motion` feature it
  mirrors.

## Decisions

**1. `Motion` is a dedicated theme token type (`src/theme/motion.rs`), not a
bare `bool` field on `Theme`.** Mirrors the existing `Density`/`Radii`/
`Palette` pattern — a small `Copy` struct with named constructors — rather
than a raw `Theme.reduced_motion: bool`. Costs one extra small file today, but
keeps `Motion` extensible (e.g. a future duration-scale field) without ever
changing `Theme`'s own field list. (User-confirmed during design.)

**2. `theme.motion.reduced` always overrides any per-instance animation
choice on the components that honor it.** Rejected the alternative (an
explicit per-instance opt-in like `.wave()` still animating through reduced
motion, with a separate override knob to force it) — accessibility settings
should trump decorative defaults unconditionally, and there's no cited case
of a skeleton animation that's load-bearing enough to need one. (User
-confirmed during design.)

**3. `spinner` is exempted rather than gated.** Considered gating it the same
way as skeleton (freeze at a static arc when reduced), which would have kept
the issue's "both components honor it" framing literally intact. Rejected:
freezing the one visual cue that a `ProgressIndicator` is doing anything
removes information a user needs, which is exactly what WCAG 2.3.3's
"unless essential" clause exists to prevent. (User-confirmed during design.)

## Design

### `src/theme/motion.rs` (new)

```rust
//! Motion preference — whether decorative animation should play.
//!
//! `void_ui` is presentation-only and never queries the OS's
//! `prefers-reduced-motion` (or platform equivalent) itself. Host apps detect
//! that preference and map it onto [`Motion`] before constructing or
//! mutating a [`crate::Theme`], the same way they already own swapping
//! [`crate::theme::Density`] or [`crate::theme::ThemeVariant`].
//!
//! Components that read this token must treat `reduced: true` as an
//! unconditional override of any per-instance animation choice — see
//! `Skeleton::render` for the reference implementation. Not every animated
//! widget honors it: [`crate::components::spinner`]'s rotation is its only
//! signal that work is in progress, so it is deliberately exempt (WCAG
//! 2.3.3's "unless essential" carve-out).

/// Motion tokens components consult before playing decorative animation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Motion {
    /// When `true`, components that honor this token suppress decorative
    /// animation regardless of any per-instance choice.
    pub reduced: bool,
}

impl Motion {
    /// Full motion — decorative animation plays normally. The default.
    #[must_use]
    pub const fn full() -> Self {
        Self { reduced: false }
    }

    /// Reduced motion — components that honor this token suppress
    /// decorative animation.
    #[must_use]
    pub const fn reduced() -> Self {
        Self { reduced: true }
    }
}
```

### `src/theme/mod.rs` (changed)

- `mod motion;` / `pub use motion::Motion;`, alongside the existing
  `Density`/`Radii` exports.
- `Theme` gains `pub motion: Motion`.
- `Theme::dark()` / `Theme::light()` set `motion: Motion::default()`
  (`Motion::full()`).
- New builder, mirroring `with_density`:

```rust
/// Replace the motion preference, keeping palette / density / typography /
/// radii.
#[must_use]
pub fn with_motion(mut self, motion: Motion) -> Self {
    self.motion = motion;
    self
}
```

### `src/lib.rs` (changed)

Add `Motion` to the existing re-export line:

```rust
pub use theme::{CodePalette, Density, FontStack, Motion, Palette, Radii, Theme, ThemeVariant, Typography};
```

### `src/components/skeleton/view.rs` (changed)

`Skeleton::render` resolves the effective animation before building the view,
instead of passing `self.animation` straight through:

```rust
pub fn render(self, theme: &Theme) -> SkeletonView {
    let animation = if theme.motion.reduced {
        SkeletonAnimation::None
    } else {
        self.animation
    };
    let color = /* ...unchanged... */;
    let highlight = /* ...unchanged... */;
    SkeletonView {
        color,
        highlight,
        radius: /* ...unchanged... */,
        circle: /* ...unchanged... */,
        width: self.width,
        height: /* ...unchanged... */,
        animation,
    }
}
```

No widget (`SkeletonWidget`) or `SkeletonView::rebuild` changes are needed:
`rebuild` already diffs `self.animation != prev.animation` and calls
`SkeletonWidget::set_animation`, which already starts/stops the anim-frame
loop correctly. Since `render()` is called fresh on every rebuild with the
live `Theme`, toggling `theme.motion.reduced` at the host-app root and
rebuilding is sufficient to freeze/unfreeze every skeleton in the tree — no
widget needs to know about `Theme` directly, preserving the existing
theme-propagation model (`CLAUDE.md`).

Doc comment on `Skeleton::animated` (and the module doc's example) gets a
one-line addition noting `theme.motion.reduced` overrides this per-instance
setting.

### `src/components/spinner/view.rs` (changed, docs only)

Add a short note to the module doc explaining the exemption, so it reads as
a decision rather than a gap:

```rust
//! Note: unlike `skeleton`, `spinner` does not honor
//! `theme.motion.reduced` — its rotation is the only signal it is a
//! progress indicator, so freezing it under reduced motion would remove
//! information rather than just decoration. See
//! `docs/superpowers/specs/2026-07-27-motion-token-design.md`.
```

No field, builder, or widget changes to `spinner`.

### `examples/gallery.rs` (changed)

Add a "Motion" section to `theme_panel`, next to "Density", following the
exact `density_row` idiom:

```rust
fn theme_panel(theme: &Theme) -> impl WidgetView<State> {
    flex_col((
        section_header("Theme", theme),
        theme_variant_row(theme),
        section_header("Density", theme),
        density_row(theme),
        section_header("Motion", theme),
        motion_row(theme),
        // ...unchanged sections follow...
    ))
    // ...
}

fn motion_row(theme: &Theme) -> impl WidgetView<State> + use<> {
    flex_row((
        button(|s: &mut State| {
            s.theme = s.theme.with_motion(Motion::full());
        })
        .label("Full")
        .selected(!theme.motion.reduced)
        .render(theme),
        button(|s: &mut State| {
            s.theme = s.theme.with_motion(Motion::reduced());
        })
        .label("Reduced")
        .selected(theme.motion.reduced)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(6.0))
}
```

Toggling "Reduced" freezes every skeleton pulse/wave in the gallery's
existing skeleton demo section while the spinner demo keeps spinning —
visual, live confirmation of the split behavior without touching either
demo panel's own code.

## Testing plan

- `src/theme/motion.rs`: unit tests that `Motion::full()` /
  `Motion::default()` have `reduced == false`, and `Motion::reduced()` has
  `reduced == true`.
- `src/theme/mod.rs`: extend (or add alongside) the existing
  `radii_default_stack_has_tiny_token`-style test to assert
  `Theme::dark().motion == Motion::full()` and same for `Theme::light()`.
- `src/components/skeleton/view.rs`: new test asserting
  `theme.motion.reduced` forces `SkeletonAnimation::None` even when the
  builder explicitly chose `.wave()` or `.animated(true)` — mirroring the
  existing `animated_true_preserves_an_explicit_animation` test's structure,
  but with a `Motion::reduced()` theme instead of a builder call.
- No new widget-level tests: `SkeletonWidget`'s existing
  `mounts_and_paints_without_panicking` test already covers all three
  `SkeletonAnimation` variants; the view-layer resolution is what's new.
- No `spinner` test changes — behavior is unchanged by this design.

## Open questions

None — token shape/location, override precedence, and the spinner exemption
were all resolved during design with the user.
