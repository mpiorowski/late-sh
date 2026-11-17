use std::cell::Cell;
use std::time::Instant;

use ratatui::{
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Wrap},
};

use crate::app::common::scroll_view::{ScrollView, ScrollViewport};

use super::data::{HelpTopic, lines_for};

/// Max gap between two left-clicks (on the same topic) to count as a double-click.
pub(crate) const HELP_DOUBLE_CLICK_WINDOW_MS: u128 = 400;

pub(crate) struct HelpModalState {
    selected_topic: HelpTopic,
    /// One scrolled body per topic, so each keeps its place. `views[i]`
    /// corresponds to `HelpTopic::ALL[i]`; the renderer publishes the open
    /// topic's viewport each frame.
    views: [ScrollView; HelpTopic::ALL.len()],
    /// Per-topic on-screen rectangles for the tab strip, populated by the
    /// renderer each frame. `tab_rects[i]` corresponds to `HelpTopic::ALL[i]`.
    tab_rects: Cell<[Rect; HelpTopic::ALL.len()]>,
    last_click: Option<(Instant, HelpTopic)>,
    /// Mirrors the `keep_composer_focused` profile tweak. Drives the Chat
    /// topic's Compose section. Set by callers before opening / on each
    /// frame; the modal itself doesn't own this preference.
    keep_composer_focused: bool,
    /// The `/` prompt while it is open: every key types into it.
    query: Option<String>,
    /// The last query searched, for `n` and `N`.
    last_query: Option<String>,
    /// The line the last search landed on, highlighted while it is shown.
    hit: Option<(HelpTopic, usize)>,
}

