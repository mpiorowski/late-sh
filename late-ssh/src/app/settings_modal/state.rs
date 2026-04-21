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
    Room,
}

/// Snapshot of one room the user is a member of, flattened to the minimum
/// the modal needs to render + filter. Built by the caller (dashboard/chat
/// code has access to slug/kind/DM peer usernames), so this module stays
/// decoupled from `ChatRoom` and `usernames` lookups.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoomOption {
    pub id: Uuid,
    /// Display label: e.g. `"#general"`, `"#rust-nerds"`, `"@alice"`.
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    Username,
    Theme,
    BackgroundColor,
    DashboardHeader,
    RightSidebar,
    GamesSidebar,
    Country,
    Timezone,
    DirectMessages,
    Mentions,
    GameEvents,
    Bell,
    Cooldown,
    NotifyFormat,
}

impl Row {
    pub const ALL: [Row; 14] = [
        Row::Username,
        Row::Theme,
        Row::BackgroundColor,
        Row::DashboardHeader,
        Row::RightSidebar,
        Row::GamesSidebar,
        Row::Country,
        Row::Timezone,
        Row::DirectMessages,
        Row::Mentions,
        Row::GameEvents,
        Row::Bell,
        Row::Cooldown,
        Row::NotifyFormat,
    ];
}

/// Top-level tab in the settings modal. `Settings` holds every compact
/// row (identity/appearance/location/notifications); `Bio` is a separate
/// full-width pane with the markdown editor + preview; `Favorites` manages
/// the dashboard quick-switch room list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    Settings,
    Bio,
    Favorites,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Settings, Tab::Bio, Tab::Favorites];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Settings => "Settings",
            Tab::Bio => "Bio",
            Tab::Favorites => "Favorites",
        }
    }
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
    selected_tab: Tab,
    row_index: usize,
    editing_username: bool,
    username_input: TextArea<'static>,
    editing_bio: bool,
    bio_input: TextArea<'static>,
    picker: PickerState,
    /// Catalog of rooms the user can pick favorites from. Re-supplied on
    /// every modal open so we always reflect current membership.
    available_rooms: Vec<RoomOption>,
    /// Cursor in the Favorites tab: 0..favorites.len() selects a favorite,
    /// the final slot (favorites.len()) selects the "Add favorite…" row.
    favorites_index: usize,
}

impl SettingsModalState {
    pub fn new(profile_service: ProfileService, user_id: Uuid) -> Self {
        Self {
            profile_service,
            user_id,
            draft: Profile::default(),
            selected_tab: Tab::Settings,
            row_index: 0,
            editing_username: false,
            username_input: new_username_textarea(false),
            editing_bio: false,
            bio_input: new_bio_textarea(false),
            picker: PickerState::default(),
            available_rooms: Vec::new(),
            favorites_index: 0,
        }
    }

    pub fn open_from_profile(
        &mut self,
        profile: &Profile,
        available_rooms: Vec<RoomOption>,
        _modal_width: u16,
    ) {
        self.draft = profile.clone();
        // Drop favorites the user is no longer a member of so the modal never
        // shows ghost entries. Preserve order of the survivors.
        let member_ids: std::collections::HashSet<Uuid> =
            available_rooms.iter().map(|room| room.id).collect();
        self.draft
            .favorite_room_ids
            .retain(|id| member_ids.contains(id));
        self.available_rooms = available_rooms;
        self.selected_tab = Tab::Settings;
        self.row_index = 0;
        self.editing_username = false;
        self.username_input = new_username_textarea(false);
        self.editing_bio = false;
        self.bio_input = bio_textarea_for_readonly_text(&self.draft.bio);
        self.picker = PickerState::default();
        self.favorites_index = 0;
    }

    pub fn selected_tab(&self) -> Tab {
        self.selected_tab
    }

