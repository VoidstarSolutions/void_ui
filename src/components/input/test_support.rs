//! Shared scaffolding for the input family's unit tests.
//!
//! Several of the input modules build a `ViewCtx` (or feed one to a
//! `TestHarness`) to exercise their views off-screen. That needs a `RawProxy`
//! and a tokio runtime, neither of which the tests actually drive — so a single
//! no-op proxy and a current-thread runtime live here rather than being
//! re-declared in each module's test block.

use std::fmt;
use std::sync::Arc;

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
