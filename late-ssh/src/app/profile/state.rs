use late_core::models::profile::{Profile, ProfileParams};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{ProfileEvent, ProfileService, ProfileSnapshot};
use crate::app::common::primitives::Banner;

const USERNAME_MAX_LEN: usize = 12;

pub struct ProfileState {
    profile_service: ProfileService,
    user_id: Uuid,
    pub(crate) profile: Profile,
    snapshot_rx: watch::Receiver<ProfileSnapshot>,
    event_rx: broadcast::Receiver<ProfileEvent>,
    pub(crate) editing_username: bool,
    pub(crate) username_composer: String,
    bg_task: tokio::task::AbortHandle,

    /// Which settings row is selected (0 = DM Notifications, 1 = Cooldown).
    pub(crate) settings_row: usize,

    // Display config (informational)
    pub(crate) ai_model: String,

    // Scroll
    pub(crate) scroll_offset: u16,
    pub(crate) viewport_height: u16,
}

impl Drop for ProfileState {
    fn drop(&mut self) {
        self.bg_task.abort();
        self.profile_service
            .prune_user_snapshot_channel(self.user_id);
    }
}

impl ProfileState {
    pub fn new(profile_service: ProfileService, user_id: Uuid, ai_model: String) -> Self {
        let snapshot_rx = profile_service.subscribe_snapshot(user_id);
        let event_rx = profile_service.subscribe_events();
        let bg_task = profile_service.start_user_refresh_task(user_id);
        Self {
            profile_service,
            user_id,
            profile: Profile::default(),
            snapshot_rx,
            event_rx,
            editing_username: false,
            username_composer: String::new(),
            bg_task,
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

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn set_viewport_height(&mut self, h: u16) {
        self.viewport_height = h;
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
        self.profile.username = self.username_composer.clone();
        self.save_profile();
        self.username_composer.clear();
    }

    pub fn composer_push(&mut self, ch: char) {
        if self.username_composer.len() < USERNAME_MAX_LEN {
            self.username_composer.push(ch);
        }
    }

    pub fn composer_backspace(&mut self) {
        self.username_composer.pop();
    }

    const SETTINGS_ROW_COUNT: usize = 2;

    pub fn move_settings_row(&mut self, delta: isize) {
        let row = self.settings_row as isize + delta;
        self.settings_row = row.clamp(0, (Self::SETTINGS_ROW_COUNT - 1) as isize) as usize;
    }

    /// Cycle the currently selected setting and save immediately.
    pub fn cycle_setting(&mut self, forward: bool) {
        match self.settings_row {
            0 => self.cycle_dm_notify(forward),
            1 => self.cycle_cooldown(forward),
            _ => {}
        }
    }

    fn cycle_dm_notify(&mut self, forward: bool) {
        const OPTIONS: &[&str] = &["unfocused", "always", "off"];
        let current_idx = OPTIONS
            .iter()
            .position(|&o| o == self.profile.dm_notify)
            .unwrap_or(0);
        let next_idx = if forward {
            (current_idx + 1) % OPTIONS.len()
        } else {
            (current_idx + OPTIONS.len() - 1) % OPTIONS.len()
        };
        self.profile.dm_notify = OPTIONS[next_idx].to_string();
        self.save_profile();
    }

    fn cycle_cooldown(&mut self, forward: bool) {
        const OPTIONS: &[i32] = &[1, 2, 5, 10, 15, 30, 60, 120, 240];
        let current_idx = OPTIONS
            .iter()
            .position(|&o| o == self.profile.dm_notify_cooldown_mins)
            .unwrap_or(2); // default to 5
        let next_idx = if forward {
            (current_idx + 1) % OPTIONS.len()
        } else {
            (current_idx + OPTIONS.len() - 1) % OPTIONS.len()
        };
        self.profile.dm_notify_cooldown_mins = OPTIONS[next_idx];
        self.save_profile();
    }

    fn save_profile(&self) {
        self.profile_service.edit_profile(
            self.user_id,
            self.profile.id,
            ProfileParams {
                user_id: self.user_id,
                username: self.profile.username.clone(),
                enable_ghost: self.profile.enable_ghost,
                dm_notify: self.profile.dm_notify.clone(),
                dm_notify_cooldown_mins: self.profile.dm_notify_cooldown_mins,
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
                if let Some(profile) = profile {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_max_len_constant_is_12() {
        assert_eq!(USERNAME_MAX_LEN, 12);
    }
}
