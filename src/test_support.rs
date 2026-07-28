//! Shared scaffolding for widget and view unit tests.
//!
//! Several of the input modules build a `ViewCtx` (or feed one to a
//! `TestHarness`) to exercise their views off-screen. That needs a `RawProxy`
//! and a tokio runtime, neither of which the tests actually drive — so a single
//! no-op proxy and a current-thread runtime live here rather than being
//! re-declared in each module's test block.
//!
//! [`StashBox`] is a second, unrelated piece of scaffolding for widgets that
//! animate: it gives their tests a stashable parent to observe stashed/resumed
//! behavior, without each widget module re-declaring the same container.

use std::fmt;
use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NewWidget, NoAction, PaintCtx, PropertiesRef,
    RegisterCtx, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::testing::TestHarness;
use xilem::core::{ProxyError, RawProxy, SendMessage, ViewId};

/// A [`RawProxy`] that drops every message. Tests invoke views directly and
/// never expect a proxied message back, so the send is a no-op.
struct NoopProxy;

impl fmt::Debug for NoopProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NoopProxy")
    }
}

impl RawProxy for NoopProxy {
    fn send_message(&self, _path: Arc<[ViewId]>, _message: SendMessage) -> Result<(), ProxyError> {
        Ok(())
    }
    fn dyn_debug(&self) -> &dyn fmt::Debug {
        self
    }
}

/// A boxed no-op proxy, ready to hand to `ViewCtx::new`.
pub(crate) fn noop_proxy() -> Arc<dyn RawProxy> {
    Arc::new(NoopProxy)
}

/// A single-threaded tokio runtime for building views in tests.
pub(crate) fn current_thread_runtime() -> Arc<tokio::runtime::Runtime> {
    Arc::new(
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap(),
    )
}

/// Minimal container that can stash its child, so tests can put an animating
/// widget in the state a collapsed panel or a closed overlay would. Masonry's
/// anim pass does not skip stashed widgets, so this is the only way to
/// observe what a hidden animator actually does.
///
/// Generic rather than one struct per animating widget: the stash/unstash
/// mechanics don't depend on what's inside, only reading the animated phase
/// back out does, and that stays with each widget's own tests via
/// [`StashBox::child_mut`] (private widget fields are visible there, not here).
pub(crate) struct StashBox<W: Widget> {
    child: WidgetPod<W>,
}

impl<W: Widget> StashBox<W> {
    pub(crate) fn harness(child: W) -> TestHarness<Self> {
        TestHarness::create(
            masonry::theme::default_property_set(),
            NewWidget::new(Self {
                child: WidgetPod::new(child),
            }),
        )
    }

    pub(crate) fn set_child_stashed(this: &mut WidgetMut<'_, Self>, stashed: bool) {
        this.ctx.set_stashed(&mut this.widget.child, stashed);
    }

    /// The child widget, mutable, so the caller can read back whatever
    /// animation-phase field it defines.
    pub(crate) fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, W> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}

impl<W: Widget> Widget for StashBox<W> {
    type Action = NoAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }

    fn property_changed(&mut self, _ctx: &mut UpdateCtx<'_>, _t: std::any::TypeId) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        Length::px(16.0)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if !ctx.child_is_stashed(&self.child) {
            ctx.run_layout(&mut self.child, size);
            ctx.place_child(&mut self.child, Point::ZERO);
        }
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}
