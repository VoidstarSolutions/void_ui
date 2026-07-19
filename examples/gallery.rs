//! void-ui component gallery.
//!
//! Run with `cargo run -p void-ui --example gallery`.
//!
//! Left rail lists components; main pane shows the focused component's
//! demo panel. A gear button in the top-right toggles a theme panel
//! (right side) with theme/density controls plus a live reference of
//! the current palette, text samples, and density numbers.

use void_ui::components::ScrollBarVisibility::OnActivity;
use void_ui::components::dialog::demo::DialogDemoState;
use void_ui::components::notification::demo::NotificationDemoState;
use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, portal,
    sized_box, zstack,
};
use xilem::winit::error::EventLoopError;
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

use void_ui::components::{ComponentKind, SidebarNavItem, button, sidebar_nav, sidebar_panel};
use void_ui::layout::flex_wrap;
use void_ui::overlay_scope::overlay_scope;
use void_ui::theme::{Density, Theme};
use void_ui::{label, scroll_container};

struct State {
    theme: Theme,
    focused: ComponentKind,
    theme_panel_open: bool,
    sidebar_collapsed: bool,
    notification_demo: NotificationDemoState,
    dialog_demo: DialogDemoState,
}

impl State {
    fn new() -> Self {
        Self {
            theme: Theme::dark(),
            focused: ComponentKind::Button,
            theme_panel_open: false,
            sidebar_collapsed: false,
            notification_demo: NotificationDemoState::default(),
            dialog_demo: DialogDemoState::default(),
        }
    }
}

impl AsMut<NotificationDemoState> for State {
    fn as_mut(&mut self) -> &mut NotificationDemoState {
        &mut self.notification_demo
    }
}

impl AsMut<DialogDemoState> for State {
    fn as_mut(&mut self) -> &mut DialogDemoState {
        &mut self.dialog_demo
    }
}

