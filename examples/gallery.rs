//! void-ui component gallery.
//!
//! Run with `cargo run -p void-ui --example gallery`.
//!
//! Left rail lists components; main pane shows the focused component's
//! demo panel. A gear button in the top-right toggles a theme panel
//! (right side) with theme/density controls plus a live reference of
//! the current palette, text samples, and density numbers.

use void_ui::components::ScrollBarVisibility::OnActivity;
use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, portal,
    sized_box,
};
use xilem::winit::error::EventLoopError;
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

use void_ui::components::data_grid::demo::{
    Demo, DemoTick, StockDemo, StockQuote, arrange_columns, arrange_stock_columns,
};
use void_ui::components::{
    ColumnId, ColumnWidths, ComponentKind, FilterState, SortState, button, data_grid, sidebar_item,
    sidebar_panel,
};
use void_ui::layout::flex_wrap;
use void_ui::theme::{Density, Theme};
use void_ui::{label, scroll_container};

struct State {
    theme: Theme,
    focused: ComponentKind,
    theme_panel_open: bool,
    sidebar_collapsed: bool,
    data_grid: Demo,
    stock_quotes: StockDemo,
}

impl State {
    fn new() -> Self {
        Self {
            theme: Theme::dark(),
            focused: ComponentKind::Button,
            theme_panel_open: false,
            sidebar_collapsed: false,
            // Seed with 100k rows so virtualization is exercised on
            // first open; cheaper than a million but big enough that
            // scrolling has to be real.
            data_grid: Demo::with_initial(100_000),
            // The "value lens" board: a static NASDAQ-style snapshot.
            stock_quotes: StockDemo::new(),
        }
    }
}

fn app_logic(state: &mut State) -> impl WidgetView<State> + use<> {
    let theme = state.theme;
    let focused = state.focused;
    let theme_panel_open = state.theme_panel_open;
    let sidebar_collapsed = state.sidebar_collapsed;
    // Snapshot the data-grid demo's row count and base timestamp at
    // frame time so the panel can be built without further state
    // access. The grid reads rows via the lens closures it captures.
    // Row count reflects the materialized (filtered and/or sorted) view
    // when one is active: the host owns the row order + count.
    let dg_visible_len = if state.data_grid.view_is_materialized() {
        state.data_grid.visible.len()
    } else {
        state.data_grid.ticks.len()
    };
    let dg_row_count = u64::try_from(dg_visible_len).unwrap_or(u64::MAX);
    let dg_base_time_ns = state.data_grid.ticks.first().map_or(0, |t| t.event_ns);
    let dg_sort = state.data_grid.sort.clone();
    let dg_filter = state.data_grid.filter.clone();
    let dg_widths = state.data_grid.column_widths.clone();
    let dg_column_layout = state.data_grid.column_layout();

    // Same frame-time snapshot for the stock-quote board (its row count is
    // the visible/full quote count; no base_time_ns — quotes aren't timed).
    let sq = &state.stock_quotes;
    let sq_len = if sq.view_is_materialized() {
        sq.visible.len()
    } else {
        sq.quotes.len()
    };
    let stock = StockSnapshot {
        row_count: u64::try_from(sq_len).unwrap_or(u64::MAX),
        sort: sq.sort.clone(),
        filter: sq.filter.clone(),
        widths: sq.column_widths.clone(),
        column_layout: sq.column_layout(),
    };

    let workspace = workspace_row(
        focused,
        theme_panel_open,
        sidebar_collapsed,
        &theme,
        DataGridSnapshot {
            row_count: dg_row_count,
            base_time_ns: dg_base_time_ns,
            sort: dg_sort,
            filter: dg_filter,
            widths: dg_widths,
            column_layout: dg_column_layout,
        },
        stock,
    );

    let outer = flex_col((topbar(theme_panel_open, &theme), workspace.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start);

    sized_box(outer).background_color(theme.palette.bg_deep)
}

/// Frame-time snapshot of the data-grid demo's interaction state,
/// threaded from `app_logic` down to the grid panel. Grouped so the
/// intervening layout functions take one argument instead of five.
struct DataGridSnapshot {
    row_count: u64,
    base_time_ns: i64,
    sort: SortState,
    filter: FilterState,
    widths: ColumnWidths,
    column_layout: Vec<ColumnId>,
}

/// Frame-time snapshot of the stock-quote board's interaction state — the
/// stock-demo analogue of [`DataGridSnapshot`] (no `base_time_ns`; quotes
/// are a static snapshot, not a timed stream).
struct StockSnapshot {
    row_count: u64,
    sort: SortState,
    filter: FilterState,
    widths: ColumnWidths,
    column_layout: Vec<ColumnId>,
}

fn workspace_row(
    focused: ComponentKind,
    theme_panel_open: bool,
    sidebar_collapsed: bool,
    theme: &Theme,
    dg: DataGridSnapshot,
    stock: StockSnapshot,
) -> Box<AnyWidgetView<State>> {
    let sidebar_view = sidebar_panel(
        sized_box(sidebar_items(focused, theme))
            .padding(Length::px(12.0))
            .background_color(theme.palette.surface),
        |s: &mut State| s.sidebar_collapsed = !s.sidebar_collapsed,
    )
    .collapsed(sidebar_collapsed)
    .render(theme);
    let main = sized_box(main_pane(focused, theme, dg, stock))
        .padding(Length::px(20.0))
        .background_color(theme.palette.bg);

    if theme_panel_open {
        let panel = sized_box(
            portal(sized_box(theme_panel(theme)).padding(Length::px(16.0)))
                .constrain_horizontal(true),
        )
        .fixed_width(Length::px(360.0))
        .background_color(theme.palette.surface)
        .border(theme.palette.border, Length::px(1.0));
        Box::new(
            flex_row((sidebar_view, main.flex(1.0), panel))
                .cross_axis_alignment(CrossAxisAlignment::Stretch),
        )
    } else {
        Box::new(
            flex_row((sidebar_view, main.flex(1.0)))
                .cross_axis_alignment(CrossAxisAlignment::Stretch),
        )
    }
}

fn topbar(theme_panel_open: bool, theme: &Theme) -> impl WidgetView<State> + use<> {
    let title = label("void-ui · components")
        .text_size(16.0)
        .color(theme.palette.text)
        .render(theme);
    let subtitle = label("Tessera-styled widget library")
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_faint)
        .render(theme);
    let header = flex_col((title, subtitle))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(2.0));

    let gear = button(|s: &mut State| {
        s.theme_panel_open = !s.theme_panel_open;
    })
    .label("\u{2699} Theme")
    .active(theme_panel_open)
    .render(theme);

    let chrome = flex_row((header, FlexSpacer::Flex(1.0), gear))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0));

    sized_box(chrome)
        .padding(Length::px(12.0))
        .background_color(theme.palette.surface)
        .border(theme.palette.border, Length::px(1.0))
}

