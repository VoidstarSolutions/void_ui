//! void-ui Tessera theme gallery.
//!
//! Run with `cargo run -p void-ui --example gallery`.
//!
//! Renders the resolved [`void_ui::Theme`] tokens — palette swatches, text
//! samples, density numbers, radii — as a visual reference. Toggles switch
//! dark/light and the three density steps so the live theme can be
//! sanity-checked end-to-end.

use xilem::masonry::layout::Length;
use xilem::peniko::Color;
use xilem::style::Style as _;
use xilem::view::{
    CrossAxisAlignment, FlexSpacer, MainAxisAlignment, flex_col, flex_row, label, sized_box,
    text_button,
};
use xilem::winit::error::EventLoopError;
use xilem::{EventLoop, WidgetView, WindowOptions, Xilem};

use void_ui::theme::{Density, Theme};

struct State {
    theme: Theme,
}

impl State {
    fn new() -> Self {
        Self {
            theme: Theme::dark(),
        }
    }
}

fn app_logic(state: &mut State) -> impl WidgetView<State> + use<> {
    let theme = state.theme;
    let body = flex_col((
        topbar(&theme),
        section_surfaces(&theme),
        section_text(&theme),
        section_accents(&theme),
        section_domain(&theme),
        section_tokens(&theme),
        FlexSpacer::Flex(1.0),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_alignment(MainAxisAlignment::Start)
    .gap(Length::px(18.0));

    sized_box(body)
        .padding(20.0)
        .background_color(theme.palette.bg_deep)
}

fn topbar(theme: &Theme) -> impl WidgetView<State> + use<> {
    let title = label("void-ui · Tessera tokens")
        .text_size(16.0)
        .color(theme.palette.text);
    let subtitle = label("dark / light · compact / balanced / airy")
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_faint);
    let header = flex_col((title, subtitle))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(2.0));

    let theme_toggle = flex_row((
        text_button("Dark", |s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::dark().with_density(d);
        }),
        text_button("Light", |s: &mut State| {
            let d = s.theme.density;
            s.theme = Theme::light().with_density(d);
        }),
    ))
    .gap(Length::px(6.0));

    let density_toggle = flex_row((
        text_button("Compact", |s: &mut State| {
            s.theme = s.theme.with_density(Density::compact());
        }),
        text_button("Balanced", |s: &mut State| {
            s.theme = s.theme.with_density(Density::balanced());
        }),
        text_button("Airy", |s: &mut State| {
            s.theme = s.theme.with_density(Density::airy());
        }),
    ))
    .gap(Length::px(6.0));

    flex_row((header, FlexSpacer::Flex(1.0), theme_toggle, density_toggle))
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(Length::px(12.0))
}

fn section_surfaces(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    section(
        "Surfaces",
        theme,
        flex_row((
            swatch_tile("bg", p.bg, theme),
            swatch_tile("bg_deep", p.bg_deep, theme),
            swatch_tile("surface", p.surface, theme),
            swatch_tile("surface_2", p.surface_2, theme),
            swatch_tile("surface_hi", p.surface_hi, theme),
            swatch_tile("border", p.border, theme),
            swatch_tile("border_strong", p.border_strong, theme),
            swatch_tile("grid", p.grid, theme),
            swatch_tile("grid_major", p.grid_major, theme),
        )),
    )
}

fn section_text(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    let sample = |name: &'static str, color: Color| {
        flex_col((
            label("Aa 0123 — pivot $184.62")
                .text_size(theme.typography.size_body)
                .color(color),
            label(name)
                .text_size(theme.typography.size_caption)
                .color(theme.palette.text_faint),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(4.0))
    };
    section(
        "Text",
        theme,
        flex_row((
            sample("text", p.text),
            sample("text_muted", p.text_muted),
            sample("text_faint", p.text_faint),
        ))
        .gap(Length::px(24.0)),
    )
}

fn section_accents(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    let row_a = flex_row((
        swatch_tile("teal", p.teal, theme),
        swatch_tile("teal_soft", p.teal_soft, theme),
        swatch_tile("coral", p.coral, theme),
        swatch_tile("coral_soft", p.coral_soft, theme),
    ));
    let row_b = flex_row((
        swatch_tile("amber (signal)", p.amber, theme),
        swatch_tile("amber_soft", p.amber_soft, theme),
        swatch_tile("violet (pattern)", p.violet, theme),
        swatch_tile("violet_soft", p.violet_soft, theme),
    ));
    section(
        "Accents",
        theme,
        flex_col((row_a, row_b)).gap(Length::px(10.0)),
    )
}

fn section_domain(theme: &Theme) -> impl WidgetView<State> + use<> {
    let p = &theme.palette;
    section(
        "Domain",
        theme,
        flex_row((
            swatch_tile("target", p.target, theme),
            swatch_tile("target_soft", p.target_soft, theme),
            swatch_tile("compare", p.compare, theme),
        )),
    )
}

fn section_tokens(theme: &Theme) -> impl WidgetView<State> + use<> {
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
    section(
        "Density · Radii",
        theme,
        flex_row((
            kv("row", format!("{:.0} px", d.row)),
            kv("col", format!("{:.0} px", d.col)),
            kv("pad", format!("{:.0} px", d.pad)),
            kv("ui_fs", format!("{:.0} px", d.ui_font_size)),
            kv("radius.s", format!("{:.0} px", r.small)),
            kv("radius.l", format!("{:.0} px", r.large)),
        ))
        .gap(Length::px(20.0)),
    )
}

fn section<V>(title: &'static str, theme: &Theme, body: V) -> impl WidgetView<State> + use<V>
where
    V: WidgetView<State>,
{
    let header = label(title)
        .text_size(theme.typography.size_caption)
        .letter_spacing(1.2)
        .color(theme.palette.text_faint);
    flex_col((header, body))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(10.0))
}

fn swatch_tile(name: &'static str, color: Color, theme: &Theme) -> impl WidgetView<State> + use<> {
    let block = sized_box(label(""))
        .fixed_width(Length::px(110.0))
        .fixed_height(Length::px(40.0))
        .background_color(color)
        .border(theme.palette.border, 1.0)
        .corner_radius(f64::from(theme.radius.small));
    let caption = label(name)
        .text_size(theme.typography.size_caption)
        .color(theme.palette.text_muted);
    flex_col((block, caption))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Length::px(4.0))
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(
        State::new(),
        app_logic,
        WindowOptions::new("void-ui · Tessera theme gallery"),
    );
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
