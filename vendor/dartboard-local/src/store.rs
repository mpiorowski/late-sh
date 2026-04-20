use dartboard_core::Canvas;

/// Persistence boundary for the canonical canvas. In-memory by default;
/// the late-sh integration plan describes a postgres-backed impl.
pub trait CanvasStore: Send + Sync {
    /// Called on server startup. Return the canvas to initialize with, or
    /// None for a fresh empty canvas.
    fn load(&self) -> Option<Canvas>;

    /// Called after every applied op. Implementations may debounce or skip.
    fn save(&mut self, canvas: &Canvas);
}

#[derive(Default)]
pub struct InMemStore;

impl CanvasStore for InMemStore {
    fn load(&self) -> Option<Canvas> {
        None
    }

    fn save(&mut self, _canvas: &Canvas) {}
}
