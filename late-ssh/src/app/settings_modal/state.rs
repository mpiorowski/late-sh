use std::cell::Cell;

use late_core::models::profile::{Profile, ProfileParams};
use late_core::models::user::sanitize_username_input;
use ratatui::style::{Modifier, Style};
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::common::theme;
use crate::app::profile::svc::ProfileService;

use super::data::{CountryOption, filter_countries, filter_timezones};

const USERNAME_MAX_LEN: usize = 12;
pub const BIO_MAX_LEN: usize = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickerKind {
    Country,
    Timezone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    Username,
    Bio,
    Theme,
    BackgroundColor,
    Country,
    Timezone,
    DirectMessages,
    Mentions,
    GameEvents,
    Bell,
    Cooldown,
    Save,
}

impl Row {
    pub const ALL: [Row; 12] = [
        Row::Username,
        Row::Bio,
        Row::Theme,
        Row::BackgroundColor,
        Row::Country,
        Row::Timezone,
        Row::DirectMessages,
        Row::Mentions,
        Row::GameEvents,
        Row::Bell,
        Row::Cooldown,
        Row::Save,
    ];
}

#[derive(Default)]
pub struct PickerState {
    pub kind: Option<PickerKind>,
    pub query: String,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub visible_height: Cell<usize>,
}

pub struct SettingsModalState {
    profile_service: ProfileService,
    user_id: Uuid,
    draft: Profile,
    row_index: usize,
    editing_username: bool,
    username_input: TextArea<'static>,
    editing_bio: bool,
    bio_input: TextArea<'static>,
    picker: PickerState,
}

impl SettingsModalState {
    pub fn new(profile_service: ProfileService, user_id: Uuid) -> Self {
        Self {
            profile_service,
            user_id,
            draft: Profile::default(),
            row_index: 0,
            editing_username: false,
            username_input: new_username_textarea(false),
            editing_bio: false,
            bio_input: new_bio_textarea(false),
            picker: PickerState::default(),
        }
    }

    pub fn open_from_profile(&mut self, profile: &Profile, _modal_width: u16) {
        self.draft = profile.clone();
        self.row_index = 0;
        self.editing_username = false;
        self.username_input = new_username_textarea(false);
        self.editing_bio = false;
        self.bio_input = bio_textarea_for_readonly_text(&self.draft.bio);
        self.picker = PickerState::default();
    }

    pub fn set_modal_width(&mut self, _modal_width: u16) {
        // TextArea wraps internally at render time; nothing to sync here.
    }

    pub fn draft(&self) -> &Profile {
        &self.draft
    }

    pub fn selected_row(&self) -> Row {
        Row::ALL[self.row_index]
    }

    pub fn move_row(&mut self, delta: isize) {
        let last = Row::ALL.len().saturating_sub(1) as isize;
        self.row_index = (self.row_index as isize + delta).clamp(0, last) as usize;
    }

    pub fn editing_username(&self) -> bool {
        self.editing_username
    }

    pub fn editing_bio(&self) -> bool {
        self.editing_bio
    }

