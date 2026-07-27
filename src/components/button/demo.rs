//! Button demo panel used by the void-ui gallery.
//!
//! Each example block is wrapped in [`crate::with_source!`] so the code
//! snippet that produced the live output is shown directly below it. Adding
//! a new variant is: add a `header`, add an example block, wrap in
//! `with_source!`.

use masonry::peniko::Color;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, FlexExt as _, flex_col, flex_row, sized_box};

use crate::label;
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::{button, content_button};
use crate::components::checkbox::checkbox;
use crate::components::{ButtonVariant, IconName, ScrollBarVisibility};
use crate::with_source;
use crate::{Theme, scroll_container, separator};

struct ButtonDemoState {
    disabled: bool,
    selected: bool,
    /// Drives the loading-spinner example. Defaults to `false`: a spinner
    /// re-arms an anim frame every tick for as long as it exists, and nothing
    /// in the paint/encode pipeline tracks damage, so leaving one running by
    /// default pinned the entire window at refresh rate the moment the gallery
    /// opened on its default Button panel.
    loading: bool,
    /// Click count for the composite-content rows — proves the *whole* row is
    /// the click target, not just some inner label.
    opens: u32,
}

type InnerView = Box<AnyWidgetView<ButtonDemoState>>;
type InnerViewState = <InnerView as View<ButtonDemoState, (), ViewCtx>>::ViewState;

