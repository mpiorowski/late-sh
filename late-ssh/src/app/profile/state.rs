use late_core::models::profile::{Profile, ProfileParams};
use late_core::models::user::sanitize_username_input;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{ProfileEvent, ProfileService, ProfileSnapshot};
use crate::app::common::{primitives::Banner, theme};

const USERNAME_MAX_LEN: usize = 12;

pub struct ProfileState {
    profile_service: ProfileService,
    user_id: Uuid,
    pub(crate) profile: Profile,
    snapshot_rx: watch::Receiver<ProfileSnapshot>,
    event_rx: broadcast::Receiver<ProfileEvent>,
    pub(crate) editing_username: bool,
    pub(crate) username_composer: String,

    /// Which settings row is selected. Rows 0..NOTIFY_KINDS.len() are the
    /// kind checkboxes; the last row is the cooldown selector.
    pub(crate) settings_row: usize,

    // Display config (informational)
    pub(crate) ai_model: String,

    // Scroll
    pub(crate) scroll_offset: u16,
    pub(crate) viewport_height: u16,
}

impl Drop for ProfileState {
    fn drop(&mut self) {
        self.profile_service
            .prune_user_snapshot_channel(self.user_id);
    }
}

impl ProfileState {
    pub fn new(
        profile_service: ProfileService,
        user_id: Uuid,
        ai_model: String,
        initial_theme_id: String,
    ) -> Self {
        let snapshot_rx = profile_service.subscribe_snapshot(user_id);
        let event_rx = profile_service.subscribe_events();
        profile_service.find_profile(user_id);
        let profile = Profile {
            theme_id: Some(theme::normalize_id(&initial_theme_id).to_string()),
            ..Profile::default()
        };
        Self {
            profile_service,
            user_id,
            profile,
            snapshot_rx,
            event_rx,
            editing_username: false,
            username_composer: String::new(),
            settings_row: 0,
            ai_model,
            scroll_offset: 0,
            viewport_height: 0,
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn editing_username(&self) -> bool {
        self.editing_username
    }

    pub fn cursor_visible(&self) -> bool {
        self.editing_username
    }

    pub fn username_composer(&self) -> &str {
        &self.username_composer
    }

    pub fn ai_model(&self) -> &str {
        &self.ai_model
    }

    pub fn theme_id(&self) -> &str {
        self.profile
            .theme_id
            .as_deref()
            .unwrap_or_else(|| theme::normalize_id(""))
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn set_viewport_height(&mut self, h: u16) {
        self.viewport_height = h;
    }

    pub fn scroll_by(&mut self, delta: i16) {
        let next = self.scroll_offset as i32 + delta as i32;
        self.scroll_offset = next.clamp(0, u16::MAX as i32) as u16;
    }

    pub fn ensure_field_visible(&mut self, field_line: u16) {
        let h = self.viewport_height;
        if h == 0 {
            return;
        }
        if field_line < self.scroll_offset {
            self.scroll_offset = field_line;
        } else if field_line >= self.scroll_offset + h {
            self.scroll_offset = field_line - h + 1;
        }
    }

    // Username editing
    pub fn start_username_edit(&mut self) {
        self.editing_username = true;
        self.username_composer = self.profile.username.clone();
    }

    pub fn cancel_username_edit(&mut self) {
        self.editing_username = false;
        self.username_composer.clear();
    }

    pub fn submit_username(&mut self) {
        self.editing_username = false;
        let normalized =
            normalize_username_submission(&self.username_composer, &self.profile.username);
        self.username_composer.clear();
        if let Some(next) = normalized {
            self.profile.username = next;
            self.save_profile();
        }
    }

    pub fn composer_push(&mut self, ch: char) {
        if self.username_composer.len() < USERNAME_MAX_LEN {
            self.username_composer.push(ch);
        }
    }

    pub fn composer_clear(&mut self) {
        self.username_composer.clear();
    }
    pub fn composer_backspace(&mut self) {
        self.username_composer.pop();
    }

    /// Notification kinds the user can toggle on the profile screen, in display order.
    pub(crate) const NOTIFY_KINDS: &'static [&'static str] = &["dms", "mentions", "game_events"];

    fn theme_row_index() -> usize {
        0
    }

    const BACKGROUND_COLOR_ROW: usize = 1;
    const NOTIFY_START_ROW: usize = 2;

    fn notify_row_index(kind_idx: usize) -> usize {
        Self::NOTIFY_START_ROW + kind_idx
    }

    fn bell_row_index() -> usize {
        Self::notify_row_index(Self::NOTIFY_KINDS.len())
    }

    fn cooldown_row_index() -> usize {
        Self::bell_row_index() + 1
    }

    pub fn move_settings_row(&mut self, delta: isize) {
        let last = Self::cooldown_row_index() as isize;
        self.settings_row = clamp_settings_row(self.settings_row as isize + delta, last);
    }

    /// Cycle the currently selected setting and save immediately.
    pub fn cycle_setting(&mut self, forward: bool) {
        if self.settings_row == Self::theme_row_index() {
            let current = self
                .profile
                .theme_id
                .as_deref()
                .unwrap_or_else(|| theme::normalize_id(""));
            let next = theme::cycle_id(current, forward).to_string();
            self.profile.theme_id = Some(next.clone());
            self.profile_service.set_theme_id(self.user_id, next);
        } else if self.settings_row == Self::BACKGROUND_COLOR_ROW {
            self.profile.enable_background_color = !self.profile.enable_background_color;
            self.save_profile();
        } else if self.settings_row == Self::cooldown_row_index() {
            self.profile.notify_cooldown_mins =
                cycle_cooldown_value(self.profile.notify_cooldown_mins, forward);
            self.save_profile();
        } else if self.settings_row == Self::bell_row_index() {
            self.profile.notify_bell ^= true;
            self.save_profile();
        } else if let Some(kind) = self
            .settings_row
            .checked_sub(Self::NOTIFY_START_ROW)
            .and_then(|idx| Self::NOTIFY_KINDS.get(idx))
        {
            toggle_notify_kind(&mut self.profile.notify_kinds, kind);
            self.save_profile();
        }
    }

    fn save_profile(&self) {
        self.profile_service.edit_profile(
            self.user_id,
            ProfileParams {
                username: self.profile.username.clone(),
                bio: self.profile.bio.clone(),
                country: self.profile.country.clone(),
                timezone: self.profile.timezone.clone(),
                notify_kinds: self.profile.notify_kinds.clone(),
                notify_bell: self.profile.notify_bell,
                notify_cooldown_mins: self.profile.notify_cooldown_mins,
                theme_id: Some(self.theme_id().to_string()),
                enable_background_color: self.profile.enable_background_color,
            },
        );
    }

    // Tick
    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        self.drain_events()
    }

    fn drain_snapshot(&mut self) {
        match self.snapshot_rx.has_changed() {
            Ok(true) => {
                let snapshot = self.snapshot_rx.borrow_and_update();
                if snapshot.user_id != Some(self.user_id) {
                    return;
                }
                let profile = snapshot.profile.clone();
                drop(snapshot);
                if let Some(mut profile) = profile {
                    let normalized = theme::normalize_id(profile.theme_id.as_deref().unwrap_or(""));
                    profile.theme_id = Some(normalized.to_string());
                    self.profile = profile;
                }
            }
            Ok(false) => (),
            Err(e) => {
                tracing::error!(%e, "failed to receive profile snapshot");
            }
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    ProfileEvent::Saved { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Profile saved!"));
                    }
                    ProfileEvent::Error { user_id, message } if self.user_id == user_id => {
                        banner = Some(Banner::error(&message));
                    }
                    _ => (),
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive profile event");
                    break;
                }
            }
        }
        banner
    }
}