    pub fn username_input(&self) -> &TextArea<'static> {
        &self.username_input
    }

    fn username_text(&self) -> String {
        self.username_input.lines().join("")
    }

    fn username_char_count(&self) -> usize {
        self.username_input
            .lines()
            .iter()
            .map(|l| l.chars().count())
            .sum()
    }

    pub fn bio_input(&self) -> &TextArea<'static> {
        &self.bio_input
    }

    fn bio_text(&self) -> String {
        self.bio_input.lines().join("\n")
    }

    fn bio_char_count(&self) -> usize {
        self.bio_input
            .lines()
            .iter()
            .map(|l| l.chars().count())
            .sum::<usize>()
            + self.bio_input.lines().len().saturating_sub(1) // count newlines between lines
    }

    pub fn picker(&self) -> &PickerState {
        &self.picker
    }

    pub fn picker_open(&self) -> bool {
        self.picker.kind.is_some()
    }

    pub fn open_picker(&mut self, kind: PickerKind) {
        self.picker.kind = Some(kind);
        self.picker.query.clear();
        self.picker.selected_index = 0;
        self.picker.scroll_offset = 0;
    }

    pub fn close_picker(&mut self) {
        self.picker = PickerState::default();
    }

    pub fn filtered_countries(&self) -> Vec<&'static CountryOption> {
        filter_countries(&self.picker.query)
    }

    pub fn filtered_timezones(&self) -> Vec<&'static str> {
        filter_timezones(&self.picker.query)
    }

    pub fn picker_len(&self) -> usize {
        match self.picker.kind {
            Some(PickerKind::Country) => self.filtered_countries().len(),
            Some(PickerKind::Timezone) => self.filtered_timezones().len(),
            None => 0,
        }
    }

    pub fn picker_move(&mut self, delta: isize) {
        let len = self.picker_len();
        if len == 0 {
            self.picker.selected_index = 0;
            self.picker.scroll_offset = 0;
            return;
        }
        let next = (self.picker.selected_index as isize + delta).clamp(0, len as isize - 1);
        self.picker.selected_index = next as usize;
        let visible = self.picker.visible_height.get().max(1);
        if self.picker.selected_index < self.picker.scroll_offset {
            self.picker.scroll_offset = self.picker.selected_index;
        } else if self.picker.selected_index >= self.picker.scroll_offset + visible {
            self.picker.scroll_offset = self.picker.selected_index.saturating_sub(visible - 1);
        }
    }

    pub fn picker_push(&mut self, ch: char) {
        self.picker.query.push(ch);
        self.picker.selected_index = 0;
        self.picker.scroll_offset = 0;
    }

    pub fn picker_backspace(&mut self) {
        self.picker.query.pop();
        self.picker.selected_index = 0;
        self.picker.scroll_offset = 0;
    }

    pub fn apply_picker_selection(&mut self) {
        match self.picker.kind {
            Some(PickerKind::Country) => {
                let options = self.filtered_countries();
                if let Some(country) = options.get(self.picker.selected_index) {
                    self.draft.country = Some(country.code.to_string());
                }
            }
            Some(PickerKind::Timezone) => {
                let options = self.filtered_timezones();
                if let Some(timezone) = options.get(self.picker.selected_index) {
                    self.draft.timezone = Some((*timezone).to_string());
                }
            }
            None => {}
        }
        self.close_picker();
    }

    pub fn start_username_edit(&mut self) {
        self.editing_username = true;
        self.username_input = new_username_textarea(true);
        self.username_input.insert_str(&self.draft.username);
    }

    pub fn cancel_username_edit(&mut self) {
        self.editing_username = false;
        self.username_input = new_username_textarea(false);
    }

    pub fn submit_username(&mut self) {
        self.editing_username = false;
        let normalized = sanitize_username_input(self.username_text().trim());
        self.username_input = new_username_textarea(false);
        self.draft.username = normalized;
    }

    pub fn username_push(&mut self, ch: char) {
        if self.username_char_count() < USERNAME_MAX_LEN {
            self.username_input.insert_char(ch);
        }
    }

    pub fn username_backspace(&mut self) {
        self.username_input.delete_char();
    }

    pub fn username_delete_right(&mut self) {
        self.username_input.delete_next_char();
    }

    pub fn username_delete_word_left(&mut self) {
        self.username_input.delete_word();
    }

    pub fn username_delete_word_right(&mut self) {
        self.username_input.delete_next_word();
    }

    pub fn username_cursor_left(&mut self) {
        self.username_input.move_cursor(CursorMove::Back);
    }

    pub fn username_cursor_right(&mut self) {
        self.username_input.move_cursor(CursorMove::Forward);
    }

    pub fn username_cursor_word_left(&mut self) {
        self.username_input.move_cursor(CursorMove::WordBack);
    }

    pub fn username_cursor_word_right(&mut self) {
        self.username_input.move_cursor(CursorMove::WordForward);
    }

    pub fn username_cursor_home(&mut self) {
        self.username_input.move_cursor(CursorMove::Head);
    }

    pub fn username_cursor_end(&mut self) {
        self.username_input.move_cursor(CursorMove::End);
    }

    pub fn username_paste(&mut self) {
        let yank = self.username_input.yank_text();
        insert_username_text_limited(&mut self.username_input, &yank);
    }

    pub fn username_undo(&mut self) {
        self.username_input.undo();
    }

    pub fn clear_username(&mut self) {
        let editing = self.editing_username;
        self.username_input = new_username_textarea(editing);
    }

    pub fn start_bio_edit(&mut self) {
        self.editing_bio = true;
        move_bio_cursor_to_end(&mut self.bio_input);
        set_bio_cursor_visible(&mut self.bio_input, true);
    }

    pub fn stop_bio_edit(&mut self) {
        self.editing_bio = false;
        self.draft.bio = self.bio_text().trim_end().to_string();
        reset_bio_view_to_top(&mut self.bio_input);
        set_bio_cursor_visible(&mut self.bio_input, false);
    }

    pub fn bio_push(&mut self, ch: char) {
        if self.bio_char_count() < BIO_MAX_LEN {
            self.bio_input.insert_char(ch);
        }
    }

    pub fn bio_backspace(&mut self) {
        self.bio_input.delete_char();
    }

    pub fn bio_delete_right(&mut self) {
        self.bio_input.delete_next_char();
    }

    pub fn bio_delete_word_left(&mut self) {
        self.bio_input.delete_word();
    }

    pub fn bio_delete_word_right(&mut self) {
        self.bio_input.delete_next_word();
    }

    pub fn bio_cursor_left(&mut self) {
        self.bio_input.move_cursor(CursorMove::Back);
    }

    pub fn bio_cursor_right(&mut self) {
        self.bio_input.move_cursor(CursorMove::Forward);
    }

    pub fn bio_cursor_up(&mut self) {
        self.bio_input.move_cursor(CursorMove::Up);
    }

    pub fn bio_cursor_down(&mut self) {
        self.bio_input.move_cursor(CursorMove::Down);
    }

    pub fn bio_cursor_word_left(&mut self) {
        self.bio_input.move_cursor(CursorMove::WordBack);
    }

    pub fn bio_cursor_word_right(&mut self) {
        self.bio_input.move_cursor(CursorMove::WordForward);
    }

    pub fn bio_clear(&mut self) {
        self.bio_input = new_bio_textarea(self.editing_bio);
    }

    pub fn bio_paste(&mut self) {
        let yank = self.bio_input.yank_text();
        insert_bio_text_limited(&mut self.bio_input, &yank);
    }

    pub fn bio_undo(&mut self) {
        self.bio_input.undo();
    }

    pub fn cycle_setting(&mut self, forward: bool) {
        match self.selected_row() {
            Row::Theme => {
                let current = self
                    .draft
                    .theme_id
                    .as_deref()
                    .unwrap_or_else(|| theme::normalize_id(""));
                self.draft.theme_id = Some(theme::cycle_id(current, forward).to_string());
            }
            Row::BackgroundColor => {
                self.draft.enable_background_color ^= true;
            }
            Row::DirectMessages => toggle_kind(&mut self.draft.notify_kinds, "dms"),
            Row::Mentions => toggle_kind(&mut self.draft.notify_kinds, "mentions"),
            Row::GameEvents => toggle_kind(&mut self.draft.notify_kinds, "game_events"),
            Row::Bell => self.draft.notify_bell ^= true,
            Row::Cooldown => {
                self.draft.notify_cooldown_mins =
                    cycle_cooldown_value(self.draft.notify_cooldown_mins, forward);
            }
            _ => {}
        }
    }

    pub fn save(&self) {
        self.profile_service.edit_profile(
            self.user_id,
            ProfileParams {
                username: self.draft.username.clone(),
                bio: self.draft.bio.clone(),
                country: self.draft.country.clone(),
                timezone: self.draft.timezone.clone(),
                notify_kinds: self.draft.notify_kinds.clone(),
                notify_bell: self.draft.notify_bell,
                notify_cooldown_mins: self.draft.notify_cooldown_mins,
                theme_id: Some(
                    self.draft
                        .theme_id
                        .clone()
                        .unwrap_or_else(|| "late".to_string()),
                ),
                enable_background_color: self.draft.enable_background_color,
            },
        );
    }
}