/// Which way a search walks the guide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SearchDirection {
    Forward,
    Backward,
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
            views: std::array::from_fn(|_| ScrollView::default()),
            tab_rects: Cell::new([Rect::new(0, 0, 0, 0); HelpTopic::ALL.len()]),
            last_click: None,
            keep_composer_focused: false,
            query: None,
            last_query: None,
            hit: None,
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

    pub(crate) fn current_lines(&self, listen_url: &str) -> Vec<String> {
        lines_for(self.selected_topic, self.keep_composer_focused, listen_url)
    }

    /// The open topic's scrolled body.
    pub(crate) fn view(&self) -> &ScrollView {
        &self.views[self.selected_topic.index()]
    }

    /// Forget every topic's drawn geometry until its next draw (a resize).
    pub(crate) fn invalidate_views(&self) {
        for view in &self.views {
            view.invalidate();
        }
    }

    pub(crate) fn move_topic(&mut self, delta: isize) {
        let len = HelpTopic::ALL.len() as isize;
        let next = (self.selected_topic.index() as isize + delta).rem_euclid(len) as usize;
        self.selected_topic = HelpTopic::ALL[next];
    }

    /// Half the body's height, the step `Ctrl+D`/`Ctrl+U` and the page keys
    /// take; one row on a body too short to halve.
    pub(crate) fn half_page(&self) -> i32 {
        i32::from((self.view().viewport().body.height / 2).max(1))
    }

    /// The `/` prompt's text while it is open.
    pub(crate) fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// The line the last search landed on, while its topic is the one shown.
    pub(crate) fn hit_line(&self) -> Option<usize> {
        match self.hit {
            Some((topic, line)) if topic == self.selected_topic => Some(line),
            Some(_) | None => None,
        }
    }

    pub(crate) fn begin_search(&mut self) {
        self.query = Some(String::new());
    }

    pub(crate) fn push_query(&mut self, ch: char) {
        if let Some(query) = self.query.as_mut() {
            query.push(ch);
        }
    }

    pub(crate) fn pop_query(&mut self) {
        if let Some(query) = self.query.as_mut() {
            query.pop();
        }
    }

    pub(crate) fn cancel_search(&mut self) {
        self.query = None;
    }

    /// Close the prompt and jump to the first match at or after the top of
    /// what is shown. Returns `false` when the guide has no match (a blank
    /// query included), leaving the page where it was.
    pub(crate) fn submit_search(&mut self, listen_url: &str) -> bool {
        let query = self.query.take().unwrap_or_default();
        if query.trim().is_empty() {
            return false;
        }
        self.last_query = Some(query);
        let start = (
            self.selected_topic.index(),
            self.first_shown_line(listen_url),
        );
        self.jump_to_match(start, SearchDirection::Forward, listen_url)
    }

    /// `n`/`N`: the next or previous match of the last query, past the one
    /// shown. Returns `false` with no query or no match.
    pub(crate) fn repeat_search(&mut self, direction: SearchDirection, listen_url: &str) -> bool {
        let topic = self.selected_topic.index();
        let line = match self.hit_line() {
            Some(line) => line,
            None => self.first_shown_line(listen_url),
        };
        let start = match direction {
            SearchDirection::Forward => (topic, line + 1),
            SearchDirection::Backward => (topic, line),
        };
        self.jump_to_match(start, direction, listen_url)
    }

    fn jump_to_match(
        &mut self,
        start: (usize, usize),
        direction: SearchDirection,
        listen_url: &str,
    ) -> bool {
        let Some(query) = self.last_query.as_deref() else {
            return false;
        };
        // Every topic draws in the same body, so the one on the page
        // measures a topic that has not been drawn yet.
        let body = self.view().viewport().body;
        let keep = self.keep_composer_focused;
        let found = find_match(query, start, direction, |topic| {
            lines_for(topic, keep, listen_url)
        });
        let Some((topic, line)) = found else {
            return false;
        };
        self.selected_topic = topic;
        self.hit = Some((topic, line));
        let lines = self.current_lines(listen_url);
        let view = self.view();
        view.set_viewport(ScrollViewport {
            body,
            content_height: wrapped_rows(&lines, body.width),
            ..ScrollViewport::default()
        });
        // The match lands a couple of rows under the top, so the line
        // before it reads as its context.
        view.scroll_to_top();
        view.scroll(i32::from(
            wrapped_rows(&lines[..line], body.width).saturating_sub(2),
        ));
        true
    }

    /// The source line at the top of the body, from the wrapped rows above
    /// it. Each line wraps on its own, so its rows add up line by line.
    fn first_shown_line(&self, listen_url: &str) -> usize {
        let lines = self.current_lines(listen_url);
        let width = self.view().viewport().body.width;
        let scroll = self.view().offset();
        let mut rows: u16 = 0;
        for (index, line) in lines.iter().enumerate() {
            rows = rows.saturating_add(wrapped_rows(std::slice::from_ref(line), width));
            if rows > scroll {
                return index;
            }
        }
        0
    }

    pub(crate) fn set_tab_rects(&self, rects: [Rect; HelpTopic::ALL.len()]) {
        self.tab_rects.set(rects);
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
        rect_contains(self.view().viewport().body, x, y)
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

/// The first line containing `query` (case aside), walking from `start`
/// (topic index into `HelpTopic::ALL`, line index) through the rest of that
/// topic and on round every topic, back to where it began. `Backward`
/// walks the other way and starts at the line before `start`.
pub(crate) fn find_match(
    query: &str,
    start: (usize, usize),
    direction: SearchDirection,
    lines_of: impl Fn(HelpTopic) -> Vec<String>,
) -> Option<(HelpTopic, usize)> {
    let needle = query.to_lowercase();
    let topics = HelpTopic::ALL.len();
    let (start_topic, start_line) = start;
    let matches = |line: &String| line.to_lowercase().contains(&needle);
    // One lap of the topics and one more visit of the first, for the lines
    // the lap skipped on its way out of it.
    for step in 0..=topics {
        let index = match direction {
            SearchDirection::Forward => (start_topic + step) % topics,
            SearchDirection::Backward => (start_topic + topics - step) % topics,
        };
        let topic = HelpTopic::ALL[index];
        let lines = lines_of(topic);
        let found = match (direction, step) {
            (SearchDirection::Forward, 0) => lines
                .iter()
                .enumerate()
                .skip(start_line)
                .find(|(_, line)| matches(line)),
            (SearchDirection::Forward, _) => {
                lines.iter().enumerate().find(|(_, line)| matches(line))
            }
            (SearchDirection::Backward, 0) => lines
                .iter()
                .enumerate()
                .take(start_line)
                .rev()
                .find(|(_, line)| matches(line)),
            (SearchDirection::Backward, _) => lines
                .iter()
                .enumerate()
                .rev()
                .find(|(_, line)| matches(line)),
        };
        if let Some((line, _)) = found {
            return Some((topic, line));
        }
    }
    None
}

/// The rows `lines` take wrapped at `width`, as the body draws them.
fn wrapped_rows(lines: &[String], width: u16) -> u16 {
    if width == 0 {
        return lines.len() as u16;
    }
    let text: Vec<Line> = lines.iter().map(|line| Line::raw(line.as_str())).collect();
    Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .line_count(width)
        .min(u16::MAX as usize) as u16
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}
