use std::collections::VecDeque;

use ratatui_textarea::{Input, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::chat::state::{MentionAutocomplete, MentionMatch};
use crate::app::common::composer;
use crate::moderation::command::mod_help_lines;

const MAX_LOG_LINES: usize = 1000;
const COMMAND_SEPARATOR: &str = "───────────";

pub struct ModModalState {
    command_input: TextArea<'static>,
    log: VecDeque<ModLogLine>,
    scroll: usize,
    screen_start: usize,
    mention_ac: MentionAutocomplete,
    has_opened: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModLogLine {
    pub text: String,
    pub kind: ModLogKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModLogKind {
    Input,
    Separator,
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
            screen_start: 0,
            mention_ac: MentionAutocomplete::default(),
            has_opened: false,
        }
    }

    pub fn open(&mut self, can_moderate: bool) {
        composer::set_themed_textarea_cursor_visible(&mut self.command_input, true);
        if self.has_opened {
            return;
        }
        self.has_opened = true;
        if can_moderate {
            self.append_help();
        } else {
            self.append_error("access denied: moderator or admin only");
        }
    }

    pub fn command_input(&self) -> &TextArea<'static> {
        &self.command_input
    }

    pub fn log(&self) -> &VecDeque<ModLogLine> {
        &self.log
    }

    pub fn viewport_start(&self, height: usize) -> usize {
        let len = self.log.len();
        if height == 0 {
            return len;
        }
        let screen_bottom_start = self.screen_start.min(len).max(len.saturating_sub(height));
        screen_bottom_start.saturating_sub(self.scroll)
    }

    pub fn command_text(&self) -> String {
        self.command_input.lines().join(" ").trim().to_string()
    }

    pub fn clear_command(&mut self) {
        self.command_input = new_command_input();
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn clear_screen(&mut self) {
        self.screen_start = self.log.len();
        self.scroll = 0;
    }

    pub fn input(&mut self, input: Input) {
        self.command_input.input(input);
    }

    pub fn is_autocomplete_active(&self) -> bool {
        self.mention_ac.active
    }

    pub fn autocomplete_matches(&self) -> &[MentionMatch] {
        &self.mention_ac.matches
    }

    pub fn autocomplete_selected(&self) -> usize {
        self.mention_ac.selected
    }

    pub fn autocomplete_query(&self) -> Option<(usize, char, String)> {
        let text = self.command_text();
        let bytes = text.as_bytes();
        for i in (0..bytes.len()).rev() {
            if matches!(bytes[i], b'@' | b'#') {
                if i == 0 || bytes[i - 1].is_ascii_whitespace() {
                    return Some((i, bytes[i] as char, text[i + 1..].to_string()));
                }
                break;
            }
            if bytes[i].is_ascii_whitespace() {
                break;
            }
        }
        None
    }

    pub fn update_autocomplete_matches(
        &mut self,
        trigger_offset: usize,
        query: String,
        matches: Vec<MentionMatch>,
    ) {
        if matches.is_empty() {
            self.mention_ac = MentionAutocomplete::default();
            return;
        }
        self.mention_ac.active = true;
        self.mention_ac.query = query;
        self.mention_ac.trigger_offset = trigger_offset;
        self.mention_ac.selected = self
            .mention_ac
            .selected
            .min(matches.len().saturating_sub(1));
        self.mention_ac.matches = matches;
    }

    pub fn ac_move_selection(&mut self, delta: isize) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let len = self.mention_ac.matches.len() as isize;
        let cur = self.mention_ac.selected as isize;
        self.mention_ac.selected = (cur + delta).clamp(0, len - 1) as usize;
    }

    pub fn ac_confirm(&mut self) {
        if !self.mention_ac.active || self.mention_ac.matches.is_empty() {
            return;
        }
        let selected = &self.mention_ac.matches[self.mention_ac.selected];
        let text = self.command_text();
        let next = format!(
            "{}{}{} ",
            &text[..self.mention_ac.trigger_offset],
            selected.prefix,
            selected.name
        );
        self.command_input = new_command_input();
        self.command_input.insert_str(next);
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn ac_dismiss(&mut self) {
        self.mention_ac = MentionAutocomplete::default();
    }

    pub fn append_input(&mut self, command: &str) {
        if !self.log.is_empty()
            && self
                .log
                .back()
                .is_none_or(|line| line.kind != ModLogKind::Separator)
        {
            self.push_log(COMMAND_SEPARATOR.to_string(), ModLogKind::Separator);
        }
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

    fn append_help(&mut self) {
        for line in mod_help_lines(None) {
            self.append_info(line);
        }
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
            self.scroll = self.scroll.saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.scroll = self.scroll.saturating_add(delta as usize);
        }
    }

    fn push_log(&mut self, text: String, kind: ModLogKind) {
        self.log.push_back(ModLogLine { text, kind });
        while self.log.len() > MAX_LOG_LINES {
            self.log.pop_front();
            self.screen_start = self.screen_start.saturating_sub(1);
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

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;