/// Opaque state owned by the button demo panel.
pub struct ButtonDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct ButtonDemoPanelState {
    state: ButtonDemoState,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Button demo panel.
///
/// Includes a "Disabled" checkbox that toggles all button variants between
/// enabled and disabled. Covers all 10 named variants plus loading and
/// trailing-icon states.
#[must_use]
pub fn panel(theme: &Theme) -> ButtonDemoPanel {
    ButtonDemoPanel { theme: *theme }
}

fn variants_example(
    theme: &Theme,
    disabled_bool: bool,
    selected_bool: bool,
) -> impl WidgetView<ButtonDemoState> + use<> {
    with_source!(theme, {
        flex_row((
            button(|_: &mut ButtonDemoState| {})
                .label("Default")
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Primary")
                .variant(ButtonVariant::Primary)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Secondary")
                .variant(ButtonVariant::Secondary)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Danger")
                .variant(ButtonVariant::Danger)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Warning")
                .variant(ButtonVariant::Warning)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Success")
                .variant(ButtonVariant::Success)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Info")
                .variant(ButtonVariant::Info)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Ghost")
                .variant(ButtonVariant::Ghost)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Link")
                .variant(ButtonVariant::Link)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Text")
                .variant(ButtonVariant::Text)
                .disabled(disabled_bool)
                .selected(selected_bool)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    })
}

fn controls_row(
    theme: &Theme,
    state: &ButtonDemoState,
) -> impl WidgetView<ButtonDemoState> + use<> {
    let loading_toggle = flex_row(
        checkbox(state.loading, |s: &mut ButtonDemoState, checked: bool| {
            s.loading = checked;
        })
        .label("loading")
        .render(theme),
    );
    let disabled_toggle = flex_row(
        checkbox(state.disabled, |s: &mut ButtonDemoState, checked: bool| {
            s.disabled = checked;
        })
        .label("disabled_bool")
        .render(theme),
    );
    let selected_toggle = flex_row(
        checkbox(state.selected, |s: &mut ButtonDemoState, checked: bool| {
            s.selected = checked;
        })
        .label("selected_bool")
        .render(theme),
    );
    flex_row((disabled_toggle, selected_toggle, loading_toggle))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
}

fn icons_section(
    theme: &Theme,
    disabled: bool,
    loading: bool,
) -> impl WidgetView<ButtonDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let loading_example = with_source!(theme, {
        flex_row((
            button(|_: &mut ButtonDemoState| {})
                .label("Saving…")
                .variant(ButtonVariant::Primary)
                .loading(loading)
                .disabled(disabled)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .label("Adding")
                .icon(IconName::Plus)
                .variant(ButtonVariant::Primary)
                .loading(loading)
                .disabled(disabled)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });
    let leading_icon_example = with_source!(theme, {
        flex_row((button(|_: &mut ButtonDemoState| {})
            .label("Create")
            .icon(IconName::Plus)
            .disabled(disabled)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });
    let trailing_icon_example = with_source!(theme, {
        flex_row((button(|_: &mut ButtonDemoState| {})
            .label("More options")
            .trailing_icon(IconName::ChevronDown)
            .disabled(disabled)
            .render(theme),))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });
    let icon_only_example = with_source!(theme, {
        flex_row((
            button(|_: &mut ButtonDemoState| {})
                .icon(IconName::Plus)
                .accessible_name("Add")
                .disabled(disabled)
                .render(theme),
            button(|_: &mut ButtonDemoState| {})
                .trailing_icon(IconName::ChevronDown)
                .accessible_name("Open menu")
                .disabled(disabled)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    });

    flex_col((
        header("Loading — spinner, interaction blocked (toggle above)"),
        loading_example,
        header("Leading icon"),
        leading_icon_example,
        header("Trailing icon"),
        trailing_icon_example,
        header("Icon only"),
        icon_only_example,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

/// `content_button` — the whole composite is the button.
///
/// The demo width is fixed so the buttons are wider than their content, which
/// is the only condition under which fill-vs-center is visible: the filled rows
/// keep their columns aligned (the middle column is `.flex(1.0)`), the centered
/// one collapses to its natural width.
/// Mutes `color` to the faint text token when the button is disabled.
///
/// `content_button`'s child is an arbitrary view that owns its own colors, so
/// `.disabled(..)` mutes the *button's* chrome but cannot reach into the child
/// to mute its text — the caller has to pass an already-muted child (see the
/// `content` module docs). Honoring that contract is the whole point of these
/// examples: `Ghost` paints a transparent background when disabled, so a child
/// that ignores `disabled` would leave the toggle with no visible effect at all.
fn muted(theme: &Theme, disabled: bool, color: Color) -> Color {
    if disabled {
        theme.palette.text_faint
    } else {
        color
    }
}

fn composite_rows_example(
    theme: &Theme,
    disabled: bool,
    selected: bool,
) -> impl WidgetView<ButtonDemoState> + use<> {
    let symbol_row = |sym: &'static str, name: &'static str, change: &'static str| {
        flex_row((
            label(sym)
                .color(muted(theme, disabled, theme.palette.text))
                .render(theme),
            label(name)
                .color(muted(theme, disabled, theme.palette.text_muted))
                .render(theme)
                .flex(1.0),
            label(change)
                .color(muted(theme, disabled, theme.palette.success))
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    };

    with_source!(theme, {
        flex_col((
            sized_box(
                content_button(
                    symbol_row("AAPL", "Apple Inc.", "+1.24%"),
                    |s: &mut ButtonDemoState| {
                        s.opens += 1;
                    },
                )
                .variant(ButtonVariant::Ghost)
                .accessible_name("Open AAPL")
                .disabled(disabled)
                .selected(selected)
                .render(theme),
            )
            .fixed_width(Length::px(360.0)),
            sized_box(
                content_button(
                    symbol_row("MSFT", "Microsoft Corp.", "+0.63%"),
                    |s: &mut ButtonDemoState| {
                        s.opens += 1;
                    },
                )
                .variant(ButtonVariant::Ghost)
                .accessible_name("Open MSFT")
                .disabled(disabled)
                .selected(selected)
                .render(theme),
            )
            .fixed_width(Length::px(360.0)),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(4.0))
    })
}

fn composite_section(
    theme: &Theme,
    state: &ButtonDemoState,
) -> impl WidgetView<ButtonDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };
    let disabled = state.disabled;
    let selected = state.selected;

    let rows_example = composite_rows_example(theme, disabled, selected);

    let centered_example = with_source!(theme, {
        sized_box(
            content_button(
                flex_row((
                    // The caller mutes the child when disabled — the button's
                    // `disabled` cannot reach into an arbitrary child view.
                    label("Add symbol")
                        .color(muted(theme, disabled, theme.palette.text))
                        .render(theme),
                    label("Ctrl+K")
                        .color(theme.palette.text_faint)
                        .render(theme),
                ))
                .gap(Length::px(8.0)),
                |s: &mut ButtonDemoState| {
                    s.opens += 1;
                },
            )
            .fill_content(false)
            .accessible_name("Add symbol")
            .disabled(disabled)
            .render(theme),
        )
        .fixed_width(Length::px(360.0))
    });

    flex_col((
        header("Composite rows — flexed columns fill the button width"),
        label(
            "Toggle disabled_bool: the child owns its own colors, so the caller mutes the child's \
             text — the button's `disabled` only mutes its own chrome.",
        )
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_faint)
        .multiline(true)
        .render(theme),
        rows_example,
        header("fill_content(false) — content centered at its natural width"),
        centered_example,
        label(format!("Rows opened: {}", state.opens))
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0))
}

fn build_inner(theme: &Theme, state: &ButtonDemoState) -> impl WidgetView<ButtonDemoState> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Button")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label("Themed, interactive button with style variants, disabled/selected states, and an optional loading spinner.")
            .color(theme.palette.text_muted)
            .multiline(true)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let variants_section = flex_col((
        header("Variants"),
        variants_example(theme, state.disabled, state.selected),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(16.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            controls_row(theme, state),
            variants_section,
            icons_section(theme, state.disabled, state.loading),
            composite_section(theme, state),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for ButtonDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ButtonDemoPanel {
    type ViewState = ButtonDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = ButtonDemoState {
            disabled: false,
            selected: false,
            loading: false,
            opens: 0,
        };
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (
            element,
            ButtonDemoPanelState {
                state,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ButtonDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.state));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.state,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut ButtonDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ButtonDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}

#[cfg(test)]
mod tests {
    use masonry::app::VisualLayerKind;
    use masonry::imaging::record::Scene;
    use masonry::testing::TestHarness;
    use xilem::ViewCtx;
    use xilem::core::View as _;

    use crate::test_support;
    use crate::theme::Theme;

    /// Frames driven with no input; ~1s at 60Hz.
    const IDLE_FRAMES: usize = 60;
    /// Trailing frames that must be completely still.
    const LATE_WINDOW: usize = 10;

    fn frame_scenes(harness: &mut TestHarness<masonry::widgets::Passthrough>) -> Vec<Scene> {
        let (plan, _) = harness.redraw();
        plan.layers
            .iter()
            .filter_map(|layer| match &layer.kind {
                VisualLayerKind::Scene(scene) => Some(scene.clone()),
                VisualLayerKind::External { .. } => None,
            })
            .collect()
    }

    /// The button demo shipped two permanently-`loading(true)` buttons, and
    /// `Button` is the gallery's default panel — so simply launching the gallery
    /// left a `SpinnerWidget` re-arming an anim frame forever. Since neither
    /// masonry nor vello tracks damage, each of those frames re-encoded and
    /// re-resolved the whole window, which is what put `render_to_texture` at
    /// the top of the profile before anyone touched anything.
    ///
    /// The loading state is still demonstrable — it is behind a toggle now —
    /// but the panel's *resting* state has to let the app go to sleep.
    #[test]
    fn the_panel_is_completely_still_when_left_alone() {
        let theme = Theme::default();
        let view = super::panel(&theme);
        let mut ctx = ViewCtx::new(
            test_support::noop_proxy(),
            test_support::current_thread_runtime(),
        );
        let mut host = ();
        let (pod, _vs) = view.build(&mut ctx, &mut host);
        let mut harness =
            TestHarness::create(masonry::theme::default_property_set(), pod.new_widget);

        let mut prev = frame_scenes(&mut harness);
        let mut last_change = None;
        for frame in 0..IDLE_FRAMES {
            harness.animate_ms(16);
            let next = frame_scenes(&mut harness);
            if next != prev {
                last_change = Some(frame);
                prev = next;
            }
        }

        if let Some(frame) = last_change {
            assert!(
                frame < IDLE_FRAMES - LATE_WINDOW,
                "the button panel was still repainting at frame {frame} of {IDLE_FRAMES} with \
                 no input — something in it never stops requesting anim frames, which pins the \
                 whole window at refresh rate"
            );
        }
    }
}
