//! void-ui component gallery.
//!
//! Run with `cargo run -p void-ui --example gallery`.
//!
//! Left rail lists components; main pane shows the focused component's
//! demo panel. A gear button in the top-right toggles a theme panel
//! (right side) with theme/density controls plus a live reference of
//! the current palette, text samples, and density numbers.

use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, portal,
    sized_box,
};
use xilem::winit::error::EventLoopError;
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

use void_ui::components::data_grid::demo::{Demo, tick_columns};
use void_ui::components::{ComponentKind, button, data_grid, sidebar_item, sidebar_panel};
use void_ui::label;
use void_ui::layout::flex_wrap;
use void_ui::theme::{Density, Theme};

struct State {
    theme: Theme,
    focused: ComponentKind,
    theme_panel_open: bool,
    sidebar_collapsed: bool,
    data_grid: Demo,
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
    // access. `data_grid` itself reads state via the lens closures
    // it captures.
    let dg_row_count = u64::try_from(state.data_grid.ticks.len()).unwrap_or(u64::MAX);
    let dg_base_time_ns = state.data_grid.ticks.first().map_or(0, |t| t.event_ns);

    let workspace = workspace_row(
        focused,
        theme_panel_open,
        sidebar_collapsed,
        &theme,
        dg_row_count,
        dg_base_time_ns,
    );

    let outer = flex_col((topbar(theme_panel_open, &theme), workspace.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start);

    sized_box(outer).background_color(theme.palette.bg_deep)
}

fn workspace_row(
    focused: ComponentKind,
    theme_panel_open: bool,
    sidebar_collapsed: bool,
    theme: &Theme,
    dg_row_count: u64,
    dg_base_time_ns: i64,
) -> Box<AnyWidgetView<State>> {
    let sidebar_view = sidebar_panel(
        sized_box(sidebar_items(focused, theme))
            .padding(Length::px(12.0))
            .background_color(theme.palette.surface),
        |s: &mut State| s.sidebar_collapsed = !s.sidebar_collapsed,
    )
    .collapsed(sidebar_collapsed)
    .render(theme);
    let main = sized_box(main_pane(focused, theme, dg_row_count, dg_base_time_ns))
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
    flex_col((
        sidebar_item("Button", |s: &mut State| {
            s.focused = ComponentKind::Button;
        })
        .active(focused == ComponentKind::Button)
        .render(theme),
        sidebar_item("Button Group", |s: &mut State| {
            s.focused = ComponentKind::ButtonGroup;
        })
        .active(focused == ComponentKind::ButtonGroup)
        .render(theme),
        sidebar_item("Checkbox", |s: &mut State| {
            s.focused = ComponentKind::Checkbox;
        })
        .active(focused == ComponentKind::Checkbox)
        .render(theme),
        sidebar_item("Clipboard", |s: &mut State| {
            s.focused = ComponentKind::Clipboard;
        })
        .active(focused == ComponentKind::Clipboard)
        .render(theme),
        sidebar_item("Code View", |s: &mut State| {
            s.focused = ComponentKind::CodeView;
        })
        .active(focused == ComponentKind::CodeView)
        .render(theme),
        sidebar_item("Data Grid", |s: &mut State| {
            s.focused = ComponentKind::DataGrid;
        })
        .active(focused == ComponentKind::DataGrid)
        .render(theme),
        sidebar_item("Label", |s: &mut State| {
            s.focused = ComponentKind::Label;
        })
        .active(focused == ComponentKind::Label)
        .render(theme),
        sidebar_item("Radio", |s: &mut State| {
            s.focused = ComponentKind::Radio;
        })
        .active(focused == ComponentKind::Radio)
        .render(theme),
        sidebar_item("Scroll Container", |s: &mut State| {
            s.focused = ComponentKind::ScrollContainer;
        })
        .active(focused == ComponentKind::ScrollContainer)
        .render(theme),
        sidebar_item("Sidebar", |s: &mut State| {
            s.focused = ComponentKind::Sidebar;
        })
        .active(focused == ComponentKind::Sidebar)
        .render(theme),
        sidebar_item("Tooltip", |s: &mut State| {
            s.focused = ComponentKind::Tooltip;
        })
        .active(focused == ComponentKind::Tooltip)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Length::px(2.0))
}

fn main_pane(
    focused: ComponentKind,
    theme: &Theme,
    dg_row_count: u64,
    dg_base_time_ns: i64,
) -> Box<AnyWidgetView<State>> {
    match focused {
        ComponentKind::Button => Box::new(void_ui::components::button::demo::panel(theme)),
        ComponentKind::ButtonGroup => {
            Box::new(void_ui::components::button_group::demo::panel(theme))
        }
        ComponentKind::Checkbox => Box::new(void_ui::components::checkbox::demo::panel(theme)),
        ComponentKind::Clipboard => Box::new(void_ui::components::clipboard::demo::panel(theme)),
        ComponentKind::DataGrid => Box::new(data_grid_panel(theme, dg_row_count, dg_base_time_ns)),
        ComponentKind::Label => Box::new(void_ui::components::label::demo::panel(theme)),
        ComponentKind::Radio => Box::new(void_ui::components::radio::demo::panel(theme)),
        ComponentKind::ScrollContainer => {
            Box::new(void_ui::components::scroll_container::demo::panel(theme))
        }
        ComponentKind::Sidebar => Box::new(void_ui::components::sidebar::demo::panel(theme)),
        ComponentKind::CodeView => Box::new(void_ui::components::code_view::demo::panel(theme)),
        ComponentKind::Tooltip => Box::new(void_ui::components::tooltip::demo::panel(theme)),
    }
}

/// Gallery panel for the `data_grid` demo: a small toolbar plus the
/// grid itself. The toolbar's bulk-selection buttons are convenient
/// alternates to mouse selection — clicking rows (with optional
/// shift / ctrl-cmd modifiers) updates the same `SelectionState`.
fn data_grid_panel(
    theme: &Theme,
    row_count: u64,
    base_time_ns: i64,
) -> impl WidgetView<State> + use<> {
    let columns = tick_columns::<State>(base_time_ns);
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
        label(format!("{row_count} ticks"))
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted)
            .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(8.0));

    let grid = data_grid(
        columns,
        row_count,
        |s: &State| &s.data_grid.ticks[..],
        |s: &mut State| &mut s.data_grid.selection,
        &theme_copy,
        22.0,
    );

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
    );
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
