//! Input demo panel used by the void-ui gallery.

use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

use crate::label;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::input;
use crate::Theme;
use crate::components::ScrollBarVisibility;
use crate::scroll_container;
use crate::separator;
use crate::with_source;

/// Fixed field width so the demo fields line up regardless of contents.
const FIELD_WIDTH: f64 = 240.0;

#[derive(Debug, Clone, Default)]
struct InputDemo {
    name: String,
    email: String,
    amount: String,
    site: String,
}

impl InputDemo {
    fn new() -> Self {
        Self {
            name: "Ada Lovelace".to_owned(),
            email: String::new(),
            amount: "1250.00".to_owned(),
            site: String::new(),
        }
    }
}

type InnerView = Box<AnyWidgetView<InputDemo>>;
type InnerViewState = <InnerView as View<InputDemo, (), ViewCtx>>::ViewState;

/// Opaque state owned by the input demo panel.
pub struct InputDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct InputDemoPanelState {
    input: InputDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Input demo panel.
///
/// The returned view owns all field state internally; callers do not need to
/// store or lens into any input state.
#[must_use]
pub fn panel(theme: &Theme) -> InputDemoPanel {
    InputDemoPanel { theme: *theme }
}

/// A labeled field: caption above a fixed-width input.
fn field<V>(theme: &Theme, caption: &'static str, control: V) -> impl WidgetView<InputDemo> + use<V>
where
    V: WidgetView<InputDemo> + 'static,
{
    flex_col((
        label(caption)
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
        sized_box(control).fixed_width(Length::px(FIELD_WIDTH)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0))
}

fn build_inner(theme: &Theme, state: &InputDemo) -> impl WidgetView<InputDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    // A prefilled field and an empty field showing its placeholder.
    let editable = with_source!(theme, {
        flex_row((
            field(
                theme,
                "Name",
                input(state.name.clone(), |s: &mut InputDemo, text| s.name = text).render(theme),
            ),
            field(
                theme,
                "Email",
                input(state.email.clone(), |s: &mut InputDemo, text| s.email = text)
                    .placeholder("you@example.com")
                    .render(theme),
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
    });

    // Leading/trailing affixes inside the field border.
    let affixes = with_source!(theme, {
        flex_row((
            field(
                theme,
                "Amount",
                input(state.amount.clone(), |s: &mut InputDemo, text| s.amount = text)
                    .prefix("$")
                    .suffix("USD")
                    .render(theme),
            ),
            field(
                theme,
                "Website",
                input(state.site.clone(), |s: &mut InputDemo, text| s.site = text)
                    .prefix("https://")
                    .placeholder("example.com")
                    .render(theme),
            ),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0))
    });

    // A disabled field does not accept input and paints muted.
    let disabled = with_source!(theme, {
        field(
            theme,
            "Disabled",
            input("Read only", |_: &mut InputDemo, _| {})
                .disabled(true)
                .render(theme),
        )
    });

    let title_block = flex_col((
        label("Input")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Single-line text field. Contents are host-controlled: the field \
             emits the updated string on every edit and the host stores it and \
             passes it back in on the next render. Press Esc in a focused field \
             to clear it.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            header("Editable"),
            editable,
            header("Prefix & suffix"),
            affixes,
            header("Disabled"),
            disabled,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme)
}

impl ViewMarker for InputDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for InputDemoPanel {
    type ViewState = InputDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut input = InputDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &input));
        let (element, inner_state) = inner_view.build(ctx, &mut input);
        (
            element,
            InputDemoPanelState {
                input,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut InputDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.input));
        new_inner.rebuild(&vs.inner_view, &mut vs.inner_state, ctx, element, &mut vs.input);
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut InputDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut InputDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.input)
    }
}
