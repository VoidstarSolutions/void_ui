//! List demo panel used by the void-ui gallery.
//!
//! Exercises every feature from issue #58: a search box (host-side
//! filtering), click/Shift/Ctrl-Cmd selection, a "Loading" toggle (empty
//! spinner vs. footer spinner), lazy loading as the view nears the end of
//! the loaded items, and scroll-to-top / scroll-to-selected buttons.

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::masonry::layout::Length;
use xilem::masonry::widgets::Passthrough;
use xilem::style::Style as _;
use xilem::view::{CrossAxisAlignment, FlexExt as _, FlexSpacer, flex_col, flex_row, sized_box};
use xilem::{AnyWidgetView, Pod, ViewCtx, WidgetView};

use super::list;
use crate::Theme;
use crate::components::data_grid::{ScrollState, SelectionState};
use crate::label;
use crate::with_source;

// --- MARK: LOCAL STATE

/// One row of synthetic data.
#[derive(Clone)]
struct Person {
    /// Stable id (== position in [`ListDemo::all`]) — survives filtering.
    id: u64,
    name: String,
    email: String,
}

/// Number of items revealed initially and per [`ListDemo::load_more`] call.
const CHUNK: u64 = 30;

const FIRST_NAMES: &[&str] = &[
    "Ada",
    "Grace",
    "Alan",
    "Margaret",
    "Linus",
    "Barbara",
    "Donald",
    "Katherine",
    "John",
    "Edsger",
];
const LAST_NAMES: &[&str] = &[
    "Lovelace", "Hopper", "Turing", "Hamilton", "Torvalds", "Liskov", "Knuth", "Johnson", "Backus",
    "Dijkstra",
];

struct ListDemo {
    all: Vec<Person>,
    query: String,
    filtered: Vec<Person>,
    loaded: u64,
    selection: SelectionState,
    scroll: ScrollState,
    loading: bool,
}

impl ListDemo {
    fn new() -> Self {
        let all: Vec<Person> = (0..500)
            .map(|id| {
                let pos = usize::try_from(id).unwrap_or(usize::MAX);
                let first = FIRST_NAMES[pos % FIRST_NAMES.len()];
                let last = LAST_NAMES[(pos / FIRST_NAMES.len()) % LAST_NAMES.len()];
                let name = format!("{first} {last} {id}");
                let email = format!(
                    "{}.{}{id}@example.com",
                    first.to_lowercase(),
                    last.to_lowercase()
                );
                Person { id, name, email }
            })
            .collect();
        let filtered = all.clone();
        let loaded = CHUNK.min(filtered.len() as u64);
        Self {
            all,
            query: String::new(),
            filtered,
            loaded,
            selection: SelectionState::default(),
            scroll: ScrollState::new(),
            loading: false,
        }
    }

