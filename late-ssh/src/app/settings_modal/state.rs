use std::cell::Cell;

use late_core::models::profile::{Profile, ProfileParams, normalize_profile_tags};
use late_core::models::rss_feed::RssFeed;
use late_core::models::user::sanitize_username_input;
use ratatui::style::{Modifier, Style};
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::theme;
use crate::app::profile::svc::ProfileService;
use crate::app::{
    chat::feeds::svc::{FeedEvent, FeedService, FeedSnapshot},
    common::primitives::Banner,
};

use super::data::{CountryOption, filter_countries, filter_timezones};
use super::gem::GemState;

const USERNAME_MAX_LEN: usize = 12;
const SYSTEM_FIELD_MAX_LEN: usize = 48;
const FEED_URL_MAX_LEN: usize = 2000;
pub const BIO_MAX_LEN: usize = 1000;

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
    Ide,
    Terminal,
    Os,
    Langs,
    Theme,
    BackgroundColor,
    DashboardHeader,
    RightSidebar,
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
    pub const ALL: [Row; 17] = [
        Row::Username,
        Row::Ide,
        Row::Terminal,
        Row::Os,
        Row::Langs,
        Row::Theme,
        Row::BackgroundColor,
        Row::DashboardHeader,
        Row::RightSidebar,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemField {
    Ide,
    Terminal,
    Os,
    Langs,
}

impl SystemField {
    pub(crate) fn from_row(row: Row) -> Option<Self> {
        match row {
            Row::Ide => Some(Self::Ide),
            Row::Terminal => Some(Self::Terminal),
            Row::Os => Some(Self::Os),
            Row::Langs => Some(Self::Langs),
            _ => None,
        }
    }

    fn value(self, profile: &Profile) -> Option<String> {
        match self {
            Self::Ide => profile.ide.clone(),
            Self::Terminal => profile.terminal.clone(),
            Self::Os => profile.os.clone(),
            Self::Langs => (!profile.langs.is_empty()).then(|| profile.langs.join(", ")),
        }
    }

    fn set_value(self, profile: &mut Profile, text: String) {
        match self {
            Self::Ide => profile.ide = normalize_optional_text(&text),
            Self::Terminal => profile.terminal = normalize_optional_text(&text),
            Self::Os => profile.os = normalize_optional_text(&text),
            Self::Langs => {
                profile.langs = normalize_profile_tags([text.as_str()]);
            }
        }
    }
}

/// Top-level tab in the settings modal. `Settings` holds every compact row
/// (identity/appearance/location/notifications); `Themes` is a fast browser
/// for the expanded theme catalog; `Bio` is a separate full-width pane with
/// the markdown editor + preview; `Favorites` manages the dashboard
/// quick-switch room list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    Settings,
    Bio,
    Themes,
    Favorites,
    Feeds,
    /// Hidden until the user has filled out at least one of bio, country,
    /// or timezone. Currently houses the "Show settings on connect" toggle.
    Special,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Settings,
        Tab::Bio,
        Tab::Themes,
        Tab::Favorites,
        Tab::Feeds,
        Tab::Special,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Settings => "Settings",
            Tab::Bio => "Bio",
            Tab::Themes => "Themes",
            Tab::Favorites => "Favorites",
            Tab::Feeds => "Feeds",
            Tab::Special => "Special",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeTreeRow {
    Group {
        group: theme::ThemeGroup,
        collapsed: bool,
    },
    Theme {
        option_index: usize,
        last_in_group: bool,
    },
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
    feed_service: FeedService,
    user_id: Uuid,
    draft: Profile,
    selected_tab: Tab,
    row_index: usize,
    theme_index: usize,
    theme_selected_row: usize,
    theme_scroll_offset: usize,
    theme_visible_height: Cell<usize>,
    theme_collapsed_groups: u16,
    editing_username: bool,
    username_input: TextArea<'static>,
    editing_system_field: Option<SystemField>,
    system_input: TextArea<'static>,
    editing_bio: bool,
    bio_input: TextArea<'static>,
    picker: PickerState,
    /// Catalog of rooms the user can pick favorites from. Re-supplied on
    /// every modal open so we always reflect current membership.
    available_rooms: Vec<RoomOption>,
    /// Cursor in the Favorites tab: 0..favorites.len() selects a favorite,
    /// the final slot (favorites.len()) selects the "Add favorite…" row.
    favorites_index: usize,
    feeds: Vec<RssFeed>,
    feed_index: usize,
    editing_feed_url: bool,
    feed_url_input: TextArea<'static>,
    feed_snapshot_rx: watch::Receiver<FeedSnapshot>,
    feed_event_rx: broadcast::Receiver<FeedEvent>,
    /// Per-session gem easter egg on the Special tab. Persists across modal
    /// open/close cycles for the lifetime of the SSH session.
    gem: GemState,
}

impl SettingsModalState {
    pub fn new(profile_service: ProfileService, feed_service: FeedService, user_id: Uuid) -> Self {
        let feed_snapshot_rx = feed_service.subscribe_snapshot();
        let feed_event_rx = feed_service.subscribe_events();
        feed_service.list_task(user_id);
        Self {
            profile_service,
            feed_service,
            user_id,
            draft: Profile::default(),
            selected_tab: Tab::Settings,
            row_index: 0,
            theme_index: 0,
            theme_selected_row: 0,
            theme_scroll_offset: 0,
            theme_visible_height: Cell::new(1),
            theme_collapsed_groups: 0,
            editing_username: false,
            username_input: new_username_textarea(false),
            editing_system_field: None,
            system_input: new_short_textarea(false),
            editing_bio: false,
            bio_input: new_bio_textarea(false),
            picker: PickerState::default(),
            available_rooms: Vec::new(),
            favorites_index: 0,
            feeds: Vec::new(),
            feed_index: 0,
            editing_feed_url: false,
            feed_url_input: new_short_textarea(false),
            feed_snapshot_rx,
            feed_event_rx,
            gem: GemState::new(),
        }
    }

    pub fn gem(&self) -> &GemState {
        &self.gem
    }

    pub fn gem_mut(&mut self) -> &mut GemState {
        &mut self.gem
    }

    pub fn open_from_profile(
        &mut self,
        profile: &Profile,
        available_rooms: Vec<RoomOption>,
        _modal_width: u16,
    ) {
        self.draft = profile.clone();
        prune_favorites_against_loaded_rooms(&mut self.draft.favorite_room_ids, &available_rooms);
        self.available_rooms = available_rooms;
        self.selected_tab = Tab::Settings;
        self.row_index = 0;
        self.sync_theme_index_to_draft();
        self.editing_username = false;
        self.username_input = new_username_textarea(false);
        self.editing_system_field = None;
        self.system_input = new_short_textarea(false);
        self.editing_bio = false;
        self.bio_input = bio_textarea_for_readonly_text(&self.draft.bio);
        self.picker = PickerState::default();
        self.favorites_index = 0;
        self.feed_service.list_task(self.user_id);
    }

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_feed_snapshot();
        self.drain_feed_events()
    }

    pub fn selected_tab(&self) -> Tab {
        self.selected_tab
    }

    /// Switch to the neighboring tab. Auto-saves + ends any in-flight bio
    /// edit when leaving the Bio tab so the preview reflects the draft.
    /// Skips the Special tab while it's hidden (no bio/country/timezone).
    pub fn cycle_tab(&mut self, forward: bool) {
        let visible = self.visible_tabs();
        let idx = visible
            .iter()
            .position(|t| *t == self.selected_tab)
            .unwrap_or(0);
        let next_idx = if forward {
            (idx + 1) % visible.len()
        } else {
            (idx + visible.len() - 1) % visible.len()
        };
        let next = visible[next_idx];
        if self.selected_tab == Tab::Bio && next != Tab::Bio && self.editing_bio {
            self.stop_bio_edit();
            self.save();
        }
        if self.selected_tab == Tab::Settings && self.editing_username {
            // Leaving the Settings tab mid-username-edit → commit what's typed.
            self.submit_username();
            self.save();
        }
        if self.selected_tab == Tab::Settings && self.editing_system_field.is_some() {
            self.submit_system_field();
            self.save();
        }
        if self.selected_tab == Tab::Feeds && self.editing_feed_url {
            self.cancel_feed_url_edit();
        }
        if next == Tab::Themes {
            self.sync_theme_index_to_draft();
        }
        self.selected_tab = next;
    }

    /// Tabs to show in the tab strip in display order. The Special tab is
    /// hidden until the user has filled out at least one of bio, country,
    /// or timezone.
    pub fn visible_tabs(&self) -> Vec<Tab> {
        Tab::ALL
            .iter()
            .copied()
            .filter(|tab| *tab != Tab::Special || self.special_tab_unlocked())
            .collect()
    }

    pub fn special_tab_unlocked(&self) -> bool {
        let bio_filled = !self.draft.bio.trim().is_empty();
        let country_filled = self
            .draft
            .country
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        let timezone_filled = self
            .draft
            .timezone
            .as_deref()
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
        bio_filled || country_filled || timezone_filled
    }

    /// Flip the "show settings on connect" toggle (the sole control on the
    /// Special tab) and persist.
    pub fn toggle_show_settings_on_connect(&mut self) {
        self.draft.show_settings_on_connect ^= true;
        self.save();
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

    pub fn theme_selected_row(&self) -> usize {
        self.theme_selected_row
    }

    pub fn theme_scroll_offset(&self) -> usize {
        self.theme_scroll_offset
    }

    pub fn set_theme_visible_height(&self, height: usize) {
        self.theme_visible_height.set(height.max(1));
    }

    pub fn move_theme_cursor(&mut self, delta: isize) {
        let rows = self.theme_tree_rows();
        let last = rows.len().saturating_sub(1) as isize;
        self.theme_selected_row =
            (self.theme_selected_row as isize + delta).clamp(0, last) as usize;
        if let Some(ThemeTreeRow::Theme { option_index, .. }) =
            rows.get(self.theme_selected_row).copied()
        {
            self.apply_theme_index(option_index);
        }
        self.keep_theme_cursor_visible();
    }

    pub fn theme_cursor_left(&mut self) {
        let rows = self.theme_tree_rows();
        match rows.get(self.theme_selected_row).copied() {
            Some(ThemeTreeRow::Group {
                group,
                collapsed: false,
            }) => self.collapse_theme_group(group),
            Some(ThemeTreeRow::Theme { option_index, .. }) => {
                self.collapse_theme_group(theme::OPTIONS[option_index].group);
            }
            _ => {}
        }
    }

    pub fn theme_cursor_right(&mut self) {
        let rows = self.theme_tree_rows();
        match rows.get(self.theme_selected_row).copied() {
            Some(ThemeTreeRow::Group {
                group,
                collapsed: true,
            }) => self.expand_theme_group(group),
            Some(ThemeTreeRow::Group {
                group,
                collapsed: false,
            }) => {
                if let Some(row) = self.first_theme_row_for_group(group) {
                    self.theme_selected_row = row;
                    if let Some(ThemeTreeRow::Theme { option_index, .. }) =
                        self.theme_tree_rows().get(row).copied()
                    {
                        self.apply_theme_index(option_index);
                    }
                    self.keep_theme_cursor_visible();
                }
            }
            _ => {}
        }
    }

    pub fn toggle_theme_tree_row(&mut self) {
        let rows = self.theme_tree_rows();
        if let Some(row) = rows.get(self.theme_selected_row).copied() {
            match row {
                ThemeTreeRow::Group { group, collapsed } => {
                    if collapsed {
                        self.expand_theme_group(group);
                    } else {
                        self.collapse_theme_group(group);
                    }
                }
                ThemeTreeRow::Theme { option_index, .. } => self.select_theme_index(option_index),
            }
        }
    }

    pub fn select_theme_index(&mut self, index: usize) {
        let clamped = index.min(theme::OPTIONS.len().saturating_sub(1));
        self.expand_theme_group(theme::OPTIONS[clamped].group);
        self.theme_index = clamped;
        self.theme_selected_row = self
            .theme_row_for_option(clamped)
            .unwrap_or(self.theme_selected_row);
        self.apply_theme_index(clamped);
        self.keep_theme_cursor_visible();
    }

    fn apply_theme_index(&mut self, index: usize) {
        if let Some(option) = theme::OPTIONS.get(index) {
            self.theme_index = index;
            let current = self
                .draft
                .theme_id
                .as_deref()
                .map(theme::normalize_id)
                .unwrap_or(theme::DEFAULT_ID);
            let changed = current != option.id;
            self.draft.theme_id = Some(option.id.to_string());
            self.keep_theme_cursor_visible();
            if changed {
                self.save();
            }
        }
    }

    pub fn theme_tree_rows(&self) -> Vec<ThemeTreeRow> {
        let mut rows = Vec::new();
        for group in theme::ThemeGroup::ALL {
            let collapsed = self.theme_group_collapsed(group);
            rows.push(ThemeTreeRow::Group { group, collapsed });
            if collapsed {
                continue;
            }

            let option_indices: Vec<usize> = theme::OPTIONS
                .iter()
                .enumerate()
                .filter_map(|(idx, option)| (option.group == group).then_some(idx))
                .collect();
            let last_option_idx = option_indices.len().saturating_sub(1);
            for (idx, option_index) in option_indices.into_iter().enumerate() {
                rows.push(ThemeTreeRow::Theme {
                    option_index,
                    last_in_group: idx == last_option_idx,
                });
            }
        }
        rows
    }

    fn sync_theme_index_to_draft(&mut self) {
        let current = self
            .draft
            .theme_id
            .as_deref()
            .unwrap_or_else(|| theme::normalize_id(""));
        let normalized = theme::normalize_id(current);
        self.theme_index = theme::OPTIONS
            .iter()
            .position(|option| option.id == normalized)
            .unwrap_or(0);
        self.expand_theme_group(theme::OPTIONS[self.theme_index].group);
        self.theme_selected_row = self.theme_row_for_option(self.theme_index).unwrap_or(0);
        self.keep_theme_cursor_visible();
    }

    fn keep_theme_cursor_visible(&mut self) {
        let visible = self.theme_visible_height.get().max(1);
        let max_scroll = self.theme_tree_rows().len().saturating_sub(visible);
        if self.theme_selected_row < self.theme_scroll_offset {
            self.theme_scroll_offset = self.theme_selected_row;
        } else if self.theme_selected_row >= self.theme_scroll_offset + visible {
            self.theme_scroll_offset = self.theme_selected_row.saturating_sub(visible - 1);
        }
        self.theme_scroll_offset = self.theme_scroll_offset.min(max_scroll);
    }

    fn theme_group_collapsed(&self, group: theme::ThemeGroup) -> bool {
        self.theme_collapsed_groups & group.bit() != 0
    }

    fn expand_theme_group(&mut self, group: theme::ThemeGroup) {
        self.theme_collapsed_groups &= !group.bit();
        self.keep_theme_cursor_visible();
    }

    fn collapse_theme_group(&mut self, group: theme::ThemeGroup) {
        self.theme_collapsed_groups |= group.bit();
        self.theme_selected_row = self.theme_group_row(group).unwrap_or_else(|| {
            self.theme_selected_row
                .min(self.theme_tree_rows().len().saturating_sub(1))
        });
        self.keep_theme_cursor_visible();
    }

    fn theme_group_row(&self, group: theme::ThemeGroup) -> Option<usize> {
        self.theme_tree_rows()
            .iter()
            .position(|row| matches!(row, ThemeTreeRow::Group { group: row_group, .. } if *row_group == group))
    }

    fn theme_row_for_option(&self, option_index: usize) -> Option<usize> {
        self.theme_tree_rows().iter().position(
            |row| matches!(row, ThemeTreeRow::Theme { option_index: row_index, .. } if *row_index == option_index),
        )
    }

    fn first_theme_row_for_group(&self, group: theme::ThemeGroup) -> Option<usize> {
        self.theme_tree_rows().iter().position(|row| {
            matches!(
                row,
                ThemeTreeRow::Theme { option_index, .. }
                    if theme::OPTIONS[*option_index].group == group
            )
        })
    }

    pub fn editing_username(&self) -> bool {
        self.editing_username
    }

    pub fn editing_system_field(&self) -> Option<SystemField> {
        self.editing_system_field
    }

    pub fn editing_system_row(&self, row: Row) -> bool {
        self.editing_system_field == SystemField::from_row(row)
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

    pub fn system_input(&self) -> &TextArea<'static> {
        &self.system_input
    }

    fn system_text(&self) -> String {
        self.system_input.lines().join("")
    }

    fn system_char_count(&self) -> usize {
        self.system_input
            .lines()
            .iter()
            .map(|l| l.chars().count())
            .sum()
    }

    pub fn bio_input(&self) -> &TextArea<'static> {
        &self.bio_input
    }

    pub fn feeds(&self) -> &[RssFeed] {
        &self.feeds
    }

    pub fn feed_index(&self) -> usize {
        self.feed_index
    }

    pub fn editing_feed_url(&self) -> bool {
        self.editing_feed_url
    }

    pub fn feed_url_input(&self) -> &TextArea<'static> {
        &self.feed_url_input
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
        self.editing_system_field = None;
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

    pub fn start_system_field_edit(&mut self, field: SystemField) {
        self.editing_username = false;
        self.editing_system_field = Some(field);
        self.system_input = new_short_textarea(true);
        if let Some(value) = field.value(&self.draft) {
            self.system_input.insert_str(&value);
        }
    }

    pub fn cancel_system_field_edit(&mut self) {
        self.editing_system_field = None;
        self.system_input = new_short_textarea(false);
    }

    pub fn submit_system_field(&mut self) {
        let Some(field) = self.editing_system_field.take() else {
            return;
        };
        let text = self.system_text();
        self.system_input = new_short_textarea(false);
        field.set_value(&mut self.draft, text);
        self.save();
    }

    pub fn system_push(&mut self, ch: char) {
        if self.system_char_count() < SYSTEM_FIELD_MAX_LEN {
            self.system_input.insert_char(ch);
        }
    }

    pub fn system_backspace(&mut self) {
        self.system_input.delete_char();
    }

    pub fn system_delete_right(&mut self) {
        self.system_input.delete_next_char();
    }

    pub fn system_delete_word_left(&mut self) {
        self.system_input.delete_word();
    }

    pub fn system_delete_word_right(&mut self) {
        self.system_input.delete_next_word();
    }

    pub fn system_cursor_left(&mut self) {
        self.system_input.move_cursor(CursorMove::Back);
    }

    pub fn system_cursor_right(&mut self) {
        self.system_input.move_cursor(CursorMove::Forward);
    }

    pub fn system_cursor_word_left(&mut self) {
        self.system_input.move_cursor(CursorMove::WordBack);
    }

    pub fn system_cursor_word_right(&mut self) {
        self.system_input.move_cursor(CursorMove::WordForward);
    }

    pub fn system_cursor_home(&mut self) {
        self.system_input.move_cursor(CursorMove::Head);
    }

    pub fn system_cursor_end(&mut self) {
        self.system_input.move_cursor(CursorMove::End);
    }

    pub fn system_paste(&mut self) {
        let yank = self.system_input.yank_text();
        insert_system_text_limited(&mut self.system_input, &yank);
    }

    pub fn system_undo(&mut self) {
        self.system_input.undo();
    }

    pub fn clear_system_field(&mut self) {
        self.system_input = new_short_textarea(self.editing_system_field.is_some());
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

    pub fn move_feed_cursor(&mut self, delta: isize) {
        let len = self.feed_slot_count();
        if len == 0 {
            self.feed_index = 0;
            return;
        }
        self.feed_index = (self.feed_index as isize + delta).clamp(0, len as isize - 1) as usize;
    }

    pub fn feed_slot_count(&self) -> usize {
        self.feeds.len() + 1
    }

    pub fn feed_index_is_add_row(&self) -> bool {
        self.feed_index == self.feeds.len()
    }

    pub fn start_feed_url_edit(&mut self) {
        self.editing_feed_url = true;
        self.feed_url_input = new_short_textarea(true);
    }

    pub fn cancel_feed_url_edit(&mut self) {
        self.editing_feed_url = false;
        self.feed_url_input = new_short_textarea(false);
    }

    pub fn submit_feed_url(&mut self) {
        let url = self.feed_url_input.lines().join("").trim().to_string();
        self.cancel_feed_url_edit();
        if url.is_empty() {
            return;
        }
        self.feed_service.add_feed_task(self.user_id, url);
    }

    pub fn remove_selected_feed(&mut self) {
        if self.feed_index_is_add_row() {
            return;
        }
        let Some(feed) = self.feeds.get(self.feed_index) else {
            return;
        };
        self.feed_service.delete_feed_task(self.user_id, feed.id);
    }

    pub fn refresh_feeds(&self) {
        self.feed_service.poll_once_task();
        self.feed_service.list_task(self.user_id);
    }

    pub fn feed_push(&mut self, ch: char) {
        if self.feed_url_char_count() < FEED_URL_MAX_LEN {
            self.feed_url_input.insert_char(ch);
        }
    }

    pub fn feed_backspace(&mut self) {
        self.feed_url_input.delete_char();
    }

    pub fn feed_delete_right(&mut self) {
        self.feed_url_input.delete_next_char();
    }

    pub fn feed_cursor_left(&mut self) {
        self.feed_url_input.move_cursor(CursorMove::Back);
    }

    pub fn feed_cursor_right(&mut self) {
        self.feed_url_input.move_cursor(CursorMove::Forward);
    }

    pub fn feed_cursor_word_left(&mut self) {
        self.feed_url_input.move_cursor(CursorMove::WordBack);
    }

    pub fn feed_cursor_word_right(&mut self) {
        self.feed_url_input.move_cursor(CursorMove::WordForward);
    }

    pub fn feed_cursor_home(&mut self) {
        self.feed_url_input.move_cursor(CursorMove::Head);
    }

    pub fn feed_cursor_end(&mut self) {
        self.feed_url_input.move_cursor(CursorMove::End);
    }

    pub fn feed_clear(&mut self) {
        self.feed_url_input = new_short_textarea(self.editing_feed_url);
    }

    pub fn feed_paste(&mut self) {
        let yank = self.feed_url_input.yank_text();
        for ch in yank.chars() {
            if !ch.is_control() && ch != '\n' && ch != '\r' {
                self.feed_push(ch);
            }
        }
    }

    pub fn feed_undo(&mut self) {
        self.feed_url_input.undo();
    }

    fn feed_url_char_count(&self) -> usize {
        self.feed_url_input
            .lines()
            .iter()
            .map(|line| line.chars().count())
            .sum()
    }

    fn drain_feed_snapshot(&mut self) {
        if let Ok(true) = self.feed_snapshot_rx.has_changed() {
            let snapshot = self.feed_snapshot_rx.borrow_and_update().clone();
            if snapshot.user_id == Some(self.user_id) {
                self.feeds = snapshot.feeds;
                self.feed_index = self
                    .feed_index
                    .min(self.feed_slot_count().saturating_sub(1));
            }
        }
    }

    fn drain_feed_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.feed_event_rx.try_recv() {
                Ok(FeedEvent::FeedAdded { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("Feed connected."));
                }
                Ok(FeedEvent::FeedDeleted { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("Feed removed."));
                }
                Ok(FeedEvent::FeedFailed { user_id, error }) if user_id == self.user_id => {
                    banner = Some(Banner::error(&format!("Feed failed: {error}")));
                }
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive settings feed event");
                    break;
                }
            }
        }
        banner
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
                self.sync_theme_index_to_draft();
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
            Row::Ide | Row::Terminal | Row::Os | Row::Langs => false,
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
                ide: self.draft.ide.clone(),
                terminal: self.draft.terminal.clone(),
                os: self.draft.os.clone(),
                langs: self.draft.langs.clone(),
                notify_kinds: self.draft.notify_kinds.clone(),
                notify_bell: self.draft.notify_bell,
                notify_cooldown_mins: self.draft.notify_cooldown_mins,
                notify_format: self.draft.notify_format.clone(),
                theme_id: Some(
                    self.draft
                        .theme_id
                        .clone()
                        .unwrap_or_else(|| theme::DEFAULT_ID.to_string()),
                ),
                enable_background_color: self.draft.enable_background_color,
                show_dashboard_header: self.draft.show_dashboard_header,
                show_right_sidebar: self.draft.show_right_sidebar,
                show_games_sidebar: self.draft.show_games_sidebar,
                show_settings_on_connect: self.draft.show_settings_on_connect,
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

