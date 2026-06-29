//! Autocomplete demo panel used by the void-ui gallery.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use crate::components::ScrollBarVisibility;
use crate::components::autocomplete::autocomplete;
use crate::label;
use crate::overlay_scope::overlay_scope;
use crate::scroll_container;
use crate::with_source;
use crate::{Theme, separator};

/// Fixed field width so all demo fields line up.
const FIELD_WIDTH: f64 = 280.0;

static COUNTRIES: &[&str] = &[
    "Afghanistan", "Albania", "Algeria", "Andorra", "Angola",
    "Argentina", "Armenia", "Australia", "Austria", "Azerbaijan",
    "Bahamas", "Bahrain", "Bangladesh", "Belarus", "Belgium",
    "Belize", "Benin", "Bhutan", "Bolivia", "Bosnia",
    "Botswana", "Brazil", "Brunei", "Bulgaria", "Burkina Faso",
    "Burundi", "Cambodia", "Cameroon", "Canada", "Chad",
    "Chile", "China", "Colombia", "Comoros", "Croatia",
    "Cuba", "Cyprus", "Czech Republic", "Denmark", "Djibouti",
    "Ecuador", "Egypt", "Estonia", "Ethiopia", "Finland",
    "France", "Georgia", "Germany", "Ghana", "Greece",
    "Guatemala", "Honduras", "Hungary", "Iceland", "India",
    "Indonesia", "Iran", "Iraq", "Ireland", "Israel",
    "Italy", "Jamaica", "Japan", "Jordan", "Kazakhstan",
    "Kenya", "Kuwait", "Kyrgyzstan", "Laos", "Latvia",
    "Lebanon", "Libya", "Lithuania", "Luxembourg", "Madagascar",
    "Malaysia", "Mali", "Malta", "Mexico", "Moldova",
    "Monaco", "Mongolia", "Montenegro", "Morocco", "Mozambique",
    "Nepal", "Netherlands", "New Zealand", "Nicaragua", "Niger",
    "Nigeria", "Norway", "Oman", "Pakistan", "Panama",
    "Paraguay", "Peru", "Philippines", "Poland", "Portugal",
    "Qatar", "Romania", "Russia", "Rwanda", "Saudi Arabia",
    "Senegal", "Serbia", "Singapore", "Slovakia", "Slovenia",
    "Somalia", "South Africa", "South Korea", "Spain", "Sri Lanka",
    "Sudan", "Sweden", "Switzerland", "Syria", "Taiwan",
    "Tajikistan", "Tanzania", "Thailand", "Togo", "Tunisia",
    "Turkey", "Uganda", "Ukraine", "United Arab Emirates",
    "United Kingdom", "United States", "Uruguay", "Uzbekistan",
    "Venezuela", "Vietnam", "Yemen", "Zimbabwe",
];

static FRUITS: &[&str] = &[
    "Apple", "Apricot", "Avocado", "Banana", "Blackberry",
    "Blueberry", "Cherry", "Coconut", "Cranberry", "Date",
    "Fig", "Grape", "Grapefruit", "Guava", "Kiwi",
    "Lemon", "Lime", "Lychee", "Mango", "Melon",
    "Nectarine", "Orange", "Papaya", "Passion fruit", "Peach",
    "Pear", "Pineapple", "Plum", "Pomegranate", "Raspberry",
    "Strawberry", "Tangerine", "Watermelon",
];

#[derive(Default, Debug)]
struct AutocompleteDemo {
    country: String,
    fruit: String,
    last: String,
}

type InnerView = Box<AnyWidgetView<AutocompleteDemo>>;
type InnerViewState = <InnerView as View<AutocompleteDemo, (), ViewCtx>>::ViewState;

/// Opaque state for the autocomplete gallery panel.
pub struct AutocompleteDemoPanel {
    theme: Theme,
}

#[doc(hidden)]
pub struct AutocompleteDemoPanelState {
    state: AutocompleteDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

/// Renders the Autocomplete demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> AutocompleteDemoPanel {
    AutocompleteDemoPanel { theme: *theme }
}

fn labeled_field<V>(
    theme: &Theme,
    caption: &'static str,
    control: V,
) -> impl WidgetView<AutocompleteDemo> + use<V>
where
    V: WidgetView<AutocompleteDemo> + 'static,
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

fn build_inner(theme: &Theme, state: &AutocompleteDemo) -> impl WidgetView<AutocompleteDemo> + use<> {
    let header = |text: &'static str| {
        label(text)
            .text_size(theme.typography.size_caption)
            .letter_spacing(1.2)
            .color(theme.palette.text_faint)
            .render(theme)
    };

    let title_block = flex_col((
        label("Autocomplete")
            .text_size(theme.typography.size_title)
            .color(theme.palette.text)
            .render(theme),
        label(
            "Text field with a filtered suggestion list. \
             Type to filter; arrow keys navigate; Enter or click selects; \
             Escape or focus-loss closes the list.",
        )
        .color(theme.palette.text_muted)
        .multiline(true)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0));

    let last_label = label(format!("Last value: \"{}\"", state.last))
        .color(theme.palette.text_muted)
        .render(theme);

    let country_field = with_source!(theme, {
        labeled_field(
            theme,
            "Country",
            autocomplete(state.country.clone(), |s: &mut AutocompleteDemo, text: String| {
                s.last.clone_from(&text);
                s.country = text;
            })
            .suggestions(COUNTRIES.iter().copied())
            .placeholder("Type a country…")
            .render(theme),
        )
    });

    let fruit_field = with_source!(theme, {
        labeled_field(
            theme,
            "Fruit",
            autocomplete(state.fruit.clone(), |s: &mut AutocompleteDemo, text: String| {
                s.last.clone_from(&text);
                s.fruit = text;
            })
            .suggestions(FRUITS.iter().copied())
            .placeholder("Type a fruit…")
            .render(theme),
        )
    });

    let disabled_field = with_source!(theme, {
        labeled_field(
            theme,
            "Disabled",
            autocomplete("Read only", |_: &mut AutocompleteDemo, _| {})
                .suggestions(["Option A", "Option B"])
                .disabled(true)
                .render(theme),
        )
    });

    let inner = scroll_container(
        flex_col((
            title_block,
            separator().render(theme),
            last_label,
            header("With suggestions"),
            flex_row((country_field, fruit_field))
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .gap(Length::px(16.0)),
            header("Disabled"),
            disabled_field,
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(16.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(ScrollBarVisibility::OnActivity)
    .render(theme);

    overlay_scope(inner)
}

impl ViewMarker for AutocompleteDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for AutocompleteDemoPanel {
    type ViewState = AutocompleteDemoPanelState;
    type Element = Pod<Passthrough>;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut state = AutocompleteDemo::default();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &state));
        let (element, inner_state) = inner_view.build(ctx, &mut state);
        (element, AutocompleteDemoPanelState { state, inner_view, inner_state })
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut AutocompleteDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.state));
        new_inner.rebuild(&vs.inner_view, &mut vs.inner_state, ctx, element, &mut vs.state);
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut AutocompleteDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut AutocompleteDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.state)
    }
}
