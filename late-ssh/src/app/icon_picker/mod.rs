pub mod catalog;
pub mod nerd_fonts;
pub mod picker;

use ratatui::layout::Rect;
use std::cell::Cell;
use std::time::Instant;

/// Fallback when the picker hasn't been rendered yet (first input before
/// first frame). Matches the minimum icon-list height in `picker::render`.
pub const DEFAULT_VISIBLE_HEIGHT: usize = 13;

/// Max gap between two left-clicks (on the same item) to count as a double-click.
pub const DOUBLE_CLICK_WINDOW_MS: u128 = 400;

#[derive(Debug, Clone)]
pub struct IconPickerState {
    pub search_query: String,
    pub search_cursor: usize,
    pub selected_index: usize,
    pub scroll_offset: usize,
    /// Last-rendered icon-list visible height; updated by the renderer each frame.
    pub visible_height: Cell<usize>,
    /// Last-rendered icon-list inner area (0-based, ratatui coords).
    pub list_inner: Cell<Rect>,
    /// (time, selectable_index) of the previous left-click, for double-click detection.
    pub last_click: Option<(Instant, usize)>,
}

impl Default for IconPickerState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            search_cursor: 0,
            selected_index: 0,
            scroll_offset: 0,
            visible_height: Cell::new(DEFAULT_VISIBLE_HEIGHT),
            list_inner: Cell::new(Rect::new(0, 0, 0, 0)),
            last_click: None,
        }
    }
}
