use std::time::Instant;

use super::state::{App, GAME_SELECTION_SNAKE, GAME_SELECTION_TETRIS};
use crate::app::activity::channel::ACTIVITY_HISTORY_MAX_EVENTS;
use crate::app::activity::event::ActivityKind;
use crate::app::activity::filter::ActivityFilter;
use crate::app::common::primitives::Screen;
use crate::app::common::theme;
use crate::app::files::inline_image::InlineImageRenderSettings;
use crate::app::pinstar::browser::BrowserActionResult;
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
        if let Some(room_id) = self.chat.take_requested_poll_room() {
            let allow_poll_modal = self.screen == Screen::Dashboard;
            crate::app::chat::input::open_requested_poll_modal(self, room_id, allow_poll_modal);
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
        self.chat
            .poll_inline_images(self.inline_image_render_settings());
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
            self.show_sheet_modal = false;
            self.sheet_modal_state.close();
            self.profile_modal_state.open(user_id, username);
            self.show_profile_modal = true;
        }
        if let Some(request) = self.chat.take_requested_open_sheet() {
            self.show_profile_modal = false;
            self.sheet_modal_state.open(request);
            self.show_sheet_modal = true;
        }
        if let Some(save) = self.sheet_modal_state.take_pending_save() {
            self.chat
                .service
                .save_sheet_task(self.user_id, save.room_id, save.name, save.body);
        }
        // Debounced profile-open from a single click on a chat-author
        // username. We held this back so a fast second click on the same
        // username can be promoted to inserting an `@mention` instead
        // (see `app::input::handle_chat_scroll_click`). Once the debounce
        // window elapses with no double-click, the modal opens.
        if let Some(pending) = self
            .pending_chat_profile_open
            .take_if(|p| p.time.elapsed() >= crate::app::input::PROFILE_CLICK_DEBOUNCE)
        {
            self.show_sheet_modal = false;
            self.sheet_modal_state.close();
            self.profile_modal_state
                .open(pending.user_id, pending.username);
            self.show_profile_modal = true;
        }
        if let Some(b) = self.audio.tick() {
            self.banner = Some(b);
        }
        self.voice.tick();
        self.drain_voice_join_results();
        // News state is ticked inside chat.tick()
        if let Some(b) = self.profile_state.tick() {
            self.banner = Some(b);
        }
        self.chat
            .set_favorite_room_ids(self.profile_state.profile().favorite_room_ids.clone());
        self.sudoku_state.poll_daily_generation();
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
                SessionMessage::UltimateCast {
                    ultimate_id,
                    seed,
                    duration_ms,
                } => {
                    if let Some(kind) =
                        self.ultimate_state
                            .apply_cast(&crate::app::ultimates::UltimateCast {
                                ultimate_id,
                                seed,
                                duration_ms,
                            })
                    {
                        let label = match kind {
                            crate::app::ultimates::UltimateKind::Wonderland => "Wonderland",
                            crate::app::ultimates::UltimateKind::Thematrix => "The Matrix",
                        };
                        self.banner = Some(crate::app::common::primitives::Banner::success(
                            &format!("{label} is in effect"),
                        ));
                    }
                }
                SessionMessage::UltimateCooldownUpdated {
                    ultimate_id,
                    remaining_ms,
                } => {
                    self.ultimate_state
                        .set_cooldown(&ultimate_id, std::time::Duration::from_millis(remaining_ms));
                }
                SessionMessage::UltimateCooldownDbRereadOk { cooldowns } => {
                    self.ultimate_state.replace_cooldowns(
                        cooldowns
                            .into_iter()
                            .map(|(ultimate_id, remaining_ms)| {
                                (ultimate_id, std::time::Duration::from_millis(remaining_ms))
                            })
                            .collect(),
                    );
                }
                SessionMessage::UltimateCastRejected {
                    ultimate_id,
                    remaining_ms,
                } => {
                    self.ultimate_state
                        .set_cooldown(&ultimate_id, std::time::Duration::from_millis(remaining_ms));
                    let label = crate::app::ultimates::UltimateKind::from_id(&ultimate_id)
                        .map(crate::app::ultimates::UltimateKind::name)
                        .unwrap_or("Ultimate");
                    let message = if remaining_ms > 0 {
                        format!(
                            "{label} is cooling down ({})",
                            crate::app::ultimates::format_cooldown(
                                std::time::Duration::from_millis(remaining_ms)
                            )
                        )
                    } else {
                        format!("Could not cast {label}")
                    };
                    self.banner = Some(crate::app::common::primitives::Banner::error(&message));
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
                selection if crate::app::arcade::input::is_nes_selection(selection) => {
                    self.nes_cabinet_state.tick();
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
        if let Some(state) = self.lateania_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.rebels_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.nethack_state.as_mut() {
            state.tick();
        }
        // Door games are launched from the Games hub, so they return there when
        // they exit. Rebels flips out of Running the tick its proxy closes;
        // NetHack does the same but first holds a short input grace (so a dying
        // player's key-mashing can't fall through), so wait that out first.
        if self.screen == Screen::Rebels
            && self.rebels_state.as_ref().is_none_or(|s| !s.is_running())
        {
            self.set_screen(Screen::Games);
        }
        if self.screen == Screen::Nethack
            && self
                .nethack_state
                .as_ref()
                .is_none_or(|s| !s.is_running() && !s.in_exit_grace())
        {
            self.set_screen(Screen::Games);
        }
        // Pinstar Browser Actions
        if let Some(action) = self.pinstar_browser.pending_action.take() {
            use crate::app::pinstar::browser::BrowserActionResult;

            let registry = self.pinstar_registry.clone();
            let user_id = self.user_id;
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.pinstar_open_rx = Some(rx);

            match action {
                crate::app::pinstar::browser::BrowserAction::Create { title } => {
                    tokio::spawn(async move {
                        let res = registry.create_new_diagram(user_id, title).await;
                        let _ = tx.send(res.map(|id| BrowserActionResult::Open {
                            id,
                            role: "owner".to_string(),
                        }));
                    });
                }
                crate::app::pinstar::browser::BrowserAction::Import { title, data } => {
                    tokio::spawn(async move {
                        let res = registry.import_diagram(user_id, title, data).await;
                        let _ = tx.send(res.map(|id| BrowserActionResult::Open {
                            id,
                            role: "owner".to_string(),
                        }));
                    });
                }
                crate::app::pinstar::browser::BrowserAction::Open(id, role) => {
                    let _ = tx.send(Ok(BrowserActionResult::Open { id, role }));
                }
                crate::app::pinstar::browser::BrowserAction::AcceptInvite(token) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        if let Some(db) = db {
                            let res =
                                crate::app::pinstar::browser::accept_invite(&db, user_id, token)
                                    .await;
                            let _ = tx
                                .send(res.map(|(id, role)| BrowserActionResult::Open { id, role }));
                        } else {
                            let _ = tx.send(Err(anyhow::anyhow!("no db configured")));
                        }
                    });
                }
                crate::app::pinstar::browser::BrowserAction::GenerateInvite(diagram_id) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                let res = crate::app::pinstar::browser::create_invite_for_owner(
                                    &db,
                                    user_id,
                                    diagram_id,
                                    "editor".to_string(),
                                )
                                .await
                                .map(|token| BrowserActionResult::InviteCreated { token });
                                let _ = tx.send(res);
                            }
                            None => {
                                let _ = tx.send(Err(anyhow::anyhow!("no db configured")));
                            }
                        }
                    });
                }
                crate::app::pinstar::browser::BrowserAction::CopySource(diagram_id) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                let res =
                                    crate::app::pinstar::browser::copy_diagram_source_for_member(
                                        &db, user_id, diagram_id,
                                    )
                                    .await
                                    .map(|source| BrowserActionResult::CopiedSource { source });
                                let _ = tx.send(res);
                            }
                            None => {
                                let _ = tx.send(Err(anyhow::anyhow!("no db configured")));
                            }
                        }
                    });
                }
                crate::app::pinstar::browser::BrowserAction::Delete(id) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                let res = crate::app::pinstar::browser::delete_diagram_for_user(
                                    &db, user_id, id,
                                )
                                .await
                                .map(|_| (id, "deleted".to_string()));
                                if res.is_ok() {
                                    registry.evict(id);
                                }
                                let _ = tx.send(res.map(|_| BrowserActionResult::Deleted { id }));
                            }
                            None => {
                                let _ = tx.send(Err(anyhow::anyhow!("no db configured")));
                            }
                        }
                    });
                    // Refresh list after delete completes
                }
                crate::app::pinstar::browser::BrowserAction::Rename(id, new_title) => {
                    let db = self.pinstar_registry.db();
                    tokio::spawn(async move {
                        match db {
                            Some(db) => {
                                let res = crate::app::pinstar::browser::rename_diagram_for_owner(
                                    &db, user_id, id, &new_title,
                                )
                                .await
                                .map(|_| BrowserActionResult::Renamed);
                                let _ = tx.send(res);
                            }
                            None => {
                                let _ = tx.send(Err(anyhow::anyhow!("no db configured")));
                            }
                        }
                    });
                }
            }
        }

        // Poll Pinstar open results
        if let Some(rx) = &mut self.pinstar_open_rx {
            match rx.try_recv() {
                Ok(Ok(result)) => {
                    self.pinstar_open_rx = None;
                    match result {
                        BrowserActionResult::InviteCreated { token } => {
                            self.pinstar_browser.generated_invite_token = Some(token);
                            self.pinstar_browser.error = None;
                            self.banner = Some(crate::app::common::primitives::Banner::success(
                                "Invite link created",
                            ));
                        }
                        BrowserActionResult::CopiedSource { source } => {
                            self.pending_clipboard = Some(source);
                            self.banner = Some(crate::app::common::primitives::Banner::success(
                                "Diagram source copied to clipboard",
                            ));
                        }
                        BrowserActionResult::Deleted { id } => {
                            if self.pinstar_state.as_ref().is_some_and(|s| {
                                matches!(&s.mode, crate::app::pinstar::state::PinstarMode::Shared { service, .. } if service.diagram_id() == id)
                            }) {
                                self.pinstar_state = None;
                            }
                            self.pinstar_registry.evict(id);
                            self.banner = Some(crate::app::common::primitives::Banner::success(
                                "Diagram deleted",
                            ));
                            self.refresh_pinstar_browser();
                        }
                        BrowserActionResult::Renamed => {
                            self.banner = Some(crate::app::common::primitives::Banner::success(
                                "Diagram renamed",
                            ));
                            self.refresh_pinstar_browser();
                        }
                        BrowserActionResult::Open { id, role } => {
                            self.start_pinstar_session(id, role);
                        }
                    }
                }
                Ok(Err(e)) => {
                    self.pinstar_open_rx = None;
                    if self.pinstar_browser.mode
                        == crate::app::pinstar::browser::BrowserMode::GenerateInvite
                    {
                        self.pinstar_browser.error = Some(e.to_string());
                    } else {
                        self.banner = Some(crate::app::common::primitives::Banner::error(
                            &e.to_string(),
                        ));
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
                    self.pinstar_state = Some(
                        crate::app::pinstar::state::PinstarState::new_shared(svc, role, title),
                    );
                    self.banner = Some(crate::app::common::primitives::Banner::success(
                        "Diagram opened",
                    ));
                }
                Ok(Err(e)) => {
                    self.pinstar_session_rx = None;
                    self.banner = Some(crate::app::common::primitives::Banner::error(
                        &e.to_string(),
                    ));
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
                    self.pinstar_browser.clamp_selection();
                    self.pinstar_browser.error = None;
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
                if let Ok(metadata) = std::fs::metadata(&state.path)
                    && let Ok(modified) = metadata.modified()
                    && modified > state.last_modified
                {
                    let _ = state.reload();
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

            // Deferred save (avoid blocking event loop on drag end)
            if state.needs_save {
                state.needs_save = false;
                let _ = state.save();
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

        let quest_tick = self.quest_state.tick();
        if let Some(banner) = quest_tick.banner {
            self.banner = Some(banner);
        }

        let shop_tick = self.shop_state.tick();
        if let Some(banner) = shop_tick.banner {
            self.banner = Some(banner);
        }

        let admin_tick = self.hub_admin_state.tick(self.is_admin);
        if let Some(banner) = admin_tick.banner {
            self.banner = Some(banner);
        }

        self.ultimate_state.tick();
        if shop_tick.snapshot_changed && self.shop_state.is_loaded() {
            let equipped_badge = self.shop_state.equipped_chat_badge();
            self.chat
                .set_chat_badge(self.user_id, equipped_badge.as_deref());
            let active_bumped_join_room_ids = self.shop_state.active_bumped_join_room_ids();
            if self
                .chat
                .set_active_bumped_join_room_ids(active_bumped_join_room_ids)
            {
                self.sync_visible_chat_room();
            }
            self.aquarium_state
                .set_active_creatures(&self.shop_state.active_aquarium_fish());
            self.aquarium_state
                .set_hungry(self.shop_state.aquarium_hungry());
            if !self.shop_state.entitlements().has_aquarium() {
                self.show_aquarium_tray = false;
            }
            if !self.shop_state.dynamic_bonsai_enabled() {
                self.show_bonsai_v2_modal = false;
            }
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
        let bonsai_v2_active = self.bonsai_v2_activity_ticks_remaining > 0;
        self.bonsai_v2_activity_ticks_remaining =
            self.bonsai_v2_activity_ticks_remaining.saturating_sub(1);
        if self.use_bonsai_v2() {
            self.bonsai_v2_state.tick(bonsai_v2_active);
        }
        self.pet_state.tick();
        if self.show_aquarium_tray {
            self.aquarium_state.tick();
        }
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

    fn inline_image_render_settings(&self) -> InlineImageRenderSettings {
        InlineImageRenderSettings {
            symbol_mode: self.inline_image_symbol_mode,
            background_rgb: self.inline_image_background_rgb(),
        }
    }

    fn inline_image_background_rgb(&self) -> Option<u32> {
        let (enabled, theme_id) = if self.show_settings {
            (
                self.settings_modal_state.draft().enable_background_color,
                self.settings_modal_state
                    .draft()
                    .theme_id
                    .as_deref()
                    .unwrap_or_else(|| self.profile_state.theme_id()),
            )
        } else {
            (
                self.profile_state.profile().enable_background_color,
                self.profile_state.theme_id(),
            )
        };
        enabled.then(|| packed_rgb(theme::preview_for_id(theme_id).bg_canvas))
    }
}

fn packed_rgb(color: ratatui::style::Color) -> u32 {
    let hex = theme::color_to_hex(color);
    u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0)
}