    /// Switch to the neighboring tab. Auto-saves + ends any in-flight bio
    /// edit when leaving the Bio tab so the preview reflects the draft.
    pub fn cycle_tab(&mut self, forward: bool) {
        let idx = Tab::ALL
            .iter()
            .position(|t| *t == self.selected_tab)
            .unwrap_or(0);
        let next_idx = if forward {
            (idx + 1) % Tab::ALL.len()
        } else {
            (idx + Tab::ALL.len() - 1) % Tab::ALL.len()
        };
        let next = Tab::ALL[next_idx];
        if self.selected_tab == Tab::Bio && next != Tab::Bio && self.editing_bio {
            self.stop_bio_edit();
            self.save();
        }
        if self.selected_tab == Tab::Settings && self.editing_username {
            // Leaving the Settings tab mid-username-edit → commit what's typed.
            self.submit_username();
            self.save();
        }
        self.selected_tab = next;
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

    /// Rooms the user is a member of but hasn't favorited yet, filtered by
    /// the picker's current query. Returns references into `available_rooms`
    /// so we don't clone the label on every keystroke.
    pub fn filtered_rooms(&self) -> Vec<&RoomOption> {
        let query = self.picker.query.trim().to_ascii_lowercase();
        let favorited: std::collections::HashSet<&Uuid> =
            self.draft.favorite_room_ids.iter().collect();
        self.available_rooms
            .iter()
            .filter(|room| !favorited.contains(&room.id))
            .filter(|room| query.is_empty() || room.label.to_ascii_lowercase().contains(&query))
            .collect()
    }

    pub fn picker_len(&self) -> usize {
        match self.picker.kind {
            Some(PickerKind::Country) => self.filtered_countries().len(),
            Some(PickerKind::Timezone) => self.filtered_timezones().len(),
            Some(PickerKind::Room) => self.filtered_rooms().len(),
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
        let mut mutated = false;
        match self.picker.kind {
            Some(PickerKind::Country) => {
                let options = self.filtered_countries();
                if let Some(country) = options.get(self.picker.selected_index) {
                    self.draft.country = Some(country.code.to_string());
                    mutated = true;
                }
            }
            Some(PickerKind::Room) => {
                let chosen_id = self
                    .filtered_rooms()
                    .get(self.picker.selected_index)
                    .map(|room| room.id);
                if let Some(id) = chosen_id {
                    self.draft.favorite_room_ids.push(id);
                    // Leave cursor on the freshly-added entry so follow-up
                    // reorders feel continuous.
                    self.favorites_index = self.draft.favorite_room_ids.len().saturating_sub(1);
                    mutated = true;
                }
            }
            Some(PickerKind::Timezone) => {
                let options = self.filtered_timezones();
                if let Some(timezone) = options.get(self.picker.selected_index) {
                    self.draft.timezone = Some((*timezone).to_string());
                    mutated = true;
                }
            }
            None => {}
        }
        self.close_picker();
        if mutated {
            self.save();
        }
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
        self.save();
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
        self.save();
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

    pub fn bio_paste(&mut self) {
        let yank = self.bio_input.yank_text();
        insert_bio_text_limited(&mut self.bio_input, &yank);
    }

    pub fn bio_undo(&mut self) {
        self.bio_input.undo();
    }

    pub fn bio_clear(&mut self) {
        self.bio_input = new_bio_textarea(self.editing_bio);
    }

    pub fn favorites(&self) -> &[Uuid] {
        &self.draft.favorite_room_ids
    }

    pub fn available_rooms(&self) -> &[RoomOption] {
        &self.available_rooms
    }

    /// Number of navigable slots on the Favorites tab: every pinned room
    /// plus the trailing "Add favorite…" row.
    pub fn favorites_slot_count(&self) -> usize {
        self.draft.favorite_room_ids.len() + 1
    }

    pub fn favorites_index(&self) -> usize {
        self.favorites_index
    }

    pub fn favorites_index_is_add_row(&self) -> bool {
        self.favorites_index == self.draft.favorite_room_ids.len()
    }

    pub fn room_label(&self, room_id: Uuid) -> Option<&str> {
        self.available_rooms
            .iter()
            .find(|room| room.id == room_id)
            .map(|room| room.label.as_str())
    }

    pub fn move_favorites_cursor(&mut self, delta: isize) {
        let last = self.favorites_slot_count().saturating_sub(1) as isize;
        self.favorites_index = (self.favorites_index as isize + delta).clamp(0, last) as usize;
    }

    /// Swap the selected favorite with its neighbor (positive `delta` moves
    /// toward the end of the list). No-op on the "Add favorite…" row.
    pub fn reorder_selected_favorite(&mut self, delta: isize) {
        if self.favorites_index_is_add_row() {
            return;
        }
        let len = self.draft.favorite_room_ids.len();
        if len < 2 {
            return;
        }
        let from = self.favorites_index;
        let to = (from as isize + delta).clamp(0, len as isize - 1) as usize;
        if to == from {
            return;
        }
        self.draft.favorite_room_ids.swap(from, to);
        self.favorites_index = to;
        self.save();
    }

    pub fn remove_selected_favorite(&mut self) {
        if self.favorites_index_is_add_row() {
            return;
        }
        let idx = self.favorites_index;
        if idx >= self.draft.favorite_room_ids.len() {
            return;
        }
        self.draft.favorite_room_ids.remove(idx);
        // Keep the cursor stable: if the deleted entry was the last pinned
        // room, fall back onto the "Add favorite…" row.
        if idx >= self.draft.favorite_room_ids.len() {
            self.favorites_index = self.draft.favorite_room_ids.len();
        }
        self.save();
    }

    /// Cycle the value of the currently selected row and auto-persist.
    /// Username/Country/Timezone don't cycle here (they open editors/pickers);
    /// this only fires for the toggle/enum rows.
    pub fn cycle_setting(&mut self, forward: bool) {
        let mutated = match self.selected_row() {
            Row::Theme => {
                let current = self
                    .draft
                    .theme_id
                    .as_deref()
                    .unwrap_or_else(|| theme::normalize_id(""));
                self.draft.theme_id = Some(theme::cycle_id(current, forward).to_string());
                true
            }
            Row::BackgroundColor => {
                self.draft.enable_background_color ^= true;
                true
            }
            Row::DashboardHeader => {
                self.draft.show_dashboard_header ^= true;
                true
            }
            Row::RightSidebar => {
                self.draft.show_right_sidebar ^= true;
                true
            }
            Row::GamesSidebar => {
                self.draft.show_games_sidebar ^= true;
                true
            }
            Row::DirectMessages => {
                toggle_kind(&mut self.draft.notify_kinds, "dms");
                true
            }
            Row::Mentions => {
                toggle_kind(&mut self.draft.notify_kinds, "mentions");
                true
            }
            Row::GameEvents => {
                toggle_kind(&mut self.draft.notify_kinds, "game_events");
                true
            }
            Row::Bell => {
                self.draft.notify_bell ^= true;
                true
            }
            Row::Cooldown => {
                self.draft.notify_cooldown_mins =
                    cycle_cooldown_value(self.draft.notify_cooldown_mins, forward);
                true
            }
            Row::NotifyFormat => {
                self.draft.notify_format = Some(
                    cycle_notify_format(self.draft.notify_format.as_deref(), forward).to_string(),
                );
                true
            }
            _ => false,
        };
        if mutated {
            self.save();
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
                notify_format: self.draft.notify_format.clone(),
                theme_id: Some(
                    self.draft
                        .theme_id
                        .clone()
                        .unwrap_or_else(|| "late".to_string()),
                ),
                enable_background_color: self.draft.enable_background_color,
                show_dashboard_header: self.draft.show_dashboard_header,
                show_right_sidebar: self.draft.show_right_sidebar,
                show_games_sidebar: self.draft.show_games_sidebar,
                favorite_room_ids: self.draft.favorite_room_ids.clone(),
            },
        );
    }
}

fn cycle_notify_format(current: Option<&str>, forward: bool) -> &'static str {
    const OPTIONS: &[&str] = &["both", "osc777", "osc9"];
    let idx = OPTIONS
        .iter()
        .position(|value| Some(*value) == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % OPTIONS.len()
    } else {
        (idx + OPTIONS.len() - 1) % OPTIONS.len()
    };
    OPTIONS[next]
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
