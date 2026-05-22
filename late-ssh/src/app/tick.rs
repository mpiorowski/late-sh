use std::time::Instant;

use super::state::{App, GAME_SELECTION_SNAKE, GAME_SELECTION_TETRIS};
use crate::app::activity::channel::ACTIVITY_HISTORY_MAX_EVENTS;
use crate::app::activity::event::ActivityKind;
use crate::app::activity::filter::ActivityFilter;
use crate::app::common::primitives::Screen;
use crate::session::SessionMessage;
use late_core::models::user::AudioSource;

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
        self.chat.poll_terminal_images();
        for output in self.chat.take_mod_outputs() {
            self.mod_modal_state
                .append_result(output.success, output.lines);
        }
        self.sync_visible_chat_room();
        if self.chat.pending_chat_screen_switch {
            self.chat.pending_chat_screen_switch = false;
            self.set_screen(Screen::Dashboard);
        }
        if let Some((user_id, username)) = self.chat.take_requested_open_profile() {
            self.profile_modal_state.open(user_id, username);
            self.show_profile_modal = true;
        }
        if let Some(b) = self.vote.tick() {
            self.banner = Some(b);
        }
        if let Some(b) = self.audio.tick() {
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

        for msg in messages {
            match msg {
                SessionMessage::Heartbeat => {}
                SessionMessage::Viz(viz) => {
                    self.push_viz_frame(viz);
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
                }
                SessionMessage::ClipboardImageFailed { message } => {
                    self.chat.clear_pending_clipboard_image_upload();
                    self.banner = Some(crate::app::common::primitives::Banner::error(&message));
                }
                SessionMessage::Terminate { reason } => {
                    tracing::info!(reason, "session terminated by control message");
                    self.running = false;
                }
                SessionMessage::ArtboardBanChanged { banned, expires_at } => {
                    self.set_artboard_banned(banned, expires_at);
                }
                SessionMessage::PermissionsChanged { permissions } => {
                    self.set_permissions(permissions);
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
                }
                SessionMessage::BrowserPaired => {
                    self.replay_paired_browser_source();
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
        // Pinstar Browser Actions
        if let Some(action) = self.pinstar_browser.pending_action.take() {
            let registry = self.pinstar_registry.clone();
            let user_id = self.user_id;
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.pinstar_open_rx = Some(rx);
            
            match action {
                crate::app::pinstar::browser::BrowserAction::Create { title } => {
                    tokio::spawn(async move {
                        let res = registry.create_new_diagram(user_id, title).await;
                        let _ = tx.send(res.map(|id| (id, "owner".to_string())));
                    });
                }
                crate::app::pinstar::browser::BrowserAction::Open(id, role) => {
                    let _ = tx.send(Ok((id, role)));
                }
                crate::app::pinstar::browser::BrowserAction::AcceptInvite(token) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        if let Some(db) = db {
                            let res = crate::app::pinstar::browser::accept_invite(&db, user_id, token).await;
                            let _ = tx.send(res.map(|id| (id, "editor".to_string())));
                        } else {
                            let _ = tx.send(Err(anyhow::anyhow!("No DB configured")));
                        }
                    });
                }
                crate::app::pinstar::browser::BrowserAction::GenerateInvite(diagram_id) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                match db.get().await {
                                    Ok(client) => {
                                        let token = late_core::models::pinstar_invite::PinstarInvite::generate_token();
                                        let params = late_core::models::pinstar_invite::PinstarInviteParams {
                                            diagram_id,
                                            token: token.clone(),
                                            role: "editor".to_string(),
                                            uses_left: Some(10),
                                            expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(24)),
                                        };
                                        match late_core::models::pinstar_invite::PinstarInvite::create(&client, params).await {
                                            Ok(_) => { let _ = tx.send(Ok((diagram_id, format!("invite:{}", token)))); }
                                            Err(e) => { let _ = tx.send(Err(anyhow::anyhow!("Failed: {}", e))); }
                                        }
                                    }
                                    Err(e) => { let _ = tx.send(Err(anyhow::anyhow!("DB error: {}", e))); }
                                }
                            }
                            None => { let _ = tx.send(Err(anyhow::anyhow!("No DB configured"))); }
                        }
                    });
                }
                crate::app::pinstar::browser::BrowserAction::Delete(id) => {
                    // If deleting the currently open diagram, close the editor
                    let should_close = self.pinstar_state.as_ref().is_some_and(|s| {
                        matches!(&s.mode, crate::app::pinstar::state::PinstarMode::Shared { service, .. } if service.diagram_id() == id)
                    });
                    if should_close {
                        self.pinstar_state = None;
                    }
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                match db.get().await {
                                    Ok(client) => {
                                        // Delete members first, then diagram
                                        let _ = late_core::models::pinstar_diagram_member::PinstarDiagramMember::delete_by_diagram(&client, id).await;
                                        match late_core::models::pinstar_diagram::PinstarDiagram::delete(&client, id).await {
                                            Ok(_) => { let _ = tx.send(Ok((id, "deleted".to_string()))); }
                                            Err(e) => { let _ = tx.send(Err(anyhow::anyhow!("Delete failed: {}", e))); }
                                        }
                                    }
                                    Err(e) => { let _ = tx.send(Err(anyhow::anyhow!("DB error: {}", e))); }
                                }
                            }
                            None => { let _ = tx.send(Err(anyhow::anyhow!("No DB configured"))); }
                        }
                    });
                    // Refresh list after delete completes
                }
                crate::app::pinstar::browser::BrowserAction::Rename(id, new_title) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                match db.get().await {
                                    Ok(client) => {
                                        match late_core::models::pinstar_diagram::PinstarDiagram::update_title(&client, id, &new_title).await {
                                            Ok(_) => { let _ = tx.send(Ok((id, "renamed".to_string()))); }
                                            Err(e) => { let _ = tx.send(Err(anyhow::anyhow!("Rename failed: {}", e))); }
                                        }
                                    }
                                    Err(e) => { let _ = tx.send(Err(anyhow::anyhow!("DB error: {}", e))); }
                                }
                            }
                            None => { let _ = tx.send(Err(anyhow::anyhow!("No DB configured"))); }
                        }
                    });
                }
            }
        }

        // Poll Pinstar open results
        if let Some(rx) = &mut self.pinstar_open_rx {
            match rx.try_recv() {
                Ok(Ok((id, role))) => {
                    self.pinstar_open_rx = None;
                    if let Some(token) = role.strip_prefix("invite:") {
                        self.pinstar_browser.generated_invite_token = Some(token.to_string());
                        self.pinstar_browser.error = None;
                        self.banner = Some(crate::app::common::primitives::Banner::success("Invite link created"));
                    } else if role == "deleted" {
                        self.banner = Some(crate::app::common::primitives::Banner::success("Diagram deleted"));
                        self.refresh_pinstar_browser();
                    } else if role == "renamed" {
                        self.banner = Some(crate::app::common::primitives::Banner::success("Diagram renamed"));
                        self.refresh_pinstar_browser();
                    } else {
                        self.start_pinstar_session(id, role);
                    }
                }
                Ok(Err(e)) => {
                    self.pinstar_open_rx = None;
                    if self.pinstar_browser.mode == crate::app::pinstar::browser::BrowserMode::GenerateInvite {
                        self.pinstar_browser.error = Some(e.to_string());
                    } else {
                        self.banner = Some(crate::app::common::primitives::Banner::error(&e.to_string()));
                    }
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.pinstar_open_rx = None;
                }
            }
        }

        // Poll Pinstar session results
        if let Some(rx) = &mut self.pinstar_session_rx {
            match rx.try_recv() {
                Ok(Ok((svc, role))) => {
                    self.pinstar_session_rx = None;
                    let title = svc.snapshot().title.clone();
                    self.pinstar_state = Some(crate::app::pinstar::state::PinstarState::new_shared(svc, role, title));
                    self.banner = Some(crate::app::common::primitives::Banner::success("Diagram opened"));
                }
                Ok(Err(e)) => {
                    self.pinstar_session_rx = None;
                    self.banner = Some(crate::app::common::primitives::Banner::error(&e.to_string()));
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.pinstar_session_rx = None;
                }
            }
        }

        // Poll Pinstar list results
        if let Some(rx) = &mut self.pinstar_list_rx {
            match rx.try_recv() {
                Ok(Ok(entries)) => {
                    self.pinstar_list_rx = None;
                    self.pinstar_browser.entries = entries;
                    self.pinstar_browser.loading = false;
                }
                Ok(Err(e)) => {
                    self.pinstar_list_rx = None;
                    self.pinstar_browser.loading = false;
                    self.pinstar_browser.error = Some(e.to_string());
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.pinstar_list_rx = None;
                    self.pinstar_browser.loading = false;
                }
            }
        }

        // Pinstar: reload diagram if file changed on disk, or drain events
        if let Some(state) = self.pinstar_state.as_mut() {
            if let crate::app::pinstar::state::PinstarMode::Local { .. } = &state.mode {
                if let Ok(metadata) = std::fs::metadata(&state.path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified > state.last_modified {
                            let _ = state.reload();
                        }
                    }
                }
            } else {
                state.drain_service_events();
            }

            // Poll invite results
            if let Some(rx) = &mut state.invite_result_rx {
                match rx.try_recv() {
                    Ok(Ok(token)) => {
                        state.invite_token = Some(token);
                        state.invite_result_rx = None;
                    }
                    Ok(Err(err)) => {
                        state.invite_error = Some(err);
                        state.invite_result_rx = None;
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        state.invite_error = Some("Invite task failed unexpectedly".to_string());
                        state.invite_result_rx = None;
                    }
                }
            }
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

        let shop_tick = self.shop_state.tick();
        if let Some(banner) = shop_tick.banner {
            self.banner = Some(banner);
        }
        if shop_tick.snapshot_changed
            && self.shop_state.is_loaded()
            && self
                .active_room_game
                .as_ref()
                .is_none_or(|game| game.can_sync_external_chip_balance())
        {
            self.chip_balance = self.shop_state.balance();
            if let Some(active_room_game) = &mut self.active_room_game {
                active_room_game.sync_external_chip_balance(self.chip_balance);
            }
        }

        // Bonsai passive growth
        self.bonsai_state.tick();
        self.cat_state.tick();
        if self.show_bonsai_modal {
            self.bonsai_care_state.tick();
        }

        if let Some(rx) = &mut self.activity_feed_rx {
            let activity_filter = ActivityFilter::dashboard();
            while let Ok(event) = rx.try_recv() {
                if !activity_filter.includes(&event) {
                    continue;
                }
                if matches!(&event.kind, ActivityKind::UserJoined)
                    && let Some(user_id) = event.user_id
                    && let Some(b) = self.chat.note_friend_join(user_id, &event.username)
                {
                    self.banner = Some(b);
                }
                self.activity.push_back(event);
                if self.activity.len() > ACTIVITY_HISTORY_MAX_EVENTS {
                    self.activity.pop_front();
                }
            }
        }

        // Browser-audible audio is synthetic-only. If a CLI is paired and the
        // user is in Icecast mode, the CLI owns Icecast and sends real
        // VizFrames, so don't mask those with the browser's procedural path.
        let has_browser = self
            .paired_client_state()
            .map(|state| state.client_kind == crate::app::audio::client_state::ClientKind::Browser)
            .unwrap_or(false);
        let browser_owns_icecast = self
            .paired_client_registry
            .as_ref()
            .map(|registry| registry.web_icecast_enabled(&self.session_token))
            .unwrap_or(false);
        let procedural = has_browser
            && (self.paired_browser_source == AudioSource::Youtube || browser_owns_icecast);
        self.visualizer.set_procedural_active(procedural);
        if procedural {
            self.visualizer.tick_procedural();
        } else {
            self.visualizer.tick_idle();
        }
    }

    fn push_viz_frame(&mut self, frame: late_core::audio::VizFrame) {
        self.last_viz_frame_at = Some(Instant::now());
        self.visualizer.update(&frame);
        self.viz_frame_buffer.push_back(frame);
        while self.viz_frame_buffer.len() > 75 {
            self.viz_frame_buffer.pop_front();
        }
    }
}