/// Cooldown values cycled through on the profile screen, in display order.
/// `0` is rendered as "Off".
const COOLDOWN_OPTIONS: &[i32] = &[0, 1, 2, 5, 10, 15, 30, 60, 120, 240];

/// Returns the new username to persist, or `None` if the submission is empty
/// after trimming or unchanged from the current value.
fn normalize_username_submission(composer: &str, current: &str) -> Option<String> {
    let trimmed = composer.trim();
    if trimmed.is_empty() {
        None
    } else {
        let normalized = sanitize_username_input(trimmed);
        if normalized == current {
            None
        } else {
            Some(normalized)
        }
    }
}

/// Toggle `kind` in `kinds`: remove it if present, otherwise append it.
fn toggle_notify_kind(kinds: &mut Vec<String>, kind: &str) {
    if let Some(idx) = kinds.iter().position(|k| k == kind) {
        kinds.remove(idx);
    } else {
        kinds.push(kind.to_string());
    }
}

/// Return the next cooldown value in the configured cycle, wrapping at both
/// ends. Unknown values snap back to the start of the cycle.
fn cycle_cooldown_value(current: i32, forward: bool) -> i32 {
    let idx = COOLDOWN_OPTIONS
        .iter()
        .position(|&o| o == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % COOLDOWN_OPTIONS.len()
    } else {
        (idx + COOLDOWN_OPTIONS.len() - 1) % COOLDOWN_OPTIONS.len()
    };
    COOLDOWN_OPTIONS[next]
}