fn app_logic(state: &mut State) -> impl WidgetView<State> + use<> {
    let theme = state.theme;
    let focused = state.focused;
    let theme_panel_open = state.theme_panel_open;
    let sidebar_collapsed = state.sidebar_collapsed;
    let notification_demo = state.notification_demo.clone();

    let workspace = workspace_row(focused, theme_panel_open, sidebar_collapsed, &theme);

    let outer = flex_col((topbar(theme_panel_open, &theme), workspace.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_alignment(MainAxisAlignment::Start);

    let content = sized_box(outer).background_color(theme.palette.bg_deep);
    let toast_layer =
        void_ui::components::notification::demo::overlay::<State>(&theme, &notification_demo);

    overlay_scope(zstack((content, toast_layer)))
}

fn workspace_row(
    focused: ComponentKind,
    theme_panel_open: bool,
    sidebar_collapsed: bool,
    theme: &Theme,
) -> Box<AnyWidgetView<State>> {
    let sidebar_view = sidebar_panel(
        sized_box(sidebar_items(focused, theme))
            .padding(Length::px(12.0))
            .background_color(theme.palette.surface),
        |s: &mut State| s.sidebar_collapsed = !s.sidebar_collapsed,
    )
    .collapsed(sidebar_collapsed)
    .render(theme);
    let main = sized_box(main_pane(focused, theme))
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
    .selected(theme_panel_open)
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
    let kinds = ComponentKind::all();
    let active = kinds.iter().position(|&k| k == focused).unwrap_or(0);
    let items = kinds
        .iter()
        .map(|kind| SidebarNavItem::new(kind.label()))
        .collect();

    scroll_container(
        sidebar_nav(items, active, |s: &mut State, i| {
            s.focused = ComponentKind::all()[i];
        })
        .render(theme),
    )
    .constrain_horizontal(true)
    .scroll_bar_visibility(OnActivity)
    .render(theme)
}

fn main_pane(focused: ComponentKind, theme: &Theme) -> Box<AnyWidgetView<State>> {
    match focused {
        ComponentKind::Alert => Box::new(void_ui::components::alert::demo::panel(theme)),
        ComponentKind::Autocomplete => {
            Box::new(void_ui::components::autocomplete::demo::panel(theme))
        }
        ComponentKind::Badge => Box::new(void_ui::components::badge::demo::panel(theme)),
        ComponentKind::Breadcrumb => Box::new(void_ui::components::breadcrumb::demo::panel(theme)),
        ComponentKind::Button => Box::new(void_ui::components::button::demo::panel(theme)),
        ComponentKind::ButtonGroup => {
            Box::new(void_ui::components::button_group::demo::panel(theme))
        }
        ComponentKind::Card => Box::new(void_ui::components::card::demo::panel(theme)),
        ComponentKind::Checkbox => Box::new(void_ui::components::checkbox::demo::panel(theme)),
        ComponentKind::Collapsible => {
            Box::new(void_ui::components::collapsible::demo::panel(theme))
        }
        ComponentKind::Clipboard => Box::new(void_ui::components::clipboard::demo::panel(theme)),
        ComponentKind::ContextMenu => {
            Box::new(void_ui::components::context_menu::demo::panel(theme))
        }
        ComponentKind::DataGrid => Box::new(void_ui::components::data_grid::demo::panel(theme)),
        ComponentKind::DatePicker => Box::new(void_ui::components::date_picker::demo::panel(theme)),
        ComponentKind::Dialog => Box::new(void_ui::components::dialog::demo::panel(theme)),
        ComponentKind::DropdownButton => {
            Box::new(void_ui::components::dropdown_button::demo::panel(theme))
        }
        ComponentKind::GroupBox => Box::new(void_ui::components::group_box::demo::panel(theme)),
        ComponentKind::Icon => Box::new(void_ui::components::icon::demo::panel(theme)),
        ComponentKind::Input => Box::new(void_ui::components::input::demo::panel(theme)),
        ComponentKind::Label => Box::new(void_ui::components::label::demo::panel(theme)),
        ComponentKind::List => Box::new(void_ui::components::list::demo::panel(theme)),
        ComponentKind::Meter => Box::new(void_ui::components::meter::demo::panel(theme)),
        ComponentKind::Notification => {
            Box::new(void_ui::components::notification::demo::panel(theme))
        }

        ComponentKind::Radio => Box::new(void_ui::components::radio::demo::panel(theme)),
        ComponentKind::ScrollContainer => {
            Box::new(void_ui::components::scroll_container::demo::panel(theme))
        }
        ComponentKind::Resizable => Box::new(void_ui::components::resizable::demo::panel(theme)),
        ComponentKind::Separator => Box::new(void_ui::components::separator::demo::panel(theme)),
        ComponentKind::Sidebar => Box::new(void_ui::components::sidebar::demo::panel(theme)),
        ComponentKind::Skeleton => Box::new(void_ui::components::skeleton::demo::panel(theme)),
        ComponentKind::Slider => Box::new(void_ui::components::slider::demo::panel(theme)),
        ComponentKind::Spinner => Box::new(void_ui::components::spinner::demo::panel(theme)),
        ComponentKind::StatusDot => Box::new(void_ui::components::status_dot::demo::panel(theme)),
        ComponentKind::StockQuotes => Box::new(
            void_ui::components::data_grid::demo::stock_quotes_panel(theme),
        ),
        ComponentKind::TreeGrid => {
            Box::new(void_ui::components::data_grid::demo::tree_grid_panel(theme))
        }
        ComponentKind::Tabs => Box::new(void_ui::components::tabs::demo::panel(theme)),
        ComponentKind::Toggle => Box::new(void_ui::components::toggle::demo::panel(theme)),
        ComponentKind::CodeView => Box::new(void_ui::components::code_view::demo::panel(theme)),
        ComponentKind::Popover => Box::new(void_ui::components::popover::demo::panel(theme)),
        ComponentKind::Tooltip => Box::new(void_ui::components::tooltip::demo::panel(theme)),
    }
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
        section_header("Semantic", theme),
        semantic_block(theme),
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
        .selected(theme.is_dark())
        .render(theme),
        button(|s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::light().with_density(d);
        })
        .label("Light")
        .selected(theme.is_light())
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
        .selected(theme.density == Density::compact())
        .render(theme),
        button(|s: &mut State| {
            s.theme = s.theme.with_density(Density::balanced());
        })
        .label("Balanced")
        .selected(theme.density == Density::balanced())
        .render(theme),
        button(|s: &mut State| {
            s.theme = s.theme.with_density(Density::airy());
        })
        .label("Airy")
        .selected(theme.density == Density::airy())
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

fn semantic_block(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    flex_wrap((
        (
            swatch_tile("accent", p.accent, theme),
            swatch_tile("accent_deep", p.accent_deep, theme),
            swatch_tile("accent_soft", p.accent_soft, theme),
            swatch_tile("danger", p.danger, theme),
            swatch_tile("danger_deep", p.danger_deep, theme),
            swatch_tile("danger_soft", p.danger_soft, theme),
            swatch_tile("warning", p.warning, theme),
            swatch_tile("warning_deep", p.warning_deep, theme),
            swatch_tile("warning_soft", p.warning_soft, theme),
        ),
        (
            swatch_tile("secondary", p.secondary, theme),
            swatch_tile("secondary_deep", p.secondary_deep, theme),
            swatch_tile("secondary_soft", p.secondary_soft, theme),
            swatch_tile("success", p.success, theme),
            swatch_tile("success_deep", p.success_deep, theme),
            swatch_tile("success_soft", p.success_soft, theme),
            swatch_tile("info", p.info, theme),
            swatch_tile("info_deep", p.info_deep, theme),
            swatch_tile("info_soft", p.info_soft, theme),
            swatch_tile("focus", p.focus, theme),
        ),
    ))
    .gap(6.0)
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
        kv("pad", format!("{:.0} px", d.pad)),
        kv("pad_h/v", format!("{:.0}/{:.0} px", d.pad_h, d.pad_v)),
        kv("gap", format!("{:.0} px", d.gap)),
        kv("gap_lg", format!("{:.0} px", d.gap_lg)),
        kv("control", format!("{:.0} px", d.control)),
        kv("row_h", format!("{:.0} px", d.row_height)),
        kv("ui_fs", format!("{:.0} px", d.ui_font_size)),
        kv("radius.t", format!("{:.0} px", r.tiny)),
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
