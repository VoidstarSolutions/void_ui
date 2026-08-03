//! Headless per-frame cost probe for the gallery's demo panels.
//!
//! Run with:
//!
//! ```sh
//! cargo run --release -p void-ui --example perf_probe --features gallery
//! ```
//!
//! Neither masonry nor vello does damage tracking, so the main-thread cost of a
//! frame is proportional to the *total* painted content of the window rather
//! than to whatever changed. This probe measures that total directly, without a
//! GPU or a window, by driving each demo panel through a `TestHarness` and
//! reading masonry's paint output:
//!
//! - **cmds** — commands in the flattened root scene. This is the unit of work
//!   that gets copied by `append_transformed` in masonry's paint pass, re-encoded
//!   into a fresh `vello::Scene` by `imaging_vello::encode_source`, and resolved
//!   again on the CPU inside `vello::Renderer::render_to_texture`. Every frame,
//!   for the whole tree.
//! - **repaint** — wall time for a `redraw()` where a single widget has been
//!   invalidated. This is the floor for "user hovered one button": no matter how
//!   little changed, the whole scene is re-flattened.
//!
//! Then, with *no user input at all*, it drives 60 animation frames (~1 s) and
//! reports two different things, which must not be confused:
//!
//! - **settle** — the frame index at which the scene last changed. A one-off
//!   entry transition (a scrollbar fading out, a clip animating open) shows up
//!   here and is fine: it ends.
//! - **late** — scene changes during the final 10 frames, i.e. still moving a
//!   full second after the panel was built with nobody touching it. Anything
//!   above 0 is a panel that pins the whole window at refresh rate *forever*,
//!   and every one of those frames pays the full vello encode + resolve +
//!   submit cost no matter how small its scene is.
//!
//! `--release` matters; a debug build measures unoptimized paint code.

use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use masonry::core::NewWidget;
use masonry::imaging::record::Scene;
use masonry::testing::TestHarness;
use masonry::theme::default_property_set;
use masonry::widgets::Passthrough;
use xilem::AnyWidgetView;
use xilem::core::{ProxyError, RawProxy, SendMessage, View, ViewId};
use xilem::style::Style as _;
use xilem::view::{flex_col, sized_box};
use xilem_masonry::ViewCtx;

use void_ui::components::ScrollBarVisibility::OnActivity;
use void_ui::components::dialog::demo::DialogDemoState;
use void_ui::components::notification::demo::NotificationDemoState;
use void_ui::components::{ComponentKind, SidebarNavItem, sidebar_nav};
use void_ui::overlay_scope::overlay_scope;
use void_ui::scroll_container;
use void_ui::theme::Theme;

/// Window size the probe lays panels out in — a plausible gallery window.
const WINDOW: (u16, u16) = (1400, 900);
/// Repaint samples per panel. The spread between min and median is noise; the
/// median is what a user feels.
const SAMPLES: usize = 40;
/// Animation frames driven with no user input (~1 s at 60 Hz).
const IDLE_FRAMES: usize = 60;
/// View-rebuild samples per panel. Fewer than repaint samples because a
/// rebuild is far more expensive.
const REBUILD_SAMPLES: usize = 10;
/// How many trailing frames count as "still moving long after settling".
const LATE_WINDOW: usize = 10;

// --- MARK: HARNESS PLUMBING

/// Demo panels are generic over app state and reach for the two demo states the
/// gallery owns, so the probe mirrors the gallery's `State` shape.
struct ProbeState {
    notification: NotificationDemoState,
    dialog: DialogDemoState,
}

impl AsMut<NotificationDemoState> for ProbeState {
    fn as_mut(&mut self) -> &mut NotificationDemoState {
        &mut self.notification
    }
}

impl AsMut<DialogDemoState> for ProbeState {
    fn as_mut(&mut self) -> &mut DialogDemoState {
        &mut self.dialog
    }
}

/// A `RawProxy` that drops every message. The probe never proxies anything.
#[derive(Debug)]
struct NoopProxy;

impl RawProxy for NoopProxy {
    fn send_message(&self, _path: Arc<[ViewId]>, _message: SendMessage) -> Result<(), ProxyError> {
        Ok(())
    }
    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

/// View state for a boxed, erased panel view.
type ProbeViewState = <Box<AnyWidgetView<ProbeState>> as View<ProbeState, (), ViewCtx>>::ViewState;

/// A live `ViewCtx`, kept around so the probe can drive rebuilds as well as
/// the initial build.
fn probe_ctx() -> ViewCtx {
    let runtime = Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("probe runtime"),
    );
    ViewCtx::new(Arc::new(NoopProxy), runtime)
}

/// Materialize a xilem view into a masonry widget, the way the real app does at
/// startup, so the probe measures shipped widgets rather than a stand-in.
fn build_widget(
    view: &AnyWidgetView<ProbeState>,
    state: &mut ProbeState,
    ctx: &mut ViewCtx,
) -> (NewWidget<Passthrough>, ProbeViewState) {
    let (pod, view_state) = view.build(ctx, state);
    // A boxed, erased xilem view builds to `Pod<Passthrough>`, which is already
    // sized — so it can be the harness root directly, with no extra wrapper
    // widget between the harness and the panel under measurement.
    (pod.new_widget, view_state)
}