fn toggle_kind(kinds: &mut Vec<String>, kind: &str) {
    if let Some(idx) = kinds.iter().position(|value| value == kind) {
        kinds.remove(idx);
    } else {
        kinds.push(kind.to_string());
    }
}

fn cycle_cooldown_value(current: i32, forward: bool) -> i32 {
    const OPTIONS: &[i32] = &[0, 1, 2, 5, 10, 15, 30, 60, 120, 240];
    let idx = OPTIONS
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % OPTIONS.len()
    } else {
        (idx + OPTIONS.len() - 1) % OPTIONS.len()
    };
    OPTIONS[next]
}

fn bio_char_count_for_input(input: &TextArea<'static>) -> usize {
    input
        .lines()
        .iter()
        .map(|l| l.chars().count())
        .sum::<usize>()
        + input.lines().len().saturating_sub(1)
}

fn username_char_count_for_input(input: &TextArea<'static>) -> usize {
    input.lines().iter().map(|l| l.chars().count()).sum()
}

fn insert_username_text_limited(input: &mut TextArea<'static>, text: &str) {
    for ch in text.chars() {
        if username_char_count_for_input(input) >= USERNAME_MAX_LEN {
            break;
        }
        if !ch.is_control() && ch != '\n' && ch != '\r' {
            input.insert_char(ch);
        }
    }
}

