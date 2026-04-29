use std::collections::VecDeque;

use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::common::composer;

const MAX_LOG_LINES: usize = 300;

pub struct ModModalState {
    command_input: TextArea<'static>,
    log: VecDeque<ModLogLine>,
    scroll: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModLogLine {
    pub text: String,
    pub kind: ModLogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModLogKind {
    Input,
    Info,
    Success,
    Error,
}

impl ModModalState {
    pub fn new() -> Self {
        Self {
            command_input: new_command_input(),
            log: VecDeque::new(),
            scroll: 0,
        }
    }

    pub fn open(&mut self, can_moderate: bool) {
        composer::set_themed_textarea_cursor_visible(&mut self.command_input, true);
        if self.log.is_empty() {
            if can_moderate {
                self.append_info("type help for commands");
            } else {
                self.append_error("access denied: moderator or admin only");
            }
        }
    }

    pub fn command_input(&self) -> &TextArea<'static> {
        &self.command_input
    }

    pub fn log(&self) -> &VecDeque<ModLogLine> {
        &self.log
    }

    pub fn scroll(&self) -> u16 {
        self.scroll
    }

    pub fn command_text(&self) -> String {
        self.command_input.lines().join(" ").trim().to_string()
    }

    pub fn clear_command(&mut self) {
        self.command_input = new_command_input();
    }

    pub fn clear_log(&mut self) {
        self.log.clear();
        self.scroll = 0;
    }

    pub fn push_char(&mut self, ch: char) {
        self.command_input.insert_char(ch);
    }

    pub fn backspace(&mut self) {
        self.command_input.delete_char();
    }

    pub fn delete_right(&mut self) {
        self.command_input.delete_next_char();
    }

    pub fn delete_word_left(&mut self) {
        self.command_input.delete_word();
    }

    pub fn move_left(&mut self) {
        self.command_input.move_cursor(CursorMove::Back);
    }

    pub fn move_right(&mut self) {
        self.command_input.move_cursor(CursorMove::Forward);
    }

    pub fn move_word_left(&mut self) {
        self.command_input.move_cursor(CursorMove::WordBack);
    }

    pub fn move_word_right(&mut self) {
        self.command_input.move_cursor(CursorMove::WordForward);
    }

    pub fn append_input(&mut self, command: &str) {
        self.push_log(format!("> {command}"), ModLogKind::Input);
    }

    pub fn append_pending(&mut self, request_id: Uuid) {
        self.push_log(format!("running... {request_id}"), ModLogKind::Info);
    }

    pub fn append_info(&mut self, line: impl Into<String>) {
        self.push_log(line.into(), ModLogKind::Info);
    }

    pub fn append_error(&mut self, line: impl Into<String>) {
        self.push_log(line.into(), ModLogKind::Error);
    }

    pub fn append_result(&mut self, success: bool, lines: Vec<String>) {
        let kind = if success {
            ModLogKind::Success
        } else {
            ModLogKind::Error
        };
        for line in lines {
            self.push_log(line, kind);
        }
    }

    pub fn scroll_log(&mut self, delta: i16) {
        if delta < 0 {
            self.scroll = self.scroll.saturating_sub(delta.unsigned_abs());
        } else {
            self.scroll = self.scroll.saturating_add(delta as u16);
        }
    }

    fn push_log(&mut self, text: String, kind: ModLogKind) {
        self.log.push_back(ModLogLine { text, kind });
        while self.log.len() > MAX_LOG_LINES {
            self.log.pop_front();
        }
        self.scroll = 0;
    }
}

impl Default for ModModalState {
    fn default() -> Self {
        Self::new()
    }
}

fn new_command_input() -> TextArea<'static> {
    composer::new_themed_textarea("mod command", WrapMode::None, true)
}
