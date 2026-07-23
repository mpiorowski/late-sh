use std::time::{Duration, Instant};

use super::state::{App, GAME_SELECTION_SNAKE, GAME_SELECTION_TETRIS, GAME_SELECTION_TRAFFIC};
use crate::app::activity::event::ActivityKind;
use crate::app::common::primitives::Screen;
use crate::app::common::theme;
use crate::app::files::inline_image::InlineImageRenderSettings;
use crate::app::pinstar::browser::BrowserActionResult;
use crate::session::SessionMessage;
use late_core::models::user::AudioSource;

/// The hot world-tick cadence (the classic 15fps): animations that earn
/// full rate run here.
pub(crate) const HOT_TICK: Duration = Duration::from_millis(66);
/// Clubhouse ambience cadence (~4fps).
pub(crate) const AMBIENT_TICK: Duration = Duration::from_millis(266);
/// Idle floor: nothing visible animates, ticks only drain service channels.
/// Worst-case latency for an unprompted event (a chat message arriving
/// while idle) is one floor interval; input and push wakes stay instant.
pub(crate) const IDLE_TICK: Duration = Duration::from_millis(500);
/// After any input, hold the hot cadence briefly so async responses to that
/// input (menu DB loads, chat send echo) land at typing latency.
const POST_INPUT_HOT_WINDOW: Duration = Duration::from_secs(2);

impl App {
    /// Advance world time by one tick. Returns true when anything render-
    /// visible may have changed, so the render loop can skip drawing clean
    /// frames. Prove-clean, not prove-dirty: anything uncertain reports
    /// changed, because a spurious frame costs nothing while a wrong "clean"
    /// freezes part of the UI.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        let screen_before = self.screen;
        let chat_context_epoch_before = self.chat.context_epoch();
        let chat_ctx_epoch_before = self.chat_ctx_epoch;

        let pending_escape_before = self.pending_escape;
        crate::app::input::flush_pending_escape(self);
        if pending_escape_before && !self.pending_escape {
            changed = true;
        }

        // The counter is derived from wall time (one unit per 66ms) so
        // animation phase stays correct however sparsely the adaptive loop
        // ticks. Edge checks below compare period indexes against the
        // previous tick's value instead of `is_multiple_of`, which would
        // miss boundaries under sparse ticking.
        let prev_marquee_tick = self.marquee_tick;
        self.marquee_tick = (self.started_at.elapsed().as_millis() / 66) as usize;
        // Shared second-boundary edge for every 1Hz consumer in this tick.
        // None fires immediately so the first frames already have presence,
        // directory, and clock state.
        let one_hz = match self.last_one_hz_index {
            None => true,
            Some(prev) => self.marquee_tick / 15 != prev,
        };
        self.last_one_hz_index = Some(self.marquee_tick / 15);

