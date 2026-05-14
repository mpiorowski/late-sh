use std::time::Instant;

use late_core::audio::VizFrame;

use super::state::{App, GAME_SELECTION_SNAKE, GAME_SELECTION_TETRIS};
use crate::app::activity::channel::ACTIVITY_HISTORY_MAX_EVENTS;
use crate::app::activity::filter::ActivityFilter;
use crate::app::common::primitives::Screen;
use crate::session::{BrowserVizFrame, SessionMessage};

impl App {
    pub fn tick(&mut self) {
        crate::app::input::flush_pending_escape(self);

        if self.show_splash {
            self.splash_ticks = self.splash_ticks.saturating_add(1);
            if self.splash_ticks > 90 {
                self.show_splash = false;
            }
        }

        let mut messages = Vec::new();
        if let Some(rx) = &mut self.session_rx {
            while let Ok(msg) = rx.try_recv() {
                messages.push(msg);
            }
        }

        self.sync_visible_chat_room();

        // Services
        if let Some(b) = self.chat.tick() {
            self.banner = Some(b);
        }
        // Poll image upload results.
        if let Some(result) = self.chat.poll_image_upload() {
            let target_room_id = self.chat.take_image_upload_target_room_id();
            match result {
                Ok(url) => {
                    if let Some(room_id) = target_room_id.or(self.chat.selected_room_id) {
                        self.chat.start_composing_in_room(room_id);
                        self.chat.composer_push_str(&url);
                    }
                    self.banner = Some(crate::app::common::primitives::Banner::success(
                        "Image uploaded - press Enter to send",
                    ));
                }
                Err(msg) => {
                    self.banner = Some(crate::app::common::primitives::Banner::error(&msg));
                }
            }
        }
        self.chat.poll_inline_images();
        for output in self.chat.take_mod_outputs() {
            self.mod_modal_state
                .append_result(output.success, output.lines);
        }
        self.sync_visible_chat_room();
        if self.chat.pending_chat_screen_switch {
            self.chat.pending_chat_screen_switch = false;
            self.set_screen(Screen::Dashboard);
        }
        if let Some(b) = self.vote.tick() {
            self.banner = Some(b);
        }
        // News state is ticked inside chat.tick()
        if let Some(b) = self.profile_state.tick() {
            self.banner = Some(b);
        }
        self.chat
            .set_favorite_room_ids(self.profile_state.profile().favorite_room_ids.clone());
        if let Some(b) = self.settings_modal_state.tick() {
            self.banner = Some(b);
        }
        if self.show_profile_modal {
            self.profile_modal_state.tick();
        }
        if self.show_settings
            && self.settings_modal_state.draft().username.is_empty()
            && !self.profile_state.profile().username.is_empty()
        {
            if self.profile_state.profile().show_settings_on_connect {
                self.settings_modal_state
                    .open_from_profile(self.profile_state.profile());
            } else {
                self.show_settings = false;
            }
        }

        let mut updated = false;
        for msg in messages {
            match msg {
                SessionMessage::Heartbeat => {}
                SessionMessage::Viz(viz) => {
                    self.push_browser_frame(viz);
                    updated = true;
                }
                SessionMessage::ClipboardImage { data } => {
                    let Some(upload) = self.chat.take_pending_clipboard_image_upload() else {
                        tracing::warn!("ignoring unsolicited paired clipboard image");
                        continue;
                    };
                    if let Some(banner) = self.chat.start_image_upload_in_room(data, upload.room_id)
                    {
                        self.banner = Some(banner);
                    } else {
                        self.banner = Some(crate::app::common::primitives::Banner::success(
                            "Clipboard image found - uploading...",
                        ));
                    }
                    updated = true;
                }
                SessionMessage::ClipboardImageFailed { message } => {
                    self.chat.clear_pending_clipboard_image_upload();
                    self.banner = Some(crate::app::common::primitives::Banner::error(&message));
                    updated = true;
                }
                SessionMessage::Terminate { reason } => {
                    tracing::info!(reason, "session terminated by control message");
                    self.running = false;
                }
                SessionMessage::ArtboardBanChanged { banned, expires_at } => {
                    self.set_artboard_banned(banned, expires_at);
                    updated = true;
                }
                SessionMessage::PermissionsChanged { permissions } => {
                    self.set_permissions(permissions);
                    updated = true;
                }
                SessionMessage::RoomRemoved {
                    room_id,
                    slug,
                    message,
                } => {
                    self.chat.remove_room_for_moderation(room_id);
                    self.chat.request_list();
                    self.banner = Some(crate::app::common::primitives::Banner::error(&format!(
                        "{message}: #{slug}"
                    )));
                    updated = true;
                }
            }
        }
        self.expire_artboard_ban_if_needed();

        if self.screen == Screen::Arcade && self.is_playing_game {
            match self.game_selection {
                GAME_SELECTION_TETRIS => {
                    self.tetris_state.tick();
                }
                GAME_SELECTION_SNAKE => {
                    self.snake_state.tick();
                }
                _ => (),
            }
        }
        if let Some(active_room_game) = &mut self.active_room_game {
            active_room_game.tick();
        }
        if let Some(b) = self.tick_rooms() {
            self.banner = Some(b);
        }
        if let Some(state) = self.dartboard_state.as_mut() {
            state.tick();
        }
        if let Some(balance) = self
            .active_room_game
            .as_ref()
            .and_then(|game| game.chip_balance())
        {
            self.chip_balance = balance;
        }

        // Leaderboard
        if let Some(rx) = &mut self.leaderboard_rx
            && rx.has_changed().unwrap_or(false)
        {
            self.leaderboard = rx.borrow_and_update().clone();
            if let Some(&balance) = self.leaderboard.user_chips.get(&self.user_id)
                && self
                    .active_room_game
                    .as_ref()
                    .is_none_or(|game| game.can_sync_external_chip_balance())
            {
                self.chip_balance = balance;
                if let Some(active_room_game) = &mut self.active_room_game {
                    active_room_game.sync_external_chip_balance(balance);
                }
            }
        }

        // Bonsai passive growth
        self.bonsai_state.tick();
        if self.show_bonsai_modal {
            self.bonsai_care_state.tick();
        }

        if let Some(rx) = &mut self.activity_feed_rx {
            let activity_filter = ActivityFilter::dashboard();
            while let Ok(event) = rx.try_recv() {
                if !activity_filter.includes(&event) {
                    continue;
                }
                self.activity.push_back(event);
                if self.activity.len() > ACTIVITY_HISTORY_MAX_EVENTS {
                    self.activity.pop_front();
                }
            }
        }

        if updated {
            if let Some(frame) = self.browser_viz_buffer.back().cloned() {
                self.visualizer.update(&frame);
            }
        } else {
            self.visualizer.tick_idle();
        }
    }

    fn push_browser_frame(&mut self, frame: BrowserVizFrame) {
        self.last_browser_viz_at = Some(Instant::now());
        let viz = VizFrame {
            bands: frame.bands,
            rms: frame.rms,
            track_pos_ms: frame.position_ms,
        };
        self.browser_viz_buffer.push_back(viz);
        while self.browser_viz_buffer.len() > 75 {
            self.browser_viz_buffer.pop_front();
        }
    }
}
