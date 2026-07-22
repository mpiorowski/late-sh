use crate::app::activity::event::ActivityGame;

/// Why the render loop drew a frame. The loop can only distinguish its two
/// wake sources; event-driven renders currently ride the world tick, so they
/// count as `WorldTick` until the loop becomes event-driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderReason {
    /// Keystroke or resize, rendered without advancing world time.
    Input,
    /// The 66ms world tick, which advanced animations first.
    WorldTick,
}

#[cfg(feature = "otel")]
mod inner {
    use std::sync::OnceLock;

    use opentelemetry::{
        KeyValue, global,
        metrics::{Counter, UpDownCounter},
    };

    use super::{ActivityGame, RenderReason};

    fn meter() -> opentelemetry::metrics::Meter {
        global::meter("late-ssh")
    }

    fn game_label(game: ActivityGame) -> &'static str {
        match game {
            ActivityGame::Asterion => "asterion",
            ActivityGame::Blackjack => "blackjack",
            ActivityGame::Chess => "chess",
            ActivityGame::GreenDragon => "greendragon",
            ActivityGame::LeWord => "le_word",
            ActivityGame::Minesweeper => "minesweeper",
            ActivityGame::Mud => "mud",
            ActivityGame::Nethack => "nethack",
            ActivityGame::Nonogram => "nonogram",
            ActivityGame::Poker => "poker",
            ActivityGame::RubiksCube => "rubiks_cube",
            ActivityGame::Sshattrick => "sshattrick",
            ActivityGame::Ssnake => "ssnake",
            ActivityGame::Solitaire => "solitaire",
            ActivityGame::Sudoku => "sudoku",
            ActivityGame::TicTacToe => "tictactoe",
            ActivityGame::Lateris => "tetris",
            ActivityGame::TwentyFortyEight => "2048",
            ActivityGame::Tron => "tron",
            ActivityGame::Snake => "snake",
            ActivityGame::Traffic => "traffic",
        }
    }

    fn ssh_connections_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_connections_total")
                .with_description("Total inbound SSH connections accepted by the server")
                .build()
        })
    }

    fn ssh_sessions_active() -> &'static UpDownCounter<i64> {
        static METRIC: OnceLock<UpDownCounter<i64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .i64_up_down_counter("late_ssh_sessions_active")
                .with_description("Current number of authenticated active SSH sessions")
                .build()
        })
    }

    fn ws_pair_success_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_ws_pair_success_total")
                .with_description("Successful browser websocket pair connections")
                .build()
        })
    }

    fn ws_pair_rejected_unknown_token_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_ws_pair_rejected_unknown_token_total")
                .with_description(
                    "Websocket pair attempts rejected because no live session owned the token",
                )
                .build()
        })
    }

    fn cli_pair_usage_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_cli_pair_usage_total")
                .with_description("Total CLI pair sessions by SSH mode and client platform")
                .build()
        })
    }

    fn cli_pair_active() -> &'static UpDownCounter<i64> {
        static METRIC: OnceLock<UpDownCounter<i64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .i64_up_down_counter("late_ssh_cli_pair_active")
                .with_description(
                    "Current active CLI pair sessions by SSH mode and client platform",
                )
                .build()
        })
    }

    fn render_frame_drops_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_render_frame_drops_total")
                .with_description("Frames dropped because the SSH channel was busy")
                .build()
        })
    }

    fn render_stall_skips_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_render_stall_skips_total")
                .with_description(
                    "Render passes skipped because a session's unacked SSH output exceeded the budget",
                )
                .build()
        })
    }

    fn render_stall_disconnects_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_render_stall_disconnects_total")
                .with_description(
                    "Sessions disconnected after staying over the SSH output budget too long",
                )
                .build()
        })
    }

    fn chat_messages_sent_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_messages_sent_total")
                .with_description("Chat messages successfully sent")
                .build()
        })
    }

    fn chat_messages_edited_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_messages_edited_total")
                .with_description("Chat messages successfully edited")
                .build()
        })
    }

    fn game_wins_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_game_wins_total")
                .with_description("Games won by game name")
                .build()
        })
    }

    pub fn record_ssh_connection() {
        ssh_connections_total().add(1, &[]);
    }

    pub fn add_ssh_session(delta: i64) {
        ssh_sessions_active().add(delta, &[]);
    }

    pub fn record_ws_pair_success() {
        ws_pair_success_total().add(1, &[]);
    }

    pub fn record_ws_pair_rejected_unknown_token() {
        ws_pair_rejected_unknown_token_total().add(1, &[]);
    }

    pub fn record_cli_pair_usage(ssh_mode: &str, platform: &str) {
        cli_pair_usage_total().add(
            1,
            &[
                KeyValue::new("ssh_mode", ssh_mode.to_string()),
                KeyValue::new("platform", platform.to_string()),
            ],
        );
    }

    pub fn add_cli_pair_active(delta: i64, ssh_mode: &str, platform: &str) {
        cli_pair_active().add(
            delta,
            &[
                KeyValue::new("ssh_mode", ssh_mode.to_string()),
                KeyValue::new("platform", platform.to_string()),
            ],
        );
    }

    pub fn record_render_frame_drop() {
        render_frame_drops_total().add(1, &[]);
    }

    pub fn record_render_stall_skip() {
        render_stall_skips_total().add(1, &[]);
    }

    pub fn record_render_stall_disconnect() {
        render_stall_disconnects_total().add(1, &[]);
    }

    pub fn record_chat_message_sent() {
        chat_messages_sent_total().add(1, &[]);
    }

    pub fn record_chat_message_edited() {
        chat_messages_edited_total().add(1, &[]);
    }

    pub fn record_game_win(game: ActivityGame) {
        game_wins_total().add(1, &[KeyValue::new("game", game_label(game))]);
    }
}

#[cfg(not(feature = "otel"))]
mod inner {
    use super::ActivityGame;

    pub fn record_ssh_connection() {}
    pub fn add_ssh_session(_delta: i64) {}
    pub fn record_ws_pair_success() {}
    pub fn record_ws_pair_rejected_unknown_token() {}
    pub fn record_cli_pair_usage(_ssh_mode: &str, _platform: &str) {}
    pub fn add_cli_pair_active(_delta: i64, _ssh_mode: &str, _platform: &str) {}
    pub fn record_render_frame_drop() {}
    pub fn record_render_stall_skip() {}
    pub fn record_render_stall_disconnect() {}
    pub fn record_chat_message_sent() {}
    pub fn record_chat_message_edited() {}
    pub fn record_game_win(_game: ActivityGame) {}
}

pub use inner::*;