    fn set_query(&mut self, query: String) {
        let needle = query.to_lowercase();
        self.filtered = self
            .all
            .iter()
            .filter(|p| {
                needle.is_empty()
                    || p.name.to_lowercase().contains(&needle)
                    || p.email.to_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        self.loaded = CHUNK.min(self.filtered.len() as u64);
        self.query = query;
    }

    fn load_more(&mut self) {
        self.loaded = (self.loaded + CHUNK).min(self.filtered.len() as u64);
    }

    fn toggle_loading(&mut self) {
        self.loading = !self.loading;
    }

    fn scroll_to_top(&mut self) {
        self.scroll.scroll_to_index(0);
    }

    fn scroll_to_selected(&mut self) {
        let Some(selected_id) = self.selection.iter().next() else {
            return;
        };
        if let Some(pos) = self.filtered.iter().position(|p| p.id == selected_id) {
            self.scroll.scroll_to_index(u64::try_from(pos).unwrap_or(0));
        }
    }
}

type InnerView = Box<AnyWidgetView<ListDemo>>;
type InnerViewState = <InnerView as View<ListDemo, (), ViewCtx>>::ViewState;

// --- MARK: PANEL VIEW STATE

pub struct ListDemoPanelState {
    demo: ListDemo,
    inner_view: InnerView,
    inner_state: InnerViewState,
}

pub struct ListDemoPanel {
    theme: Theme,
}

// --- MARK: INNER VIEW BUILDER

fn build_toolbar(theme: &Theme, loading: bool) -> impl WidgetView<ListDemo> + use<> {
    flex_row((
        crate::components::button::button(|s: &mut ListDemo| s.toggle_loading())
            .label(if loading {
                "Loading: on"
            } else {
                "Loading: off"
            })
            .active(loading)
            .render(theme),
        crate::components::button::button(|s: &mut ListDemo| s.scroll_to_top())
            .label("Scroll to top")
            .render(theme),
        crate::components::button::button(|s: &mut ListDemo| s.scroll_to_selected())
            .label("Scroll to selected")
            .render(theme),
        crate::components::button::button(|s: &mut ListDemo| s.selection.clear())
            .label("Clear selection")
            .render(theme),
        FlexSpacer::Flex(1.0),
        label("500 items")
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(f64::from(theme.density.col)))
}

/// Renders one item: name on top, muted email beneath.
fn build_item_row(
    item: &Person,
    _selected: bool,
    theme: &Theme,
) -> impl WidgetView<ListDemo> + use<> {
    sized_box(
        flex_col((
            label(item.name.clone())
                .color(theme.palette.text)
                .render(theme),
            label(item.email.clone())
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_muted)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(f64::from(theme.density.pad) / 4.0)),
    )
    .padding(Length::px(f64::from(theme.density.pad) / 2.0))
}

fn build_inner(theme: &Theme, demo: &ListDemo) -> impl WidgetView<ListDemo> + use<> {
    let item_count = demo.loaded;
    let query = demo.query.clone();
    let loading = demo.loading;
    let scroll = demo.scroll;

    let toolbar = build_toolbar(theme, loading);

    let list_view = with_source!(theme, {
        sized_box(
            list()
                .items(|s: &ListDemo| &s.filtered[..])
                .item_count(item_count)
                .item_height(52.0)
                .render_item(build_item_row)
                .item_id(|item: &Person| item.id)
                .selection(|s: &mut ListDemo| &mut s.selection)
                .scroll_to(scroll)
                .loading(loading)
                .search(query, |s: &mut ListDemo, q| s.set_query(q))
                .search_placeholder("Search by name or email\u{2026}")
                .on_load_more(|s: &mut ListDemo| s.load_more())
                .load_threshold(10)
                .render(theme),
        )
        .fixed_height(Length::px(480.0))
    });

    flex_col((toolbar, sized_box(list_view).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(f64::from(theme.density.pad)))
}

// --- MARK: VIEW IMPL

impl ViewMarker for ListDemoPanel {}

impl<S: 'static> View<S, (), ViewCtx> for ListDemoPanel {
    type Element = Pod<Passthrough>;
    type ViewState = ListDemoPanelState;

    fn build(&self, ctx: &mut ViewCtx, _: &mut S) -> (Self::Element, Self::ViewState) {
        let mut demo = ListDemo::new();
        let inner_view: InnerView = Box::new(build_inner(&self.theme, &demo));
        let (element, inner_state) = inner_view.build(ctx, &mut demo);
        (
            element,
            ListDemoPanelState {
                demo,
                inner_view,
                inner_state,
            },
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        vs: &mut ListDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) {
        let new_inner: InnerView = Box::new(build_inner(&self.theme, &vs.demo));
        new_inner.rebuild(
            &vs.inner_view,
            &mut vs.inner_state,
            ctx,
            element,
            &mut vs.demo,
        );
        vs.inner_view = new_inner;
    }

    fn teardown(
        &self,
        vs: &mut ListDemoPanelState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Pod<Passthrough>>,
    ) {
        vs.inner_view.teardown(&mut vs.inner_state, ctx, element);
    }

    fn message(
        &self,
        vs: &mut ListDemoPanelState,
        message: &mut MessageCtx,
        element: Mut<'_, Pod<Passthrough>>,
        _: &mut S,
    ) -> MessageResult<()> {
        vs.inner_view
            .message(&mut vs.inner_state, message, element, &mut vs.demo)
    }
}

// --- MARK: PUBLIC ENTRY POINT

/// Renders the list demo panel.
#[must_use]
pub fn panel(theme: &Theme) -> ListDemoPanel {
    ListDemoPanel { theme: *theme }
}
