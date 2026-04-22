use std::time::Instant;

use late_core::audio::VizFrame;

use super::state::App;
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

        // Services
        if let Some(b) = self.chat.tick() {
            self.banner = Some(b);
        }
        if self.chat.pending_chat_screen_switch {
            self.chat.pending_chat_screen_switch = false;
            self.screen = Screen::Chat;
        }
        if let Some(b) = self.vote.tick() {
            self.banner = Some(b);
        }
        // News state is ticked inside chat.tick()
        if let Some(b) = self.profile_state.tick() {
            self.banner = Some(b);
        }
        if self.show_profile_modal {
            self.profile_modal_state.tick();
        }
        if self.show_settings
            && self.settings_modal_state.draft().username.is_empty()
            && !self.profile_state.profile().username.is_empty()
        {
            self.settings_modal_state.open_from_profile(
                self.profile_state.profile(),
                self.chat.favorite_room_options(),
                crate::app::settings_modal::ui::MODAL_WIDTH,
            );
        }

        let mut updated = false;
        for msg in messages {
            match msg {
                SessionMessage::Heartbeat => {}
                SessionMessage::Viz(viz) => {
                    self.push_browser_frame(viz);
                    updated = true;
                }
            }
        }

        if self.screen == Screen::Games && self.is_playing_game && self.game_selection == 1 {
            self.tetris_state.tick();
        }
        self.blackjack_state.tick();
        self.chip_balance = self.blackjack_state.balance;

        // Leaderboard
        if let Some(rx) = &mut self.leaderboard_rx
            && rx.has_changed().unwrap_or(false)
        {
            self.leaderboard = rx.borrow_and_update().clone();
            if let Some(&balance) = self.leaderboard.user_chips.get(&self.user_id)
                && self.blackjack_state.snapshot.phase
                    == crate::app::games::blackjack::state::Phase::Betting
            {
                self.chip_balance = balance;
                self.blackjack_state.balance = balance;
            }
        }

        // Bonsai passive growth
        self.bonsai_state.tick();

        if let Some(rx) = &mut self.activity_feed_rx {
            while let Ok(event) = rx.try_recv() {
                self.activity.push_back(event);
                if self.activity.len() > 7 {
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