/// Every scene layer masonry produced for this frame, flattened into one list.
fn frame_scenes(harness: &mut TestHarness<Passthrough>) -> Vec<Scene> {
    let (plan, _) = harness.redraw();
    plan.layers
        .iter()
        .filter_map(|layer| match &layer.kind {
            masonry::app::VisualLayerKind::Scene(scene) => Some(scene.clone()),
            masonry::app::VisualLayerKind::External { .. } => None,
        })
        .collect()
}

/// Total commands across every layer masonry produced for this frame.
fn scene_commands(harness: &mut TestHarness<Passthrough>) -> usize {
    frame_scenes(harness)
        .iter()
        .map(Scene::commands)
        .map(<[_]>::len)
        .sum()
}

// --- MARK: MEASUREMENT

struct Measurement {
    label: &'static str,
    commands: usize,
    rebuild_median: Duration,
    repaint_median: Duration,
    settle_frame: Option<usize>,
    late_churn: usize,
}

/// Drive one view through a harness and time a single-widget-invalidation
/// repaint — the "user hovered one button" case.
fn measure(label: &'static str, view: Box<AnyWidgetView<ProbeState>>) -> Measurement {
    let mut state = ProbeState {
        notification: NotificationDemoState::default(),
        dialog: DialogDemoState::default(),
    };
    // The real gallery wraps its root in an `overlay_scope`, and `dialog` panics
    // without one, so the probe measures the same shape the app ships.
    let view: Box<AnyWidgetView<ProbeState>> = Box::new(overlay_scope::<ProbeState, (), _>(view));
    let mut ctx = probe_ctx();
    let (widget, mut view_state) = build_widget(&*view, &mut state, &mut ctx);
    let mut harness = TestHarness::create_with_size(default_property_set(), widget, WINDOW);

    // Settle layout and the first paint before sampling.
    let commands = scene_commands(&mut harness);

    // A xilem state change re-runs the app's view function and diffs the whole
    // view tree against the previous one. Every click pays this *before* any
    // painting happens, so it is measured separately from repaint.
    let mut rebuilds = Vec::with_capacity(REBUILD_SAMPLES);
    for _ in 0..REBUILD_SAMPLES {
        let start = Instant::now();
        harness.edit_root_widget(|root| {
            view.rebuild(&view, &mut view_state, &mut ctx, root, &mut state);
        });
        rebuilds.push(start.elapsed());
    }
    rebuilds.sort_unstable();

    let root_id = harness.root_id();
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        // Invalidate exactly one widget, then time the frame that results.
        harness.edit_widget_with_id(root_id, |mut widget| {
            widget.ctx.request_paint_only();
        });
        let start = Instant::now();
        let (plan, _) = harness.redraw();
        samples.push(start.elapsed());
        black_box(&plan);
    }
    samples.sort_unstable();

    // Drive animation frames with no input. A settling transition stops
    // changing the scene; an unbounded animator never does.
    let mut prev = frame_scenes(&mut harness);
    let mut settle_frame = None;
    let mut late_churn = 0;
    for frame in 0..IDLE_FRAMES {
        harness.animate_ms(16);
        let next = frame_scenes(&mut harness);
        if next != prev {
            settle_frame = Some(frame);
            if frame >= IDLE_FRAMES - LATE_WINDOW {
                late_churn += 1;
            }
            prev = next;
        }
    }

    Measurement {
        label,
        commands,
        rebuild_median: rebuilds[rebuilds.len() / 2],
        repaint_median: samples[samples.len() / 2],
        settle_frame,
        late_churn,
    }
}

/// The gallery's left rail, measured on its own so its cost can be separated
/// from whichever panel is focused.
fn sidebar_view(theme: &Theme) -> Box<AnyWidgetView<ProbeState>> {
    let items = ComponentKind::all()
        .iter()
        .map(|kind| SidebarNavItem::new(kind.label()))
        .collect();
    Box::new(sized_box(
        scroll_container(sidebar_nav(items, 0, |_: &mut ProbeState, _| {}).render(theme))
            .constrain_horizontal(true)
            .scroll_bar_visibility(OnActivity)
            .render(theme),
    ))
}

/// An empty panel: the floor cost of the harness itself, so panel numbers can
/// be read as "content cost" rather than "content + scaffolding".
fn baseline_view(theme: &Theme) -> Box<AnyWidgetView<ProbeState>> {
    Box::new(sized_box(flex_col(())).background_color(theme.palette.bg))
}

