//! What the scheduler fires when a trigger elapses.

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;

/// Implementing this trait is how a consumer plugs its dispatch logic into
/// the engine. The scheduler never inspects the body — it only `await`s
/// [`Action::fire`] when the trigger says so.
///
/// The plugin runtime implements this on a bridge struct that knows how to
/// call into mlua; the marketplace can use [`FnAction`] directly with a
/// plain async closure.
#[async_trait]
pub trait Action: Send + Sync + 'static {
    async fn fire(&self);
}

/// Adapter so a caller can register a plain async closure without declaring
/// a dedicated `Action` impl. Wrap the closure in an `Arc` before passing
/// it to [`crate::Scheduler::register`]: `Arc::new(FnAction(|| async { … }))`.
pub struct FnAction<F>(pub F);

#[async_trait]
impl<F, Fut> Action for FnAction<F>
where
    F:   Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    async fn fire(&self) {
        (self.0)().await
    }
}

/// Convenience alias used by [`crate::Scheduler`] and by [`crate::runner`].
pub type ArcAction = Arc<dyn Action>;
