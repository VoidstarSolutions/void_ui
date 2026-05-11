//! void-ui component gallery.
//!
//! Run with `cargo run -p void-ui --example gallery`.
//!
//! Left rail lists components; main pane shows the focused component's
//! demo panel. Theme controls (Dark/Light + density) are in the topbar
//! today; a follow-up commit moves them into a popover triggered by a
//! top-right gear.

use xilem::masonry::layout::Length;
use xilem::style::Style as _;
use xilem::view::{
    CrossAxisAlignment, FlexExt as _, FlexSpacer, MainAxisAlignment, flex_col, flex_row, label,
    sized_box,
};
use xilem::winit::error::EventLoopError;
use xilem::{AnyWidgetView, EventLoop, WidgetView, WindowOptions, Xilem};

use void_ui::components::{ComponentKind, button};
use void_ui::theme::{Density, Theme};

struct State {
    theme: Theme,
    focused: ComponentKind,
}

impl State {
    fn new() -> Self {
        Self {
            theme: Theme::dark(),
            focused: ComponentKind::Button,
        }
    }
}

fn app_logic(state: &mut State) -> impl WidgetView<State> + use<> {
    let theme = state.theme;
    let focused = state.focused;

    let workspace = flex_row((
        sized_box(sidebar(focused, &theme))
            .fixed_width(Length::px(180.0))
            .padding(12.0)
            .background_color(theme.palette.surface),
        sized_box(main_pane(focused, &theme))
            .padding(20.0)
            .background_color(theme.palette.bg)
            .flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start);

    let outer = flex_col((topbar(&theme), workspace.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_alignment(MainAxisAlignment::Start);

    sized_box(outer).background_color(theme.palette.bg_deep)
}

fn topbar(theme: &Theme) -> impl WidgetView<State> + use<> {
    let title = label("void-ui · components")
        .text_size(16.0)
        .color(theme.palette.text);
    let subtitle = label("Tessera-styled widget library")
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_faint);
    let header = flex_col((title, subtitle))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(2.0));

    let theme_toggle = flex_row((
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
    .gap(Length::px(6.0));

    let density_toggle = flex_row((
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
    .gap(Length::px(6.0));

    let chrome = flex_row((header, FlexSpacer::Flex(1.0), theme_toggle, density_toggle))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0));

    sized_box(chrome)
        .padding(12.0)
        .background_color(theme.palette.surface)
        .border(theme.palette.border, 1.0)
}

fn sidebar(focused: ComponentKind, theme: &Theme) -> impl WidgetView<State> + use<> {
    // One entry per ComponentKind. While there's only one variant, this is a
    // tuple of one. When more components arrive we'll switch to a
    // Vec<Box<AnyWidgetView<State>>>.
    flex_col((button("Button", |s: &mut State| {
        s.focused = ComponentKind::Button;
    })
    .active(focused == ComponentKind::Button)
    .render(theme),))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Length::px(4.0))
}

fn main_pane(focused: ComponentKind, theme: &Theme) -> Box<AnyWidgetView<State>> {
    match focused {
        ComponentKind::Button => Box::new(void_ui::components::button::demo::panel(theme)),
    }
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