/// Mirror of the gallery example's `main_pane`. Duplicated rather than lifted
/// into the library: this is a diagnostic tool, and the panel dispatch is not
/// something consumers should depend on.
fn panel_for(kind: ComponentKind, theme: &Theme) -> Box<AnyWidgetView<ProbeState>> {
    use void_ui::components as c;
    match kind {
        ComponentKind::Alert => Box::new(c::alert::demo::panel(theme)),
        ComponentKind::Autocomplete => Box::new(c::autocomplete::demo::panel(theme)),
        ComponentKind::Badge => Box::new(c::badge::demo::panel(theme)),
        ComponentKind::Breadcrumb => Box::new(c::breadcrumb::demo::panel(theme)),
        ComponentKind::Button => Box::new(c::button::demo::panel(theme)),
        ComponentKind::ButtonGroup => Box::new(c::button_group::demo::panel(theme)),
        ComponentKind::Card => Box::new(c::card::demo::panel(theme)),
        ComponentKind::Checkbox => Box::new(c::checkbox::demo::panel(theme)),
        ComponentKind::Clipboard => Box::new(c::clipboard::demo::panel(theme)),
        ComponentKind::CodeView => Box::new(c::code_view::demo::panel(theme)),
        ComponentKind::Collapsible => Box::new(c::collapsible::demo::panel(theme)),
        ComponentKind::ContextMenu => Box::new(c::context_menu::demo::panel(theme)),
        ComponentKind::DataGrid => Box::new(c::data_grid::demo::panel(theme)),
        ComponentKind::DatePicker => Box::new(c::date_picker::demo::panel(theme)),
        ComponentKind::Dialog => Box::new(c::dialog::demo::panel(theme)),
        ComponentKind::DropdownButton => Box::new(c::dropdown_button::demo::panel(theme)),
        ComponentKind::Form => Box::new(c::form::demo::panel(theme)),
        ComponentKind::GroupBox => Box::new(c::group_box::demo::panel(theme)),
        ComponentKind::Icon => Box::new(c::icon::demo::panel(theme)),
        ComponentKind::Input => Box::new(c::input::demo::panel(theme)),
        ComponentKind::Label => Box::new(c::label::demo::panel(theme)),
        ComponentKind::List => Box::new(c::list::demo::panel(theme)),
        ComponentKind::Meter => Box::new(c::meter::demo::panel(theme)),
        ComponentKind::Notification => Box::new(c::notification::demo::panel(theme)),
        ComponentKind::Popover => Box::new(c::popover::demo::panel(theme)),
        ComponentKind::Radio => Box::new(c::radio::demo::panel(theme)),
        ComponentKind::Resizable => Box::new(c::resizable::demo::panel(theme)),
        ComponentKind::ScrollContainer => Box::new(c::scroll_container::demo::panel(theme)),
        ComponentKind::Separator => Box::new(c::separator::demo::panel(theme)),
        ComponentKind::Sidebar => Box::new(c::sidebar::demo::panel(theme)),
        ComponentKind::Skeleton => Box::new(c::skeleton::demo::panel(theme)),
        ComponentKind::Slider => Box::new(c::slider::demo::panel(theme)),
        ComponentKind::Spinner => Box::new(c::spinner::demo::panel(theme)),
        ComponentKind::StatusDot => Box::new(c::status_dot::demo::panel(theme)),
        ComponentKind::Tabs => Box::new(c::tabs::demo::panel(theme)),
        ComponentKind::Toggle => Box::new(c::toggle::demo::panel(theme)),
        ComponentKind::Tooltip => Box::new(c::tooltip::demo::panel(theme)),
    }
}

fn main() {
    let theme = Theme::dark();

    println!(
        "void-ui perf probe — {}x{} window, {SAMPLES} samples/panel",
        WINDOW.0, WINDOW.1
    );
    println!(
        "\n{:<22} {:>7} {:>13} {:>12} {:>8} {:>6}",
        "target", "cmds", "rebuild p50", "repaint p50", "settle", "late"
    );
    println!("{}", "-".repeat(74));

    let mut rows = vec![
        measure("(empty baseline)", baseline_view(&theme)),
        measure("sidebar rail", sidebar_view(&theme)),
    ];

    for kind in ComponentKind::all() {
        rows.push(measure(kind.label(), panel_for(*kind, &theme)));
    }

    // Panels first by cost, so the worst offenders are the first thing read.
    let (fixed, panels) = rows.split_at_mut(2);
    panels.sort_by_key(|m| std::cmp::Reverse(m.rebuild_median));

    for m in fixed.iter().chain(panels.iter()) {
        let flag = if m.late_churn > 0 {
            "  <-- NEVER IDLES"
        } else {
            ""
        };
        let settle = m
            .settle_frame
            .map_or_else(|| "-".to_string(), |f| format!("f{f}"));
        println!(
            "{:<22} {:>7} {:>13.3?} {:>12.3?} {:>8} {:>6}{flag}",
            m.label, m.commands, m.rebuild_median, m.repaint_median, settle, m.late_churn
        );
    }

    let total: usize = panels.iter().map(|m| m.commands).sum();
    let churning: Vec<_> = fixed
        .iter()
        .chain(panels.iter())
        .filter(|m| m.late_churn > 0)
        .map(|m| m.label)
        .collect();
    println!("\n{} panels, {total} commands total", panels.len());
    println!(
        "{} target(s) never go idle: {}",
        churning.len(),
        if churning.is_empty() {
            "-".to_string()
        } else {
            churning.join(", ")
        }
    );
}
