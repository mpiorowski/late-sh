use std::cell::Cell;
use std::time::Instant;

use ratatui::layout::Rect;

use super::data::{HelpTopic, lines_for};

/// Max gap between two left-clicks (on the same topic) to count as a double-click.
pub(crate) const HELP_DOUBLE_CLICK_WINDOW_MS: u128 = 400;

pub(crate) struct HelpModalState {
    selected_topic: HelpTopic,
    scroll_offsets: [u16; HelpTopic::ALL.len()],
    /// Per-topic on-screen rectangles for the tab strip, populated by the
    /// renderer each frame. `tab_rects[i]` corresponds to `HelpTopic::ALL[i]`.
    tab_rects: Cell<[Rect; HelpTopic::ALL.len()]>,
    /// Bounds of the body area (text body for the current topic). Used to
    /// gate scroll-wheel events.
    body_area: Cell<Rect>,
    last_click: Option<(Instant, HelpTopic)>,
    /// Mirrors the `keep_composer_focused` profile tweak. Drives the Chat
    /// topic's Compose section. Set by callers before opening / on each
    /// frame; the modal itself doesn't own this preference.
    keep_composer_focused: bool,
}

impl Default for HelpModalState {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpModalState {
    pub(crate) fn new() -> Self {
        Self {
            selected_topic: HelpTopic::Pair,
            scroll_offsets: [0; HelpTopic::ALL.len()],
            tab_rects: Cell::new([Rect::new(0, 0, 0, 0); HelpTopic::ALL.len()]),
            body_area: Cell::new(Rect::new(0, 0, 0, 0)),
            last_click: None,
            keep_composer_focused: false,
        }
    }

    pub(crate) fn open(&mut self, topic: HelpTopic) {
        self.selected_topic = topic;
    }

    pub(crate) fn set_keep_composer_focused(&mut self, value: bool) {
        self.keep_composer_focused = value;
    }

    pub(crate) fn selected_topic(&self) -> HelpTopic {
        self.selected_topic
    }

    pub(crate) fn current_lines(&self, pair_url: &str) -> Vec<String> {
        lines_for(self.selected_topic, self.keep_composer_focused, pair_url)
    }

    pub(crate) fn current_scroll(&self) -> u16 {
        self.scroll_offsets[self.selected_topic.index()]
    }

    pub(crate) fn move_topic(&mut self, delta: isize) {
        let len = HelpTopic::ALL.len() as isize;
        let next = (self.selected_topic.index() as isize + delta).rem_euclid(len) as usize;
        self.selected_topic = HelpTopic::ALL[next];
    }

    pub(crate) fn scroll(&mut self, delta: i16) {
        let idx = self.selected_topic.index();
        let current = self.scroll_offsets[idx] as i32;
        self.scroll_offsets[idx] = (current + delta as i32).max(0) as u16;
    }

    pub(crate) fn set_tab_rects(&self, rects: [Rect; HelpTopic::ALL.len()]) {
        self.tab_rects.set(rects);
    }

    pub(crate) fn set_body_area(&self, area: Rect) {
        self.body_area.set(area);
    }

    pub(crate) fn topic_at_point(&self, x: u16, y: u16) -> Option<HelpTopic> {
        let rects = self.tab_rects.get();
        rects.iter().enumerate().find_map(|(idx, rect)| {
            if rect_contains(*rect, x, y) {
                Some(HelpTopic::ALL[idx])
            } else {
                None
            }
        })
    }

    pub(crate) fn body_contains(&self, x: u16, y: u16) -> bool {
        rect_contains(self.body_area.get(), x, y)
    }

    /// Switch to the clicked topic. Returns `true` if it chained with the
    /// previous click on the same topic within the double-click window.
    pub(crate) fn click_topic(&mut self, topic: HelpTopic) -> bool {
        let now = Instant::now();
        let is_double = match self.last_click {
            Some((prev_time, prev_topic)) => {
                prev_topic == topic
                    && now.duration_since(prev_time).as_millis() <= HELP_DOUBLE_CLICK_WINDOW_MS
            }
            None => false,
        };
        self.selected_topic = topic;
        self.last_click = if is_double { None } else { Some((now, topic)) };
        is_double
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}
