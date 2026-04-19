use std::cell::Cell;

use late_core::models::profile::{Profile, ProfileParams};
use late_core::models::user::sanitize_username_input;
use uuid::Uuid;

use crate::app::common::{composer::ComposerState, theme};
use crate::app::profile::svc::ProfileService;

use super::data::{CountryOption, filter_countries, filter_timezones};
use super::ui::bio_text_width;

const USERNAME_MAX_LEN: usize = 12;
const SHORT_FIELD_MAX_LEN: usize = 20;
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
    Distro,
    Terminal,
    Editor,
    Save,
}

impl Row {
    pub const ALL: [Row; 15] = [
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
        Row::Distro,
        Row::Terminal,
        Row::Editor,
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

pub struct WelcomeModalState {
    profile_service: ProfileService,
    user_id: Uuid,
    draft: Profile,
    row_index: usize,
    editing_username: bool,
    username_input: String,
    editing_bio: bool,
    editing_distro: bool,
    editing_terminal: bool,
    editing_editor: bool,
    bio_input: ComposerState,
    distro_input: String,
    terminal_input: String,
    editor_input: String,
    picker: PickerState,
}

impl WelcomeModalState {
    pub fn new(profile_service: ProfileService, user_id: Uuid) -> Self {
        Self {
            profile_service,
            user_id,
            draft: Profile::default(),
            row_index: 0,
            editing_username: false,
            username_input: String::new(),
            editing_bio: false,
            editing_distro: false,
            editing_terminal: false,
            editing_editor: false,
            bio_input: ComposerState::new(48),
            distro_input: String::new(),
            terminal_input: String::new(),
            editor_input: String::new(),
            picker: PickerState::default(),
        }
    }

    pub fn open_from_profile(&mut self, profile: &Profile, modal_width: u16) {
        self.draft = profile.clone();
        self.row_index = 0;
        self.editing_username = false;
        self.username_input.clear();
        self.editing_bio = false;
        self.editing_distro = false;
        self.editing_terminal = false;
        self.editing_editor = false;
        self.bio_input = ComposerState::new(bio_text_width(modal_width));
        self.bio_input.set_text(self.draft.bio.clone());
        self.distro_input.clear();
        self.terminal_input.clear();
        self.editor_input.clear();
        self.picker = PickerState::default();
    }

    pub fn set_modal_width(&mut self, modal_width: u16) {
        self.bio_input.set_text_width(bio_text_width(modal_width));
        self.bio_input.sync_layout();
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

    pub fn editing_distro(&self) -> bool {
        self.editing_distro
    }

    pub fn editing_terminal(&self) -> bool {
        self.editing_terminal
    }

    pub fn editing_editor(&self) -> bool {
        self.editing_editor
    }

    pub fn username_input(&self) -> &str {
        &self.username_input
    }

    pub fn bio_input(&self) -> &ComposerState {
        &self.bio_input
    }

    pub fn bio_input_mut(&mut self) -> &mut ComposerState {
        &mut self.bio_input
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
        self.username_input = self.draft.username.clone();
    }

    pub fn cancel_username_edit(&mut self) {
        self.editing_username = false;
        self.username_input.clear();
    }

    pub fn submit_username(&mut self) {
        self.editing_username = false;
        let normalized = sanitize_username_input(self.username_input.trim());
        self.username_input.clear();
        self.draft.username = normalized;
    }

    pub fn username_push(&mut self, ch: char) {
        if self.username_input.chars().count() < USERNAME_MAX_LEN {
            self.username_input.push(ch);
        }
    }

    pub fn username_backspace(&mut self) {
        self.username_input.pop();
    }

    pub fn clear_username(&mut self) {
        self.username_input.clear();
    }

    pub fn start_bio_edit(&mut self) {
        self.editing_bio = true;
        self.bio_input.sync_layout();
    }

    pub fn stop_bio_edit(&mut self) {
        self.editing_bio = false;
        self.bio_input.sync_layout();
        self.draft.bio = self.bio_input.text().trim_end().to_string();
    }

    pub fn bio_push(&mut self, ch: char) {
        if self.bio_input.text().chars().count() < BIO_MAX_LEN {
            self.bio_input.push(ch);
        }
    }

    pub fn start_distro_edit(&mut self) {
        self.editing_distro = true;
        self.distro_input = self.draft.distro.unwrap_or_default();
    }

    pub fn cancel_distro_edit(&mut self) {
        self.editing_distro = false;
        self.distro_input.clear();
    }

    pub fn submit_distro(&mut self) {
        self.editing_distro = false;
        self.distro_input.clear();
        self.draft.distro = Some(self.distro_input.trim().into());
    }

    pub fn distro_push(&mut self, ch: char) {
        if self.distro_input.chars().count() < SHORT_FIELD_MAX_LEN {
            self.distro_input.push(ch);
        }
    }

    pub fn distro_backspace(&mut self) {
        self.distro_input.pop();
    }

    pub fn clear_distro(&mut self) {
        self.distro_input.clear();
    }

    pub fn start_terminal_edit(&mut self) {
        self.editing_terminal = true;
        self.terminal_input = self.draft.terminal.unwrap_or_default();
    }

    pub fn cancel_terminal_edit(&mut self) {
        self.editing_terminal = false;
        self.terminal_input.clear();
    }

    pub fn submit_terminal(&mut self) {
        self.editing_terminal = false;
        self.terminal_input.clear();
        self.draft.terminal = Some(self.terminal_input.trim().into());
    }

    pub fn terminal_push(&mut self, ch: char) {
        if self.terminal_input.chars().count() < SHORT_FIELD_MAX_LEN {
            self.terminal_input.push(ch);
        }
    }

    pub fn terminal_backspace(&mut self) {
        self.terminal_input.pop();
    }

    pub fn clear_terminal(&mut self) {
        self.terminal_input.clear();
    }

    pub fn start_editor_edit(&mut self) {
        self.editing_editor = true;
        self.editor_input = self.draft.editor.unwrap_or_default();
    }

    pub fn cancel_editor_edit(&mut self) {
        self.editing_editor = false;
        self.editor_input.clear();
    }

    pub fn submit_editor(&mut self) {
        self.editing_editor = false;
        self.editor_input.clear();
        self.draft.editor = Some(self.editor_input.trim().into());
    }

    pub fn editor_push(&mut self, ch: char) {
        if self.editor_input.chars().count() < SHORT_FIELD_MAX_LEN {
            self.editor_input.push(ch);
        }
    }

    pub fn editor_backspace(&mut self) {
        self.editor_input.pop();
    }

    pub fn clear_editor(&mut self) {
        self.editor_input.clear();
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
                distro: self.draft.distro.clone(),
                terminal: self.draft.terminal.clone(),
                editor: self.draft.editor.clone(),
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
