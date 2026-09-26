//! The undercity guide's view state: whether it is open over the street
//! and how far it is scrolled. Pure: no I/O, no clock reads. The page
//! height is recorded by the renderer (interior mutability, the help
//! modal's shape) so a scroll past the end holds at the end instead of
//! piling up presses that `k` would have to undo.

use std::cell::Cell;

#[derive(Debug, Default)]
pub struct State {
    open: bool,
    scroll: u16,
    /// The furthest the body may scroll, as the last frame measured it:
    /// the body's line count less its height, floored at zero.
    max_scroll: Cell<u16>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Open at the top: a guide is read from the start every time.
    pub fn open(&mut self) {
        self.open = true;
        self.scroll = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    /// Move the body by `delta` lines, holding at the top and at the end
    /// the renderer last measured.
    pub fn scroll_by(&mut self, delta: i16) {
        let max = self.max_scroll.get();
        self.scroll = self.scroll.saturating_add_signed(delta).min(max);
    }

    /// The renderer's measure of the body: `lines` of copy in `height`
    /// rows. Recorded every frame so a resize is honored on the next key.
    pub fn record_page(&self, lines: u16, height: u16) {
        self.max_scroll.set(lines.saturating_sub(height));
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