fn insert_bio_text_limited(input: &mut TextArea<'static>, text: &str) {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for ch in normalized.chars() {
        if bio_char_count_for_input(input) >= BIO_MAX_LEN {
            break;
        }
        if ch == '\n' || (!ch.is_control() && ch != '\u{7f}') {
            input.insert_char(ch);
        }
    }
}

fn reset_bio_view_to_top(input: &mut TextArea<'static>) {
    input.move_cursor(CursorMove::Top);
    input.move_cursor(CursorMove::Head);
}

fn move_bio_cursor_to_end(input: &mut TextArea<'static>) {
    input.move_cursor(CursorMove::Bottom);
    input.move_cursor(CursorMove::End);
}

fn bio_textarea_for_readonly_text(text: &str) -> TextArea<'static> {
    let mut input = new_bio_textarea(false);
    input.insert_str(text);
    reset_bio_view_to_top(&mut input);
    input
}

fn new_bio_textarea(editing: bool) -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_wrap_mode(WrapMode::Word);
    set_bio_cursor_visible(&mut ta, editing);
    ta
}

fn set_bio_cursor_visible(ta: &mut TextArea<'static>, visible: bool) {
    let style = if visible {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    ta.set_cursor_style(style);
}

fn new_username_textarea(editing: bool) -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_wrap_mode(WrapMode::None);
    let style = if editing {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    ta.set_cursor_style(style);
    ta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_yank_respects_max_length() {
        let mut input = new_username_textarea(true);
        input.insert_str("abcdefghijk");
        input.set_yank_text("xyz");
        let yank = input.yank_text();

        insert_username_text_limited(&mut input, &yank);

        assert_eq!(input.lines().join(""), "abcdefghijkx");
        assert_eq!(username_char_count_for_input(&input), USERNAME_MAX_LEN);
    }

    #[test]
    fn bio_yank_respects_max_length() {
        let mut input = new_bio_textarea(true);
        input.insert_str("a".repeat(BIO_MAX_LEN - 1));
        input.set_yank_text("xyz");
        let yank = input.yank_text();

        insert_bio_text_limited(&mut input, &yank);

        assert_eq!(bio_char_count_for_input(&input), BIO_MAX_LEN);
        assert_eq!(
            input.lines().join(""),
            format!("{}x", "a".repeat(BIO_MAX_LEN - 1))
        );
    }

    #[test]
    fn readonly_bio_textarea_resets_cursor_to_top() {
        let input = bio_textarea_for_readonly_text("first line\nsecond line\nthird line");
        assert_eq!(input.cursor(), (0usize, 0usize));
    }

    #[test]
    fn move_bio_cursor_to_end_goes_to_last_line_end() {
        let mut input = bio_textarea_for_readonly_text("first line\nsecond line\nthird line");

        move_bio_cursor_to_end(&mut input);

        assert_eq!(input.cursor(), (2usize, "third line".chars().count()));
    }
}
