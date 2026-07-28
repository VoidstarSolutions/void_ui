//! Frame-latency instrumentation layer for the gallery example.
//!
//! Included by `examples/gallery.rs`; activated with `VOID_UI_TRACE_FRAMES=1`.
//!
//! The headless probe (`examples/perf_probe.rs`) shows masonry's paint pass and
//! xilem's view rebuild are both microseconds, so when the *real* app feels
//! slow the cost has to be somewhere the probe cannot see: the GPU render, the
//! surface present, or a gap where no frame was requested at all.
//!
//! This layer times masonry's own `info_span!`s in the running app and prints
//! anything slow, plus the wall-clock gap between an input event arriving and
//! the next frame being rendered — which is the number the user actually feels.
//!
//! # Reading the output
//!
//! Durations are measured **enter -> exit**, i.e. time actually spent inside
//! the span, and deliberately *not* creation -> close.
//!
//! That distinction is the whole reason this file has a doc comment. Masonry
//! stores a per-widget span on the widget itself (`state.trace_span =
//! widget.make_trace_span(id)` in the `update_new_widgets` pass), and in
//! `tracing` a child span keeps its parent alive — so a pass span does not
//! *close* until every widget created during it has been dropped. Timing on
//! close therefore reports widget teardown as if it were pass duration, and
//! produces spectacular fictions: an early version of this layer showed
//! `update_new_widgets` taking 1.2-4.4 seconds, with seven separate 1.19s
//! spans all closing within 4ms of each other. None of it was real work.
//!
//! One caveat remains: **`input -> frame` over-reports when an input needed no
//! repaint.** Not every event dirties anything (a pointer move over inert
//! chrome, say), so the pending timestamp can survive until some much later
//! frame and report a gap of many seconds. Treat individual huge values as
//! noise; what matters is the bulk of the distribution.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::span::Id;
use tracing::{Subscriber, field};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

/// Spans at or above this duration get printed individually.
const SLOW_SPAN: Duration = Duration::from_millis(2);

/// Span name masonry uses around the whole GPU render + submit.
const RENDER_SPAN: &str = "Rendering Masonry window";
/// Span name masonry uses around each winit window event.
const EVENT_SPAN: &str = "window_event";
/// Span masonry uses around dispatching a pointer event into the widget tree.
/// Logging every one of these shows whether input is *arriving* steadily, which
/// is the half of the story frame timing cannot tell.
const DISPATCH_SPAN: &str = "dispatch_pointer_event";
/// Span masonry uses around keyboard dispatch. Logged loudly so the person
/// reproducing a problem can timestamp it themselves by pressing a key — the
/// only reliable way to correlate "it felt stuck here" with the trace.
const TEXT_SPAN: &str = "dispatch_text_event";

struct Timing {
    started: Instant,
}

#[derive(Default)]
struct Shared {
    /// When the most recent input event was seen, if no frame has followed yet.
    pending_input: Option<Instant>,
    /// An app-level action awaiting the frame that renders its effect.
    pending_action: Option<(&'static str, Instant)>,
}

/// Records that an app-level state change just happened, so the next rendered
/// frame can report how long it took to become visible.
///
/// This is the only honest click-to-pixels measurement available here. Every
/// span-derived proxy measures something subtly different: `input -> frame`
/// times the *last* event before a frame, which under a continuous pointer-move
/// stream is always about one frame regardless of how stale the content is.
///
/// Call this from the callback that mutates app state — the moment the app
/// "knows" about the click.
pub fn mark(label: &'static str) {
    if let Some(shared) = SHARED.get() {
        let mut shared = shared.lock().expect("frame-trace lock");
        if shared.pending_action.is_none() {
            shared.pending_action = Some((label, Instant::now()));
        }
    }
}

/// Set once `install_if_requested` runs, so `mark` is a cheap no-op otherwise.
static SHARED: std::sync::OnceLock<&'static Mutex<Shared>> = std::sync::OnceLock::new();

pub struct FrameTraceLayer {
    shared: &'static Mutex<Shared>,
    start: Instant,
}

impl FrameTraceLayer {
    fn new(shared: &'static Mutex<Shared>) -> Self {
        Self {
            shared,
            start: Instant::now(),
        }
    }