        if self.show_splash {
            // The splash types one character per tick and self-expires.
            changed = true;
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
        // Heartbeats are a liveness no-op (matched below); a heartbeat-only
        // drain must not pay a frame.
        if messages
            .iter()
            .any(|m| !matches!(m, SessionMessage::Heartbeat))
        {
            changed = true;
        }

        self.sync_visible_chat_room();
        self.tick_clubhouse();
        if self.screen == Screen::Clubhouse && self.marquee_tick / 4 != prev_marquee_tick / 4 {
            // Only cosmetic ambience animates on the tick counter (jukebox
            // EQ, emote arms, fire/candles/stars); walker positions are
            // input-driven and the dog step is wall-clock. A ~4fps
            // heartbeat keeps the ambience moving at a quarter of the
            // cost, and every discrete change (input, chat bubbles, door
            // events) still lands within 266ms of its tick.
            changed = true;
        }

        // Expire a stale paired-clipboard wait here rather than inside
        // chat.tick(): the registry slot must be cancelled along with it, so
        // a late CLI response can't satisfy a newer request or an armed slot
        // linger after the banner already reported the timeout.
        if let Some(b) = self.chat.expire_pending_clipboard_image_upload() {
            if let Some(registry) = &self.paired_client_registry {
                registry.cancel_clipboard_request(&self.session_token);
            }
            self.banner = Some(b);
            changed = true;
        }

        // Services
        let chat_tick = self.chat.tick();
        changed |= chat_tick.changed;
        if let Some(b) = chat_tick.banner {
            self.banner = Some(b);
            changed = true;
        }
        // Fire a debounced message search for the Ctrl+/ modal's `?` mode.
        crate::app::room_search_modal::state::tick_message_search(self);
        if let Some(room_id) = self.chat.take_requested_poll_room() {
            let allow_poll_modal = self.screen == Screen::Dashboard;
            crate::app::chat::input::open_requested_poll_modal(self, room_id, allow_poll_modal);
            changed = true;
        }
        // Poll image upload results.
        if let Some(result) = self.chat.poll_image_upload() {
            changed = true;
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
        changed |= self.chat.poll_terminal_images();
        for output in self.chat.take_mod_outputs() {
            self.mod_modal_state
                .append_result(output.success, output.lines);
            changed = true;
        }
        self.sync_visible_chat_room();
        if self.chat.pending_chat_screen_switch {
            self.chat.pending_chat_screen_switch = false;
            self.set_screen(Screen::Dashboard);
        }
        if let Some((user_id, username)) = self.chat.take_requested_open_profile() {
            self.open_profile_modal(user_id, username);
            changed = true;
        }
        if let Some(request) = self.chat.take_requested_open_sheet() {
            self.show_profile_modal = false;
            self.sheet_modal_state.open(request);
            self.show_sheet_modal = true;
            changed = true;
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
            self.open_profile_modal(pending.user_id, pending.username);
            changed = true;
        }
        let audio_tick = self.audio.tick();
        changed |= audio_tick.changed;
        if let Some(b) = audio_tick.banner {
            self.banner = Some(b);
            changed = true;
        }
        changed |= self.voice.tick();
        changed |= self.drain_voice_join_results();
        // News state is ticked inside chat.tick()
        let profile_tick = self.profile_state.tick();
        changed |= profile_tick.changed;
        if let Some(b) = profile_tick.banner {
            self.banner = Some(b);
            changed = true;
        }
        self.chat
            .set_favorite_room_ids(self.profile_state.profile().favorite_room_ids.clone());
        changed |= self.sudoku_state.poll_daily_generation();
        let settings_tick = self.settings_modal_state.tick();
        changed |= settings_tick.changed;
        if let Some(b) = settings_tick.banner {
            self.banner = Some(b);
            changed = true;
        }
        if self.show_profile_modal {
            changed |= self.profile_modal_state.tick();
        }
        if self.show_settings
            && self.settings_modal_state.draft().username.is_empty()
            && !self.profile_state.profile().username.is_empty()
        {
            self.settings_modal_state
                .open_from_profile(self.profile_state.profile());
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
                SessionMessage::Toast { message, error } => {
                    self.banner = Some(if error {
                        crate::app::common::primitives::Banner::error(&message)
                    } else {
                        crate::app::common::primitives::Banner::success(&message)
                    });
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
                    changed |= self.tetris_state.tick();
                }
                GAME_SELECTION_SNAKE => {
                    changed |= self.snake_state.tick();
                }
                GAME_SELECTION_TRAFFIC => {
                    changed |= self.traffic_state.tick();
                }
                _ => (),
            }
        }
        let daily_tick = self.daily.tick();
        changed |= daily_tick.changed;
        if let Some(b) = daily_tick.banner {
            self.banner = Some(b);
            changed = true;
        }
        // Modal cursor, pending claim, and glow follow the daily snapshot.
        self.lobby.sync(&self.daily);
        // The match chat room id only becomes known once the board's row
        // loads, so the visible-room sync (read marker + tail) and the
        // one-time idempotent join both key off the loaded detail here
        // rather than off the screen switch.
        if self.screen == crate::app::common::primitives::Screen::DailyMatch {
            self.sync_visible_chat_room();
            if let Some(chat_room_id) = self.daily.board_chat_room_id()
                && let Some(board) = self.daily.board.as_mut()
                && !board.chat_join_requested
            {
                board.chat_join_requested = true;
                self.chat.join_game_room_chat(chat_room_id);
            }
        }
        let house_changed = self.house.tick();
        if self.screen == crate::app::common::primitives::Screen::HouseTable {
            // The five runtimes report real change from their snapshot
            // peeks: server loops go quiet between rounds, and every
            // countdown (including poker's action clock) is republished
            // server-side each second. Off-screen turn alerts ride the
            // notify outbox.
            changed |= house_changed;
            self.sync_visible_chat_room();
            if let Some(chat_room_id) = self.house.chat_room_id()
                && !self.house.chat_join_requested
            {
                self.house.chat_join_requested = true;
                self.chat.join_game_room_chat(chat_room_id);
            }
        }
        if let Some(state) = self.dartboard_state.as_mut() {
            // The shared canvas drains remote ops, snapshot swaps, and
            // archive loads here; it only exists while the Artboard screen
            // is up, and it reports its own changes (own edits are
            // input-driven).
            changed |= state.tick();
        }
        if let Some(state) = self.lateania_state.as_mut() {
            // Drain even off-screen so the snapshot stays current; only an
            // on-screen change pays a frame (the screen switch itself is
            // input-driven and forces one).
            let lateania_changed = state.tick();
            changed |= lateania_changed && self.screen == Screen::Lateania;
        }
        if let Some(state) = self.rebels_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.nethack_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.dcss_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.usurper_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.dopewars_state.as_mut() {
            state.tick();
        }
        if let Some(state) = self.greendragon_state.as_mut() {
            // Same off-screen drain rule as Lateania above.
            let greendragon_changed = state.tick();
            changed |= greendragon_changed && self.screen == Screen::GreenDragon;
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
                // `awaiting_handle` holds the screen through the arcade-name
                // lookup and claim prompt, which run before any game does.
                .is_none_or(|s| !s.is_running() && !s.in_exit_grace() && !s.awaiting_handle())
        {
            self.set_screen(Screen::Games);
        }
        if self.screen == Screen::Dcss
            && self
                .dcss_state
                .as_ref()
                // `awaiting_handle` holds the screen through the arcade-name
                // lookup and claim prompt, which run before any game does.
                .is_none_or(|s| !s.is_running() && !s.in_exit_grace() && !s.awaiting_handle())
        {
            self.set_screen(Screen::Games);
        }
        if self.screen == Screen::Usurper
            && self
                .usurper_state
                .as_ref()
                // `awaiting_handle` holds the screen through the arcade-name
                // lookup and claim prompt, which run before any game does.
                .is_none_or(|s| !s.is_running() && !s.in_exit_grace() && !s.awaiting_handle())
        {
            self.set_screen(Screen::Games);
        }
        if self.screen == Screen::Dopewars
            && self
                .dopewars_state
                .as_ref()
                .is_none_or(|s| !s.is_running() && !s.in_exit_grace())
        {
            self.set_screen(Screen::Games);
        }
        // Pinstar Browser Actions
        if self.pinstar_browser.pending_action.is_some() {
            changed = true;
        }
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

        // Poll Pinstar open results. Every completed poll below (result,
        // error, or closed channel) clears its receiver, so an rx that
        // transitions to None marks a render-visible change.
        let pinstar_open_rx_before = self.pinstar_open_rx.is_some();
        let pinstar_session_rx_before = self.pinstar_session_rx.is_some();
        let pinstar_list_rx_before = self.pinstar_list_rx.is_some();
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

        changed |= pinstar_open_rx_before && self.pinstar_open_rx.is_none();
        changed |= pinstar_session_rx_before && self.pinstar_session_rx.is_none();
        changed |= pinstar_list_rx_before && self.pinstar_list_rx.is_none();

        // Pinstar: reload diagram if file changed on disk, or drain events
        if let Some(state) = self.pinstar_state.as_mut() {
            if let crate::app::pinstar::state::PinstarMode::Local { .. } = &state.mode {
                if let Ok(metadata) = std::fs::metadata(&state.path)
                    && let Ok(modified) = metadata.modified()
                    && modified > state.last_modified
                {
                    let _ = state.reload();
                    changed = true;
                }
            } else {
                changed |= !state.drain_service_events().is_empty();
            }

            // Poll invite results; completed polls clear the receiver.
            let invite_rx_before = state.invite_result_rx.is_some();
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

            changed |= invite_rx_before && state.invite_result_rx.is_none();

            // Deferred save (avoid blocking event loop on drag end)
            if state.needs_save {
                state.needs_save = false;
                let _ = state.save();
            }
        }
        if let Some(balance) = self.house.client().and_then(|client| client.chip_balance())
            && balance != self.chip_balance
        {
            self.chip_balance = balance;
            changed = true;
        }

        // Drunk glow for chat author labels: copy out of the shared lobby
        // about once a second so renders read owned state, and re-reading
        // also lets the tint fade as the buzz decays. Username effects ride
        // the same cadence: one Arc clone of the flair directory, resolved
        // into paintable styles (which is also what steps shimmer at 1 Hz)
        // and expired at read.
        if one_hz {
            let drunk_levels = self.clubhouse.drunk_levels();
            if self.drunk_levels != drunk_levels {
                self.drunk_levels = drunk_levels;
                self.chat_ctx_epoch += 1;
            }
            if let Some(directory) = &self.flair_directory {
                let phase = crate::app::common::username_effect::shimmer_phase(self.marquee_tick);
                let name_styles = crate::app::common::username_effect::resolve_all(
                    &crate::app::common::username_effect::snapshot(directory),
                    phase,
                    chrono::Utc::now(),
                );
                if self.name_styles != name_styles {
                    self.name_styles = name_styles;
                    self.chat_ctx_epoch += 1;
                }
            }
            // Presence reads on the same cadence: renders consume these owned
            // values instead of locking `active_users` twice per frame.
            if let Some(active_users) = &self.active_users {
                let online_count = crate::state::online_human_count(active_users);
                if online_count != self.online_count {
                    self.online_count = online_count;
                    changed = true;
                }
            }
            let active_friend_names = self.chat.active_friend_names();
            if active_friend_names != self.active_friend_names {
                self.active_friend_names = active_friend_names;
                changed = true;
            }
            // The username directory swaps its Arc on every real change, so
            // pointer equality is the change signal for the row cache epoch.
            // Lives here (not in render) so a skipped frame cannot delay the
            // epoch bump that would schedule the repaint.
            let username_directory_snapshot = self
                .username_directory
                .as_ref()
                .map(crate::usernames::snapshot);
            let directory_changed =
                match (&username_directory_snapshot, &self.last_username_directory) {
                    (Some(current), Some(previous)) => !std::sync::Arc::ptr_eq(current, previous),
                    (None, None) => false,
                    _ => true,
                };
            if directory_changed {
                self.last_username_directory = username_directory_snapshot;
                self.chat_ctx_epoch += 1;
            }
            // AFK set: same Arc-swap-on-change contract as the directory.
            let afk_user_ids = crate::state::afk_users_snapshot(&self.afk_users);
            if !std::sync::Arc::ptr_eq(&afk_user_ids, &self.afk_user_ids) {
                self.afk_user_ids = afk_user_ids;
                self.chat_ctx_epoch += 1;
            }
            // Sidebar clock shows minutes; repaint on rollover.
            let sidebar_clock = crate::app::common::sidebar::sidebar_clock_text(
                self.profile_state.profile().timezone.as_deref(),
            );
            if sidebar_clock != self.last_sidebar_clock {
                self.last_sidebar_clock = sidebar_clock;
                changed = true;
            }
        }

        // Leaderboard
        if let Some(rx) = &mut self.leaderboard_rx
            && rx.has_changed().unwrap_or(false)
        {
            changed = true;
            self.leaderboard = rx.borrow_and_update().clone();
            if let Some(&balance) = self.leaderboard.user_chips.get(&self.user_id)
                && self
                    .house
                    .client()
                    .is_none_or(|client| client.can_sync_external_chip_balance())
            {
                self.chip_balance = balance;
                if let Some(client) = self.house.client_mut() {
                    client.sync_external_chip_balance(balance);
                }
            }
        }

        let quest_tick = self.quest_state.tick();
        changed |= quest_tick.snapshot_changed;
        if let Some(banner) = quest_tick.banner {
            self.banner = Some(banner);
            changed = true;
        }

        let shop_tick = self.shop_state.tick();
        changed |= shop_tick.snapshot_changed;
        if let Some(banner) = shop_tick.banner {
            self.banner = Some(banner);
            changed = true;
        }

        let admin_tick = self.hub_admin_state.tick(self.is_admin);
        changed |= admin_tick.changed;
        if let Some(banner) = admin_tick.banner {
            self.banner = Some(banner);
            changed = true;
        }

        // Active ultimates animate every tick; the expiry edge (active
        // before the retain, inactive after) still needs one frame to clear.
        let ultimate_was_active = self.ultimate_state.has_active_effect();
        self.ultimate_state.tick();
        changed |= ultimate_was_active || self.ultimate_state.has_active_effect();
        if shop_tick.snapshot_changed && self.shop_state.is_loaded() {
            let equipped_badge = self.shop_state.equipped_chat_badge();
            self.chat
                .set_chat_badge(self.user_id, equipped_badge.as_deref());
            self.aquarium_state
                .set_active_creatures(&self.shop_state.active_aquarium_fish());
            self.aquarium_state
                .set_hungry(self.shop_state.aquarium_hungry());
            if !self.shop_state.dynamic_bonsai_enabled() {
                self.show_bonsai_v2_modal = false;
            }
        }
        if shop_tick.snapshot_changed
            && self.shop_state.is_loaded()
            && self
                .house
                .client()
                .is_none_or(|client| client.can_sync_external_chip_balance())
        {
            self.chip_balance = self.shop_state.balance();
            let balance = self.chip_balance;
            if let Some(client) = self.house.client_mut() {
                client.sync_external_chip_balance(balance);
            }
        }

        // Bonsai growth comes from watering only; the tick just watches for
        // death during a live session.
        changed |= self.bonsai_state.tick();
        // Pet: state edges (feedback expiry, roam end, day-rollover mood and
        // needs flips) always count; the wander/blink/tail animation only
        // pays frames on ticks where the drawn strip actually differs, and
        // only while the last frame drew a strip at all (the travel slot is
        // rewritten every render). Every transition into visibility (screen
        // switch, settings, entitlements, roam end) dirties a frame through
        // its own path, which re-records the slot.
        changed |= self.pet_state.tick();
        if self.pet_state.roaming_active() {
            // The full-screen stroll overlay animates continuously.
            changed = true;
        } else if let Some(travel) = self.last_pet_strip_travel.get() {
            changed |= crate::app::pet::ui::strip_frame_changed(
                self.pet_state.mood(),
                self.pet_state.animation_ticks(),
                travel,
            );
        }
        // Mirror the render condition (render.rs `aquarium_tray_enabled`):
        // the tray setting defaults on for everyone, but only aquarium owners
        // ever see it, so only their sessions pay simulation frames.
        if self.show_aquarium_tray && self.shop_state.entitlements().has_aquarium() {
            changed |= self.aquarium_state.tick();
        }
        if self.show_bonsai_modal {
            changed |= self.bonsai_care_state.tick();
        }

        // The activity feed subscription survives the retired sidebar panel
        // for one job: edge-detecting friend joins for the friend-online
        // banner. The public feed itself ships to #lounge (activity/lounge).
        if let Some(rx) = &mut self.activity_feed_rx {
            while let Ok(event) = rx.try_recv() {
                if matches!(&event.kind, ActivityKind::UserJoined)
                    && let Some(user_id) = event.user_id
                    && let Some(b) = self.chat.note_friend_join(user_id, &event.username)
                {
                    self.banner = Some(b);
                    changed = true;
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
        let sidebar_visible = self.right_sidebar_visible();
        // The visualizer state always advances so decay keeps settling, but
        // it only costs frames while a surface that draws it is visible: the
        // right sidebar (viz and music-stage panels) or a bonsai modal
        // (beat-driven sway).
        let viz_ticked = if procedural {
            self.visualizer.tick_procedural()
        } else {
            self.visualizer.tick_idle()
        };
        changed |=
            viz_ticked && (sidebar_visible || self.show_bonsai_modal || self.show_bonsai_v2_modal);

        // Sidebar marquees: track rows and the friends row scroll while their
        // text overflows. The marquee moves at most once per
        // MARQUEE_STEP_TICKS and every transition lands on a multiple of it,
        // so only those boundary ticks need a frame.
        if self.marquee_tick / crate::app::common::marquee::MARQUEE_STEP_TICKS
            != prev_marquee_tick / crate::app::common::marquee::MARQUEE_STEP_TICKS
            && sidebar_visible
        {
            let selected_icecast_stream = self.selected_icecast_stream;
            let icecast_now_playing = self.now_playing_rx.as_ref().and_then(|rx| {
                rx.borrow()
                    .get(selected_icecast_stream.as_str())
                    .cloned()
            });
            let selected_radio_station = self.selected_radio_station;
            let radio_now_playing = self.radio_meta_rx.as_ref().and_then(|rx| {
                rx.borrow()
                    .get(selected_radio_station.as_str())
                    .map(|meta| format!("{} - {}", meta.artist, meta.title))
            });
            let queue = self.audio.queue_snapshot();
            let inputs = crate::app::common::sidebar::SidebarMarqueeInputs {
                components: &self.profile_state.profile().right_sidebar_components,
                active_friend_names: &self.active_friend_names,
                icecast_now_playing: icecast_now_playing.as_ref(),
                radio_now_playing: radio_now_playing.as_deref(),
                selected_station: selected_radio_station,
                source: self.paired_browser_source,
                queue: Some(&queue),
            };
            changed |= crate::app::common::sidebar::sidebar_marquee_scrolling(&inputs);
        }
        // Now-playing metadata changes repaint even between marquee steps.
        changed |= self
            .now_playing_rx
            .as_ref()
            .is_some_and(|rx| rx.has_changed().unwrap_or(false));
        changed |= self
            .radio_meta_rx
            .as_ref()
            .is_some_and(|rx| rx.has_changed().unwrap_or(false));

        // Expired banners need one final frame to clear, then stay quiet.
        if self.banner.as_ref().is_some_and(|banner| !banner.is_active()) {
            self.banner = None;
            changed = true;
        }

        // Most overlays are static between input and the async results their
        // tick paths already report (settings, hub, profile, poll, icon
        // picker, booth, room search, bonsai modals). The remaining coarse
        // spots each carry a reason:
        // - The lobby modal reads live table occupancy from the registry at
        //   draw time; a 1Hz cadence keeps those counts moving.
        // - The ultimate modal's cooldown label is minute-granularity and
        //   rides the per-minute global frame; only the running -> ready
        //   flip pays a one-shot frame here.
        // - The profile modal ticks a live aquarium during draw whenever the
        //   viewed profile owns fish.
        // The image modal's Sixel fetch keys off the capacity recorded by
        // the draw that opened or resized the modal (both input-forced
        // frames), so requesting here needs no frames of its own; the
        // fetch completion reports through poll_terminal_images above.
        self.chat
            .request_image_modal_terminal_image(self.terminal_image_protocol);
        changed |= self.show_lobby_modal && one_hz;
        let ultimate_cooldown_running = self.ultimate_state.has_cooldown_running();
        changed |= self.show_ultimate_modal
            && self.ultimate_cooldown_was_running
            && !ultimate_cooldown_running;
        self.ultimate_cooldown_was_running = ultimate_cooldown_running;
        changed |= self.show_profile_modal && self.profile_modal_state.aquarium_animating();

        // Daily boards are event-driven (daily_tick, chat, input); the 1Hz
        // cadence keeps the move-deadline clock honest while on screen.
        changed |= self.screen == Screen::DailyMatch && one_hz;

        // Outputs that only ship during a render: queued terminal commands,
        // a pending OSC 52 clipboard write, and desktop notifications.
        changed |= !self.pending_terminal_commands.is_empty()
            || self.pending_clipboard.is_some()
            || self.notify_outbox.has_pending();

        // Anything that bumped a row-cache epoch or switched screens this
        // tick changed the frame, wherever it happened.
        changed |= self.chat.context_epoch() != chat_context_epoch_before;
        changed |= self.chat_ctx_epoch != chat_ctx_epoch_before;
        changed |= self.screen != screen_before;

        changed
    }

    /// How long the render loop should sleep before the next world tick.
    /// Three tiers, prove-clean's cadence twin: anything that might animate
    /// soon returns the hot tick, since an over-eager wake costs a cheap
    /// clean tick, never a frame. Input, resize, and push wakes
    /// (RenderSignal) interrupt the sleep regardless.
    pub fn wake_hint(&self) -> Duration {
        let hot = self.show_splash
            || self.last_input_at.elapsed() < POST_INPUT_HOT_WINDOW
            || self.ultimate_state.has_active_effect()
            || self.screen == Screen::HouseTable
            || (self.screen == Screen::Arcade && self.is_playing_game)
            || self.pet_state.roaming_active()
            || self.last_pet_strip_travel.get().is_some()
            || (self.show_aquarium_tray && self.shop_state.entitlements().has_aquarium())
            || (self.show_profile_modal && self.profile_modal_state.aquarium_animating())
            || self.show_bonsai_modal
            || self.show_bonsai_v2_modal
            || (self.visualizer.animating() && self.right_sidebar_visible());
        if hot {
            return HOT_TICK;
        }
        if self.screen == Screen::Clubhouse {
            return AMBIENT_TICK;
        }
        IDLE_TICK
    }

    /// Whether the right sidebar draws this frame (the settings draft
    /// previews the toggle live). Shared by the viz gate in tick() and the
    /// wake cadence.
    fn right_sidebar_visible(&self) -> bool {
        if self.show_settings {
            crate::app::render::resolve_right_sidebar_enabled(
                self.settings_modal_state.draft().right_sidebar_mode,
                self.screen,
            )
        } else {
            crate::app::render::resolve_right_sidebar_enabled(
                self.profile_state.profile().right_sidebar_mode,
                self.screen,
            )
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