fn sidebar_items(focused: ComponentKind, theme: &Theme) -> impl WidgetView<State> + use<> {
    let items: Vec<Box<AnyWidgetView<State>>> = ComponentKind::all()
        .iter()
        .copied()
        .map(|kind| -> Box<AnyWidgetView<State>> {
            Box::new(
                sidebar_item(kind.label(), move |s: &mut State| {
                    s.focused = kind;
                })
                .active(focused == kind)
                .render(theme),
            )
        })
        .collect();
    scroll_container(
        flex_col(items)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(Length::px(2.0)),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(OnActivity)
    .render(theme)
}

fn main_pane(
    focused: ComponentKind,
    theme: &Theme,
    dg: DataGridSnapshot,
    stock: StockSnapshot,
) -> Box<AnyWidgetView<State>> {
    match focused {
        ComponentKind::Button => Box::new(void_ui::components::button::demo::panel(theme)),
        ComponentKind::ButtonGroup => {
            Box::new(void_ui::components::button_group::demo::panel(theme))
        }
        ComponentKind::Checkbox => Box::new(void_ui::components::checkbox::demo::panel(theme)),
        ComponentKind::Collapsible => {
            Box::new(void_ui::components::collapsible::demo::panel(theme))
        }
        ComponentKind::Clipboard => Box::new(void_ui::components::clipboard::demo::panel(theme)),
        ComponentKind::DataGrid => Box::new(data_grid_panel(theme, dg)),
        ComponentKind::Icon => Box::new(void_ui::components::icon::demo::panel(theme)),
        ComponentKind::Label => Box::new(void_ui::components::label::demo::panel(theme)),
        ComponentKind::StockQuotes => Box::new(stock_quotes_panel(theme, stock)),
        ComponentKind::Radio => Box::new(void_ui::components::radio::demo::panel(theme)),
        ComponentKind::ScrollContainer => {
            Box::new(void_ui::components::scroll_container::demo::panel(theme))
        }
        ComponentKind::Separator => Box::new(void_ui::components::separator::demo::panel(theme)),
        ComponentKind::Sidebar => Box::new(void_ui::components::sidebar::demo::panel(theme)),
        ComponentKind::Slider => Box::new(void_ui::components::slider::demo::panel(theme)),
        ComponentKind::Toggle => Box::new(void_ui::components::toggle::demo::panel(theme)),
        ComponentKind::CodeView => Box::new(void_ui::components::code_view::demo::panel(theme)),
        ComponentKind::Tooltip => Box::new(void_ui::components::tooltip::demo::panel(theme)),
    }
}

/// Gallery panel for the `data_grid` demo: a small toolbar plus the
/// grid itself. The toolbar's bulk-selection buttons are convenient
/// alternates to mouse selection — clicking rows (with optional
/// shift / ctrl-cmd modifiers) updates the same `SelectionState`.
/// Click a column header (Time / Price / Size) to cycle its sort. Type
/// in a column's filter input (e.g. "B"/"S" under Side) to filter the
/// rows; filtered columns show a persistent accent + marker.
fn data_grid_panel(theme: &Theme, dg: DataGridSnapshot) -> impl WidgetView<State> + use<> {
    let DataGridSnapshot {
        row_count,
        base_time_ns,
        sort,
        filter,
        widths,
        column_layout,
    } = dg;
    // Host-arranged columns: show/hide + reorder is expressed purely by
    // which ColumnDefs we hand the grid and in what order. Sort/filter/
    // width state (id-keyed) follows each column across the rearrangement.
    let columns = arrange_columns::<State>(&column_layout, base_time_ns);
    // Whether the wide "Notional" metric column is currently shown — drives
    // the toggle button's label. (Its id is its title.)
    let notional_id = ColumnId::from("Notional");
    let notional_shown = column_layout.contains(&notional_id);
    let theme_copy = *theme;

    let toolbar = flex_row((
        button(|s: &mut State| {
            s.data_grid.append_n(100);
        })
        .label("Add 100 ticks")
        .render(theme),
        button(|s: &mut State| {
            s.data_grid.append_n(10_000);
        })
        .label("Add 10k ticks")
        .render(theme),
        button(|s: &mut State| {
            s.data_grid.select_first(50);
        })
        .label("Select 0..50")
        .render(theme),
        button(|s: &mut State| {
            s.data_grid.clear_selection();
        })
        .label("Clear selection")
        .render(theme),
        FlexSpacer::Flex(1.0),
        // Column show/hide + reorder (host-owned layout): toggling
        // "Notional" hides/shows it; "Time ←" moves the Time column one
        // slot left; "Reset cols" restores natural order. Sort/filter/
        // width on a *moved* column follows it (keyed by ColumnId, not
        // position); *hiding* a column clears its filter and sort level
        // so no invisible criteria keep trimming/reordering rows.
        button(move |s: &mut State| {
            s.data_grid.toggle_column(&ColumnId::from("Notional"));
        })
        .label(if notional_shown {
            "Hide Notional"
        } else {
            "Show Notional"
        })
        .render(theme),
        button(|s: &mut State| {
            s.data_grid.move_column_left(&ColumnId::from("Price"));
        })
        .label("Price \u{2190}")
        .render(theme),
        button(|s: &mut State| {
            s.data_grid.reset_columns();
        })
        .label("Reset cols")
        .render(theme),
        FlexSpacer::Flex(1.0),
        label(format!("{row_count} rows"))
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    let grid = data_grid(columns)
        // Host owns row order: when the view is reordered (a filter is
        // active *or* a sort column is set) the lens serves the
        // materialized filtered-then-sorted view; otherwise the full tick
        // slice (zero-copy in the common, unordered case).
        .rows(|s: &State| {
            if s.data_grid.view_is_materialized() {
                &s.data_grid.visible[..]
            } else {
                &s.data_grid.ticks[..]
            }
        })
        .row_count(row_count)
        // Stable row id = the tick's monotonic seq. Selection is keyed by
        // this, so it follows rows across sort/filter reordering.
        .row_id(|t: &DemoTick| t.id)
        .selection(|s: &mut State| &mut s.data_grid.selection)
        // Host-side sorting: a header click cycles the host's SortState
        // and re-derives the ordered view (mirror of the filter callback).
        // `multi` (Shift+click) adds the column as a tiebreaker.
        .sort(sort, |s: &mut State, col: ColumnId, multi: bool| {
            s.data_grid.cycle_sort(col, multi);
        })
        .filter(filter, |s: &mut State, col: ColumnId, query: String| {
            s.data_grid.set_filter(col, query);
        })
        .column_widths(widths)
        .on_column_resize(|s: &mut State, col: ColumnId, new_width: f64| {
            s.data_grid.resize_column(col, new_width);
        })
        .row_height(22.0)
        .render(&theme_copy);

    flex_col((toolbar, sized_box(grid).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(12.0))
}

/// Gallery panel for the **stock-quote board** — the "value lens" demo:
/// the same generic grid as a NASDAQ-style symbol viewer. Click a header
/// to sort (try **Sector**, then **Shift+click Mkt Cap** for a grouped,
/// size-ranked board); filter by Symbol or Sector; the wide fundamentals
/// set scrolls horizontally. Same wiring as `data_grid_panel`, only the
/// row type + data source differ — which is the whole point.
fn stock_quotes_panel(theme: &Theme, stock: StockSnapshot) -> impl WidgetView<State> + use<> {
    let StockSnapshot {
        row_count,
        sort,
        filter,
        widths,
        column_layout,
    } = stock;
    let columns = arrange_stock_columns::<State>(&column_layout);
    // "Beta" is a low-value column to toggle for the show/hide demo.
    let beta_id = ColumnId::from("Beta");
    let beta_shown = column_layout.contains(&beta_id);
    let theme_copy = *theme;

    let toolbar = flex_row((
        label("NASDAQ symbols — static snapshot")
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
        FlexSpacer::Flex(1.0),
        // Show/hide + reorder over the host-owned column layout (id-keyed
        // sort/filter/width follow each column across the change).
        button(move |s: &mut State| {
            s.stock_quotes.toggle_column(&ColumnId::from("Beta"));
        })
        .label(if beta_shown { "Hide Beta" } else { "Show Beta" })
        .render(theme),
        button(|s: &mut State| {
            s.stock_quotes.move_column_left(&ColumnId::from("Sector"));
        })
        .label("Sector \u{2190}")
        .render(theme),
        button(|s: &mut State| {
            s.stock_quotes.reset_columns();
        })
        .label("Reset cols")
        .render(theme),
        FlexSpacer::Flex(1.0),
        label(format!("{row_count} symbols"))
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    let grid = data_grid(columns)
        .rows(|s: &State| {
            if s.stock_quotes.view_is_materialized() {
                &s.stock_quotes.visible[..]
            } else {
                &s.stock_quotes.quotes[..]
            }
        })
        .row_count(row_count)
        // Symbol is the stable, unique row id.
        .row_id(|q: &StockQuote| {
            // FNV-1a of the ticker — a stable u64 id from the &'static str.
            let mut h: u64 = 0xcbf2_9ce4_8422_2325;
            for b in q.symbol.as_bytes() {
                h ^= u64::from(*b);
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h
        })
        .selection(|s: &mut State| &mut s.stock_quotes.selection)
        .sort(sort, |s: &mut State, col: ColumnId, multi: bool| {
            s.stock_quotes.cycle_sort(col, multi);
        })
        .filter(filter, |s: &mut State, col: ColumnId, query: String| {
            s.stock_quotes.set_filter(col, query);
        })
        .column_widths(widths)
        .on_column_resize(|s: &mut State, col: ColumnId, new_width: f64| {
            s.stock_quotes.resize_column(col, new_width);
        })
        .row_height(24.0)
        .render(&theme_copy);

    flex_col((toolbar, sized_box(grid).flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .gap(Length::px(12.0))
}

// === Theme panel ===========================================================

fn theme_panel(theme: &Theme) -> impl WidgetView<State> {
    flex_col((
        section_header("Theme", theme),
        theme_variant_row(theme),
        section_header("Density", theme),
        density_row(theme),
        section_header("Surfaces", theme),
        surfaces_block(theme),
        section_header("Accents", theme),
        accents_block(theme),
        section_header("Domain", theme),
        domain_block(theme),
        section_header("Text", theme),
        text_block(theme),
        section_header("Density · Radii", theme),
        density_radii_block(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(12.0))
}

fn section_header(title: &'static str, theme: &Theme) -> impl WidgetView<State> + use<> {
    label(title)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint)
        .render(theme)
}

fn theme_variant_row(theme: &Theme) -> impl WidgetView<State> + use<> {
    flex_row((
        button(|s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::dark().with_density(d);
        })
        .label("Dark")
        .active(theme.is_dark())
        .render(theme),
        button(|s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::light().with_density(d);
        })
        .label("Light")
        .active(theme.is_light())
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(6.0))
}

fn density_row(theme: &Theme) -> impl WidgetView<State> + use<> {
    flex_row((
        button(|s: &mut State| {
            s.theme = s.theme.with_density(Density::compact());
        })
        .label("Compact")
        .active(theme.density == Density::compact())
        .render(theme),
        button(|s: &mut State| {
            s.theme = s.theme.with_density(Density::balanced());
        })
        .label("Balanced")
        .active(theme.density == Density::balanced())
        .render(theme),
        button(|s: &mut State| {
            s.theme = s.theme.with_density(Density::airy());
        })
        .label("Airy")
        .active(theme.density == Density::airy())
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(6.0))
}

fn surfaces_block(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    let row_a = flex_row((
        swatch_tile("bg", p.bg, theme),
        swatch_tile("surface", p.surface, theme),
        swatch_tile("surface_2", p.surface_2, theme),
    ))
    .gap(Length::px(6.0));
    let row_b = flex_row((
        swatch_tile("surface_hi", p.surface_hi, theme),
        swatch_tile("border", p.border, theme),
        swatch_tile("border_strong", p.border_strong, theme),
    ))
    .gap(Length::px(6.0));
    flex_col((row_a, row_b))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(6.0))
}

fn accents_block(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    flex_wrap((
        (
            swatch_tile("teal", p.teal, theme),
            swatch_tile("teal_deep", p.teal_deep, theme),
            swatch_tile("teal_soft", p.teal_soft, theme),
            swatch_tile("coral", p.coral, theme),
            swatch_tile("coral_deep", p.coral_deep, theme),
            swatch_tile("coral_soft", p.coral_soft, theme),
            swatch_tile("amber", p.amber, theme),
            swatch_tile("amber_deep", p.amber_deep, theme),
            swatch_tile("amber_soft", p.amber_soft, theme),
        ),
        (
            swatch_tile("violet", p.violet, theme),
            swatch_tile("violet_deep", p.violet_deep, theme),
            swatch_tile("violet_soft", p.violet_soft, theme),
            swatch_tile("green", p.green, theme),
            swatch_tile("green_deep", p.green_deep, theme),
            swatch_tile("green_soft", p.green_soft, theme),
            swatch_tile("blue", p.blue, theme),
            swatch_tile("blue_deep", p.blue_deep, theme),
            swatch_tile("blue_soft", p.blue_soft, theme),
        ),
    ))
    .gap(6.0)
}

fn domain_block(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    flex_row((
        swatch_tile("target", p.target, theme),
        swatch_tile("compare", p.compare, theme),
    ))
    .gap(Length::px(6.0))
}

fn text_block(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    let sample = |name: &'static str, color: Color| {
        flex_col((
            label("Aa 0123 — $184.62")
                .text_size(theme.typography.size_body)
                .color(color)
                .render(theme),
            label(name)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_faint)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(2.0))
    };
    flex_col((
        sample("text", p.text),
        sample("text_muted", p.text_muted),
        sample("text_faint", p.text_faint),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(8.0))
}

fn density_radii_block(theme: &Theme) -> impl WidgetView<State> + use<> {
    let d = theme.density;
    let r = theme.radius;
    let kv = |k: &'static str, v: String| {
        flex_row((
            label(k)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_faint)
                .render(theme),
            label(v)
                .text_size(theme.typography.size_body)
                .color(theme.palette.text)
                .render(theme),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(8.0))
    };
    flex_col((
        kv("row", format!("{:.0} px", d.row)),
        kv("col", format!("{:.0} px", d.col)),
        kv("pad", format!("{:.0} px", d.pad)),
        kv("ui_fs", format!("{:.0} px", d.ui_font_size)),
        kv("radius.s", format!("{:.0} px", r.small)),
        kv("radius.l", format!("{:.0} px", r.large)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0))
}

fn swatch_tile(name: &'static str, color: Color, theme: &Theme) -> impl WidgetView<State> + use<> {
    let block = sized_box(label("").render(theme))
        .fixed_width(Length::px(96.0))
        .fixed_height(Length::px(32.0))
        .background_color(color)
        .border(theme.palette.border, Length::px(1.0))
        .corner_radius(Length::px(f64::from(theme.radius.small)));
    let caption = label(name)
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_muted)
        .render(theme);
    flex_col((block, caption))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(2.0))
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(
        State::new(),
        app_logic,
        WindowOptions::new("void-ui · component gallery"),
    )
    .with_font(void_ui::LUCIDE_FONT_BYTES.to_vec());
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
