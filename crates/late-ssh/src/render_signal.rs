use std::sync::atomic::AtomicBool;
use tokio::sync::Notify;

/// Paired "there is something unrendered" flag + wakeup. `Notify` is the alarm
/// clock; `dirty` is the source of truth. Set by the input path, the rebels
/// proxy reader, and resize; the render loop clears it before each draw.
///
/// The type is `pub` only so it can appear in the `pub` `ProxyConfig`; all its
/// fields and methods stay `pub(crate)`, so the usable surface is crate-internal.
pub struct RenderSignal {
    pub(crate) dirty: AtomicBool,
    pub(crate) notify: Notify,
}

impl RenderSignal {
    pub(crate) fn new() -> Self {
        Self {
            dirty: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    /// Mark dirty and wake the render loop. Safe to call from any task.
    pub(crate) fn wake(&self) {
        self.dirty.store(true, std::sync::atomic::Ordering::Release);
        self.notify.notify_one();
    }
}
