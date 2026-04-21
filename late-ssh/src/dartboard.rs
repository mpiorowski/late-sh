use dartboard_core::Canvas;
use dartboard_local::{CanvasStore, ServerHandle};

pub const CANVAS_WIDTH: usize = 384;
pub const CANVAS_HEIGHT: usize = 192;

#[derive(Default)]
struct LateShCanvasStore;

impl CanvasStore for LateShCanvasStore {
    fn load(&self) -> Option<Canvas> {
        Some(Canvas::with_size(CANVAS_WIDTH, CANVAS_HEIGHT))
    }

    fn save(&mut self, _canvas: &Canvas) {}
}

pub fn spawn_server() -> ServerHandle {
    ServerHandle::spawn_local(LateShCanvasStore)
}
