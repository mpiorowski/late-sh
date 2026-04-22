pub mod catalog;
pub mod nerd_fonts;
pub mod picker;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui_textarea::{CursorMove, Input, TextArea, WrapMode};
use std::cell::Cell;
use std::time::Instant;

use crate::app::common::theme;

/// Fallback when the picker hasn't been rendered yet (first input before
/// first frame). Matches the minimum icon-list height in `picker::render`.
pub const DEFAULT_VISIBLE_HEIGHT: usize = 13;

/// Max gap between two left-clicks (on the same item) to count as a double-click.
pub const DOUBLE_CLICK_WINDOW_MS: u128 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconPickerTab {
    Emoji,
    Unicode,
    NerdFont,
}

impl IconPickerTab {
    pub const ALL: [IconPickerTab; 3] = [
        IconPickerTab::Emoji,
        IconPickerTab::Unicode,
        IconPickerTab::NerdFont,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Emoji => "emoji",
            Self::Unicode => "unicode",
            Self::NerdFont => "nerd font",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub struct IconPickerState {
    pub tab: IconPickerTab,
    pub search_query: TextArea<'static>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    /// Last-rendered icon-list visible height; updated by the renderer each frame.
    pub visible_height: Cell<usize>,
    /// Last-rendered icon-list inner area (0-based, ratatui coords).
    pub list_inner: Cell<Rect>,
    /// Last-rendered tab strip inner area (0-based, ratatui coords).
    pub tabs_inner: Cell<Rect>,
    /// (time, selectable_index) of the previous left-click, for double-click detection.
    pub last_click: Option<(Instant, usize)>,
}

impl Default for IconPickerState {
    fn default() -> Self {
        Self {
            tab: IconPickerTab::Emoji,
            search_query: new_search_textarea(),
            selected_index: 0,
            scroll_offset: 0,
            visible_height: Cell::new(DEFAULT_VISIBLE_HEIGHT),
            list_inner: Cell::new(Rect::new(0, 0, 0, 0)),
            tabs_inner: Cell::new(Rect::new(0, 0, 0, 0)),
            last_click: None,
        }
    }
}

impl IconPickerState {
    pub fn search_str(&self) -> String {
        self.search_query.lines().join("")
    }

    pub fn set_tab(&mut self, tab: IconPickerTab) {
        if self.tab != tab {
            self.tab = tab;
            self.reset_selection();
        }
    }

    pub fn next_tab(&mut self) {
        self.set_tab(self.tab.next());
    }

    pub fn prev_tab(&mut self) {
        self.set_tab(self.tab.prev());
    }

    pub fn search_insert_char(&mut self, ch: char) {
        self.search_query.insert_char(ch);
        self.reset_selection();
    }

    pub fn search_delete_char(&mut self) {
        self.search_query.delete_char();
        self.reset_selection();
    }

    pub fn search_delete_next_char(&mut self) {
        self.search_query.delete_next_char();
        self.reset_selection();
    }

    pub fn search_delete_word_left(&mut self) {
        self.search_query.delete_word();
        self.reset_selection();
    }

    pub fn search_delete_word_right(&mut self) {
        self.search_query.delete_next_word();
        self.reset_selection();
    }

    pub fn search_cursor_left(&mut self) {
        self.search_query.move_cursor(CursorMove::Back);
    }

    pub fn search_cursor_right(&mut self) {
        self.search_query.move_cursor(CursorMove::Forward);
    }

    pub fn search_cursor_word_left(&mut self) {
        self.search_query.move_cursor(CursorMove::WordBack);
    }

    pub fn search_cursor_word_right(&mut self) {
        self.search_query.move_cursor(CursorMove::WordForward);
    }

    pub fn search_cursor_home(&mut self) {
        self.search_query.move_cursor(CursorMove::Head);
    }

    pub fn search_cursor_end(&mut self) {
        self.search_query.move_cursor(CursorMove::End);
    }

    pub fn search_paste(&mut self) {
        self.search_query.paste();
        self.reset_selection();
    }

    pub fn search_undo(&mut self) {
        self.search_query.undo();
        self.reset_selection();
    }

    /// Forward an unclaimed `Input` to ratatui-textarea's emacs keymap
    /// (^A/^E/^K/^F/^B/^Y/...). Any chord that actually modifies the
    /// query resets the icon list selection, matching the hand-wired
    /// behavior of the individual `search_*` helpers above.
    pub fn search_input(&mut self, input: Input) {
        if self.search_query.input(input) {
            self.reset_selection();
        }
    }

    fn reset_selection(&mut self) {
        self.selected_index = 0;
        self.scroll_offset = 0;
        self.last_click = None;
    }
}

fn new_search_textarea() -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_cursor_style(
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    );
    ta.set_style(Style::default().fg(theme::TEXT_BRIGHT()));
    ta.set_wrap_mode(WrapMode::None);
    ta
}