    fn stamp(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
}

impl<S> Layer<S> for FrameTraceLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        // Stamp on enter, report on exit. Spans can be entered more than once,
        // and `insert` panics when the extension already exists, so update in
        // place when present.
        let mut ext = span.extensions_mut();
        if let Some(timing) = ext.get_mut::<Timing>() {
            timing.started = Instant::now();
        } else {
            ext.insert(Timing {
                started: Instant::now(),
            });
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        let name = span.name();
        let Some(elapsed) = span
            .extensions()
            .get::<Timing>()
            .map(|t| t.started.elapsed())
        else {
            return;
        };

        match name {
            TEXT_SPAN => {
                println!("[{:8.3}] ======== USER MARK (key) ========", self.stamp());
                // Text dispatch just finished; whatever visual response it
                // causes has to come from a later frame. Overwrite rather than
                // keep-first so `pending_input` always reflects the input that
                // most recently reached the widget tree, not a stale one from
                // earlier in the same frame gap.
                let mut shared = self.shared.lock().expect("frame-trace lock");
                shared.pending_input = Some(Instant::now());
            }
            DISPATCH_SPAN => {
                println!("[{:8.3}] pointer dispatch {:>9.3?}", self.stamp(), elapsed);
                let mut shared = self.shared.lock().expect("frame-trace lock");
                shared.pending_input = Some(Instant::now());
            }
            EVENT_SPAN => {}
            RENDER_SPAN => {
                let waited = {
                    let mut shared = self.shared.lock().expect("frame-trace lock");
                    shared.pending_input.take().map(|t| t.elapsed())
                };
                let action = {
                    let mut shared = self.shared.lock().expect("frame-trace lock");
                    shared.pending_action.take()
                };
                if let Some((label, at)) = action {
                    println!(
                        "[{:8.3}] ACTION {label:<12} -> visible after {:>9.3?}   (render {:>9.3?})",
                        self.stamp(),
                        at.elapsed(),
                        elapsed
                    );
                }
                let _ = &self.shared;
                if let Some(waited) = waited {
                    println!(
                        "[{:8.3}] input -> frame: {:>9.3?}   (render span {:>9.3?})",
                        self.stamp(),
                        waited,
                        elapsed
                    );
                } else if elapsed >= SLOW_SPAN {
                    println!(
                        "[{:8.3}] frame (no pending input): render {:>9.3?}",
                        self.stamp(),
                        elapsed
                    );
                }
            }
            _ if elapsed >= SLOW_SPAN => {
                println!(
                    "[{:8.3}] slow span {:<28} {:>9.3?}",
                    self.stamp(),
                    name,
                    elapsed
                );
            }
            _ => {}
        }
        let _ = field::Empty;
    }
}

/// Installs the frame tracer when `VOID_UI_TRACE_FRAMES=1`.
///
/// Must run before the event loop starts: masonry installs its own subscriber
/// via `try_init_tracing`, which is a no-op once one is already set. That also
/// means this has to compose the `profiling` feature's `TracyLayer` into its
/// own registry when that feature is on — otherwise finalizing our subscriber
/// here would permanently shut Tracy out, silently, for the rest of the run.
pub fn install_if_requested() {
    if std::env::var("VOID_UI_TRACE_FRAMES").as_deref() != Ok("1") {
        return;
    }
    // Leak the shared state so `mark` can reach it without threading a handle
    // through the whole gallery. The process lives as long as the tracer.
    let shared: &'static Mutex<Shared> = Box::leak(Box::new(Mutex::new(Shared::default())));
    let _ = SHARED.set(shared);

    let registry = tracing_subscriber::registry().with(FrameTraceLayer::new(shared));
    #[cfg(feature = "profiling")]
    let registry = registry.with(tracing_tracy::TracyLayer::default());
    registry.init();

    println!("frame tracing on — 'input -> frame' is click-to-repaint latency");
    #[cfg(feature = "profiling")]
    println!("also streaming to Tracy (profiling feature is on)");
}