fn clamp_settings_row(row: isize, last: isize) -> usize {
    row.clamp(0, last) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_max_len_constant_is_12() {
        assert_eq!(USERNAME_MAX_LEN, 12);
    }

    #[test]
    fn normalize_username_submission_trims_whitespace() {
        assert_eq!(
            normalize_username_submission("  alice  ", "old"),
            Some("alice".to_string())
        );
    }

    #[test]
    fn normalize_username_submission_replaces_spaces_and_invalid_chars() {
        assert_eq!(
            normalize_username_submission("  late night!!!  ", "old"),
            Some("late_night".to_string())
        );
    }

    #[test]
    fn normalize_username_submission_skips_when_empty_after_trim() {
        assert_eq!(normalize_username_submission("", "old"), None);
        assert_eq!(normalize_username_submission("   ", "old"), None);
    }

    #[test]
    fn normalize_username_submission_skips_when_unchanged() {
        assert_eq!(normalize_username_submission("alice", "alice"), None);
        // Trim then compare — whitespace-padded copy of current still skips.
        assert_eq!(normalize_username_submission("  alice ", "alice"), None);
        assert_eq!(normalize_username_submission("alice!!!", "alice"), None);
    }

    #[test]
    fn toggle_notify_kind_adds_when_absent() {
        let mut kinds: Vec<String> = Vec::new();
        toggle_notify_kind(&mut kinds, "dms");
        assert_eq!(kinds, vec!["dms".to_string()]);
    }

    #[test]
    fn toggle_notify_kind_removes_when_present() {
        let mut kinds = vec!["dms".to_string(), "mentions".to_string()];
        toggle_notify_kind(&mut kinds, "dms");
        assert_eq!(kinds, vec!["mentions".to_string()]);
    }

    #[test]
    fn toggle_notify_kind_preserves_order_of_other_kinds() {
        let mut kinds = vec![
            "dms".to_string(),
            "mentions".to_string(),
            "game_events".to_string(),
        ];
        toggle_notify_kind(&mut kinds, "mentions");
        assert_eq!(kinds, vec!["dms".to_string(), "game_events".to_string()]);
    }

    #[test]
    fn cycle_cooldown_value_steps_forward() {
        assert_eq!(cycle_cooldown_value(0, true), 1);
        assert_eq!(cycle_cooldown_value(5, true), 10);
    }

    #[test]
    fn cycle_cooldown_value_steps_backward() {
        assert_eq!(cycle_cooldown_value(1, false), 0);
        assert_eq!(cycle_cooldown_value(10, false), 5);
    }

    #[test]
    fn cycle_cooldown_value_wraps_at_both_ends() {
        assert_eq!(cycle_cooldown_value(0, false), 240);
        assert_eq!(cycle_cooldown_value(240, true), 0);
    }

    #[test]
    fn cycle_cooldown_value_snaps_unknown_value_to_start() {
        // 7 is not in the option list → treat as index 0 and advance.
        assert_eq!(cycle_cooldown_value(7, true), 1);
    }

    #[test]
    fn clamp_settings_row_clamps_below_zero() {
        assert_eq!(clamp_settings_row(-1, 3), 0);
    }

    #[test]
    fn clamp_settings_row_clamps_above_last() {
        assert_eq!(clamp_settings_row(9, 4), 4);
    }

    #[test]
    fn clamp_settings_row_passes_through_in_range() {
        assert_eq!(clamp_settings_row(2, 4), 2);
    }

    #[test]
    fn notify_kinds_constant_matches_ui_expectations() {
        // If you add a kind here, also update the UI tuple in profile/ui.rs
        // and the render-side matcher in render.rs.
        assert_eq!(
            ProfileState::NOTIFY_KINDS,
            &["dms", "mentions", "game_events"]
        );
    }

    #[test]
    fn theme_row_index_is_zero() {
        assert_eq!(ProfileState::theme_row_index(), 0);
    }

    #[test]
    fn first_notify_row_follows_background_color_row() {
        assert_eq!(ProfileState::notify_row_index(0), 2);
    }

    #[test]
    fn cooldown_row_is_last_selectable_row() {
        assert_eq!(ProfileState::cooldown_row_index(), 6);
    }
}
