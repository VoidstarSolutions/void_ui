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
//! Two caveats, both learned the hard way:
//!
//! - **Ignore per-widget rows** (`Label`, `Flex`, `Passthrough`, …). Those come
//!   from `Widget::make_trace_span`, which masonry creates once per widget and
//!   re-enters on every pass; the span closes when the *widget* is dropped, so
//!   its "duration" is the widget's lifetime, not any unit of work. Only
//!   pass-level spans — `layout`, `paint`, `compose`, `update_new_widgets`,
//!   `update_anim`, `redraw`, `Rendering Masonry window` — are meaningful here.
//! - **`input -> frame` over-reports when an input needed no repaint.** Not
//!   every event dirties anything (a pointer move over inert chrome, say), so
//!   the pending timestamp can survive until some much later frame and report a
//!   gap of many seconds. Treat individual huge values as noise; what matters
//!   is the shape of the bulk of the distribution.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tracing::span::{Attributes, Id};
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

struct Timing {
    started: Instant,
}

#[derive(Default)]
struct Shared {
    /// When the most recent input event was seen, if no frame has followed yet.
    pending_input: Option<Instant>,
}

pub struct FrameTraceLayer {
    shared: Mutex<Shared>,
    start: Instant,
}

impl FrameTraceLayer {
    fn new() -> Self {
        Self {
            shared: Mutex::new(Shared::default()),
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
    fn on_new_span(&self, _attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Timing {
                started: Instant::now(),
            });
        }
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        // Re-stamp on enter: masonry creates some spans well before entering
        // them. `insert` panics if the extension is already present, and spans
        // can be entered more than once, so update in place when it exists.
        let mut ext = span.extensions_mut();
        if let Some(timing) = ext.get_mut::<Timing>() {
            timing.started = Instant::now();
        } else {
            ext.insert(Timing {
                started: Instant::now(),
            });
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        let name = span.name();
        let elapsed = span
            .extensions()
            .get::<Timing>()
            .map(|t| t.started.elapsed())
            .unwrap_or_default();

        match name {
            EVENT_SPAN => {
                // An input event just finished being handled. Whatever visual
                // response it causes has to come from a later frame.
                let mut shared = self.shared.lock().expect("frame-trace lock");
                if shared.pending_input.is_none() {
                    shared.pending_input = Some(Instant::now());
                }
            }
            RENDER_SPAN => {
                let waited = {
                    let mut shared = self.shared.lock().expect("frame-trace lock");
                    shared.pending_input.take().map(|t| t.elapsed())
                };
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
/// via `try_init_tracing`, which is a no-op once one is already set.
pub fn install_if_requested() {
    if std::env::var("VOID_UI_TRACE_FRAMES").as_deref() != Ok("1") {
        return;
    }
    tracing_subscriber::registry()
        .with(FrameTraceLayer::new())
        .init();
    println!("frame tracing on — 'input -> frame' is click-to-repaint latency");
}
