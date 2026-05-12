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
    CrossAxisAlignment, FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, label,
    portal, sized_box,
};
use xilem::winit::error::EventLoopError;
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

use void_ui::components::data_grid::demo::{Demo, tick_columns};
use void_ui::components::{ComponentKind, button, data_grid};
use void_ui::layout::flex_wrap;
use void_ui::theme::{Density, Theme};

struct State {
    theme: Theme,
    focused: ComponentKind,
    theme_panel_open: bool,
    data_grid: Demo,
}

impl State {
    fn new() -> Self {
        Self {
            theme: Theme::dark(),
            focused: ComponentKind::Button,
            theme_panel_open: false,
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
    // Snapshot the data-grid demo's row count and base timestamp at
    // frame time so the panel can be built without further state
    // access. `data_grid` itself reads state via the lens closures
    // it captures.
    let dg_row_count = u64::try_from(state.data_grid.ticks.len()).unwrap_or(u64::MAX);
    let dg_base_time_ns = state
        .data_grid
        .ticks
        .first()
        .map_or(0, |t| t.timestamps.event.0);

    let workspace = workspace_row(
        focused,
        theme_panel_open,
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
    theme: &Theme,
    dg_row_count: u64,
    dg_base_time_ns: i64,
) -> Box<AnyWidgetView<State>> {
    let sidebar_view = sized_box(sidebar(focused, theme))
        .fixed_width(Length::px(180.0))
        .padding(12.0)
        .background_color(theme.palette.surface);
    let main = sized_box(main_pane(focused, theme, dg_row_count, dg_base_time_ns))
        .padding(20.0)
        .background_color(theme.palette.bg);

    if theme_panel_open {
        let panel = sized_box(
            portal(sized_box(theme_panel(theme)).padding(16.0)).constrain_horizontal(true),
        )
        .fixed_width(Length::px(360.0))
        .background_color(theme.palette.surface)
        .border(theme.palette.border, 1.0);
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
        .color(theme.palette.text);
    let subtitle = label("Tessera-styled widget library")
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_faint);
    let header = flex_col((title, subtitle))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(2.0));

    let gear = button("\u{2699} Theme", |s: &mut State| {
        s.theme_panel_open = !s.theme_panel_open;
    })
    .active(theme_panel_open)
    .render(theme);

    let chrome = flex_row((header, FlexSpacer::Flex(1.0), gear))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0));

    sized_box(chrome)
        .padding(12.0)
        .background_color(theme.palette.surface)
        .border(theme.palette.border, 1.0)
}

fn sidebar(focused: ComponentKind, theme: &Theme) -> impl WidgetView<State> + use<> {
    flex_col((
        button("Button", |s: &mut State| {
            s.focused = ComponentKind::Button;
        })
        .active(focused == ComponentKind::Button)
        .render(theme),
        button("Data Grid", |s: &mut State| {
            s.focused = ComponentKind::DataGrid;
        })
        .active(focused == ComponentKind::DataGrid)
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0))
}

fn main_pane(
    focused: ComponentKind,
    theme: &Theme,
    dg_row_count: u64,
    dg_base_time_ns: i64,
) -> Box<AnyWidgetView<State>> {
    match focused {
        ComponentKind::Button => Box::new(void_ui::components::button::demo::panel(theme)),
        ComponentKind::DataGrid => Box::new(data_grid_panel(theme, dg_row_count, dg_base_time_ns)),
    }
}

/// Gallery panel for the `data_grid` demo: a small toolbar plus the
/// grid itself. The toolbar exercises selection programmatically (no
/// modifier-aware row-click widget yet) so the clipboard path can be
/// validated with Ctrl/Cmd+C.
fn data_grid_panel(
    theme: &Theme,
    row_count: u64,
    base_time_ns: i64,
) -> impl WidgetView<State> + use<> {
    let columns = tick_columns::<State>(base_time_ns);
    let theme_copy = *theme;

    let toolbar = flex_row((
        button("Add 100 ticks", |s: &mut State| {
            s.data_grid.append_n(100);
        })
        .render(theme),
        button("Add 10k ticks", |s: &mut State| {
            s.data_grid.append_n(10_000);
        })
        .render(theme),
        button("Select 0..50", |s: &mut State| {
            s.data_grid.select_first(50);
        })
        .render(theme),
        button("Clear selection", |s: &mut State| {
            s.data_grid.clear_selection();
        })
        .render(theme),
        FlexSpacer::Flex(1.0),
        label(format!("{row_count} ticks"))
            .text_size(theme.typography.size_caption)
            .color(theme.palette.text_muted),
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

fn theme_panel(theme: &Theme) -> impl WidgetView<State> + use<> {
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
}

fn theme_variant_row(theme: &Theme) -> impl WidgetView<State> + use<> {
    flex_row((
        button("Dark", |s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::dark().with_density(d);
        })
        .active(theme.is_dark())
        .render(theme),
        button("Light", |s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::light().with_density(d);
        })
        .active(theme.is_light())
        .render(theme),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .gap(Length::px(6.0))
}

fn density_row(theme: &Theme) -> impl WidgetView<State> + use<> {
    flex_row((
        button("Compact", |s: &mut State| {
            s.theme = s.theme.with_density(Density::compact());
        })
        .active(theme.density == Density::compact())
        .render(theme),
        button("Balanced", |s: &mut State| {
            s.theme = s.theme.with_density(Density::balanced());
        })
        .active(theme.density == Density::balanced())
        .render(theme),
        button("Airy", |s: &mut State| {
            s.theme = s.theme.with_density(Density::airy());
        })
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
        swatch_tile("teal", p.teal, theme),
        swatch_tile("coral", p.coral, theme),
        swatch_tile("amber", p.amber, theme),
        swatch_tile("violet", p.violet, theme),
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
                .color(color),
            label(name)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_faint),
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
                .color(theme.palette.text_faint),
            label(v)
                .text_size(theme.typography.size_body)
                .color(theme.palette.text),
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
    let block = sized_box(label(""))
        .fixed_width(Length::px(96.0))
        .fixed_height(Length::px(32.0))
        .background_color(color)
        .border(theme.palette.border, 1.0)
        .corner_radius(f64::from(theme.radius.small));
    let caption = label(name)
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_muted);
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