fn prune_favorites_against_loaded_rooms(favorite_room_ids: &mut Vec<Uuid>, rooms: &[RoomOption]) {
    if rooms.is_empty() {
        return;
    }

    // Drop favorites the user is no longer a member of so the modal never
    // shows ghost entries. Preserve order of the survivors. An empty room
    // catalog means chat membership has not loaded yet, not that every room
    // was left.
    let member_ids: std::collections::HashSet<Uuid> = rooms.iter().map(|room| room.id).collect();
    favorite_room_ids.retain(|id| member_ids.contains(id));
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

fn system_char_count_for_input(input: &TextArea<'static>) -> usize {
    input.lines().iter().map(|l| l.chars().count()).sum()
}

fn normalize_optional_text(text: &str) -> Option<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
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

fn insert_system_text_limited(input: &mut TextArea<'static>, text: &str) {
    for ch in text.chars() {
        if system_char_count_for_input(input) >= SYSTEM_FIELD_MAX_LEN {
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
    new_short_textarea(editing)
}

fn new_short_textarea(editing: bool) -> TextArea<'static> {
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
    fn system_yank_respects_max_length() {
        let mut input = new_short_textarea(true);
        input.insert_str("a".repeat(SYSTEM_FIELD_MAX_LEN - 1));
        input.set_yank_text("xyz");
        let yank = input.yank_text();

        insert_system_text_limited(&mut input, &yank);

        assert_eq!(system_char_count_for_input(&input), SYSTEM_FIELD_MAX_LEN);
    }

    #[test]
    fn normalize_optional_text_trims_and_collapses_blank() {
        assert_eq!(
            normalize_optional_text("  VS   Code  ").as_deref(),
            Some("VS Code")
        );
        assert_eq!(normalize_optional_text("   "), None);
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

    #[test]
    fn empty_room_catalog_preserves_favorites() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let mut favorites = vec![first, second];

        prune_favorites_against_loaded_rooms(&mut favorites, &[]);

        assert_eq!(favorites, vec![first, second]);
    }

    #[test]
    fn loaded_room_catalog_prunes_unjoined_favorites() {
        let first = Uuid::from_u128(1);
        let second = Uuid::from_u128(2);
        let third = Uuid::from_u128(3);
        let mut favorites = vec![first, second, third];
        let rooms = vec![
            RoomOption {
                id: third,
                label: "#third".to_string(),
            },
            RoomOption {
                id: first,
                label: "#first".to_string(),
            },
        ];

        prune_favorites_against_loaded_rooms(&mut favorites, &rooms);

        assert_eq!(favorites, vec![first, third]);
    }
}
