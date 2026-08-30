use late_core::models::article::NewsShareReward;
use late_core::models::chat_message_gild::GildTier;
use late_core::models::leaderboard::DoorGame;

use crate::app::activity::event::ActivityGame;
use crate::app::chat::svc::GildRefusal;
use crate::app::crown::svc::CrownRefusal;
use crate::app::games::chips::svc::RoundRefusal;
use crate::app::lobby::daily::svc::DailyWinPayout;
use crate::app::pot::svc::PotRefusal;

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

/// How a chat-translation request resolved. `Translated` and `SameLanguage`
/// are the variants that spent an API call; the others are the cache and the
/// guardrails doing their job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationResult {
    CacheHit,
    Translated,
    /// The model judged the message already in the target language; cached,
    /// renders as nothing.
    SameLanguage,
    Failed,
    CapExhausted,
    Stale,
}

/// How a `/summary` catch-up request resolved. `Summarized` is the variant
/// that spent an API call; the rest are the guardrails and the empty case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummaryResult {
    Summarized,
    /// Nothing in the window to summarize; no call spent.
    Empty,
    /// Collapsed into a request already running for the same user and room.
    InFlight,
    Cooldown,
    CapExhausted,
    /// AI is disabled or unconfigured for this deployment.
    Unavailable,
    Failed,
}

/// How a five-minute online-time flush resolved. `Failed` means the batch is
/// retained in memory for retry; a sustained run of failures is accruing time
/// that dies with the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnlineTimeFlushResult {
    Flushed,
    Failed,
}

#[cfg(feature = "otel")]
mod inner {
    use std::sync::OnceLock;

    use opentelemetry::{
        KeyValue, global,
        metrics::{Counter, UpDownCounter},
    };

    use super::{
        ActivityGame, CrownRefusal, DailyWinPayout, DoorGame, GildRefusal, GildTier,
        NewsShareReward, OnlineTimeFlushResult, PotRefusal, RenderReason, RoundRefusal,
        SummaryResult, TranslationResult,
    };

    fn meter() -> opentelemetry::metrics::Meter {
        global::meter("late-ssh")
    }

    fn render_reason_label(reason: RenderReason) -> &'static str {
        match reason {
            RenderReason::Input => "input",
            RenderReason::WorldTick => "tick",
        }
    }

    fn game_label(game: ActivityGame) -> &'static str {
        match game {
            ActivityGame::Asterion => "asterion",
            ActivityGame::Blackjack => "blackjack",
            ActivityGame::Brogue => "brogue",
            ActivityGame::Chess => "chess",
            ActivityGame::Darkroom => "darkroom",
            ActivityGame::Dcss => "dcss",
            ActivityGame::GreenDragon => "greendragon",
            ActivityGame::LeWord => "le_word",
            ActivityGame::Minesweeper => "minesweeper",
            ActivityGame::Mud => "mud",
            ActivityGame::Nethack => "nethack",
            ActivityGame::Nonogram => "nonogram",
            ActivityGame::Poker => "poker",
            ActivityGame::RubiksCube => "rubiks_cube",
            ActivityGame::SlidingPuzzle => "sliding_puzzle",
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
                .with_description("Successful CLI/webview websocket pair connections")
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

    fn renders_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_renders_total")
                .with_description("Frames actually drawn, by render loop wake reason")
                .build()
        })
    }

    fn renders_skipped_clean_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_renders_skipped_clean_total")
                .with_description(
                    "Render passes skipped because neither input nor the world tick changed visible state",
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

    fn gild_tier_label(tier: GildTier) -> &'static str {
        match tier {
            GildTier::Bronze => "bronze",
            GildTier::Silver => "silver",
            GildTier::Gold => "gold",
        }
    }

    fn gild_refusal_label(refusal: GildRefusal) -> &'static str {
        match refusal {
            GildRefusal::MessageNotFound => "message_not_found",
            GildRefusal::NotAMember => "not_a_member",
            GildRefusal::NotPublic => "not_public",
            GildRefusal::GameRoom => "game_room",
            GildRefusal::SelfGild => "self_gild",
            GildRefusal::BotAuthor => "bot_author",
            GildRefusal::OnCooldown => "on_cooldown",
            GildRefusal::AlreadyGilded => "already_gilded",
            GildRefusal::HeldHigher => "held_higher",
            GildRefusal::InsufficientChips => "insufficient_chips",
        }
    }

    fn crown_refusal_label(refusal: CrownRefusal) -> &'static str {
        match refusal {
            CrownRefusal::AlreadyYours => "already_yours",
            CrownRefusal::InsufficientChips { .. } => "insufficient_chips",
        }
    }

    fn crown_takes_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_crown_takes_total")
                .with_description("Crown takeovers that settled")
                .build()
        })
    }

    fn crown_chips_burned_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_crown_chips_burned_total")
                .with_description("Chips destroyed by crown takeovers (the whole price)")
                .build()
        })
    }

    fn crown_takes_refused_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_crown_takes_refused_total")
                .with_description("Crown takeovers refused, by reason (none were charged)")
                .build()
        })
    }

    fn round_refusal_label(refusal: RoundRefusal) -> &'static str {
        match refusal {
            RoundRefusal::EmptyHouse => "empty_house",
            RoundRefusal::AllHolding => "all_holding",
            RoundRefusal::InsufficientChips { .. } => "insufficient_chips",
        }
    }

    fn rounds_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_rounds_total")
                .with_description("Rounds bought for the house that settled")
                .build()
        })
    }

    fn round_drinks_granted_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_round_drinks_granted_total")
                .with_description("Drink credits handed out by rounds")
                .build()
        })
    }

    fn round_drinks_cashed_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_round_drinks_cashed_total")
                .with_description(
                    "Round credits actually drunk (the gap against granted is what expired)",
                )
                .build()
        })
    }

    fn round_chips_burned_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_round_chips_burned_total")
                .with_description("Chips destroyed by rounds (the whole price)")
                .build()
        })
    }

    fn rounds_refused_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_rounds_refused_total")
                .with_description("Rounds refused, by reason (none were charged)")
                .build()
        })
    }

    fn pot_refusal_label(refusal: PotRefusal) -> &'static str {
        match refusal {
            PotRefusal::Closed => "closed",
            PotRefusal::CapReached { .. } => "cap_reached",
            PotRefusal::InsufficientChips { .. } => "insufficient_chips",
        }
    }

    fn pot_tickets_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_tickets_total")
                .with_description("Pot tickets bought")
                .build()
        })
    }

    fn pot_chips_in_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_chips_in_total")
                .with_description("Chips paid into pots for tickets")
                .build()
        })
    }

    fn pot_buys_refused_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_buys_refused_total")
                .with_description("Pot ticket buys refused, by reason (none were charged)")
                .build()
        })
    }

    fn pot_draws_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_draws_total")
                .with_description("Pots drawn with a winner")
                .build()
        })
    }

    fn pot_tickets_drawn_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_tickets_drawn_total")
                .with_description("Tickets in the field at each pot draw")
                .build()
        })
    }

    fn pot_chips_out_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_chips_out_total")
                .with_description("Chips paid out by pot draws; the gap to chips_in is the burn")
                .build()
        })
    }

    fn chat_gilds_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_gilds_total")
                .with_description("Chat message gilds bought, by tier")
                .build()
        })
    }

    fn chat_gilds_refused_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_gilds_refused_total")
                .with_description("Chat message gilds refused, by reason (none were charged)")
                .build()
        })
    }

    fn daily_win_payouts_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_daily_win_payouts_total")
                .with_description(
                    "Daily correspondence match wins by what the chips did (paid, or refused by a lobby gate)",
                )
                .build()
        })
    }

    fn news_shares_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_news_shares_total")
                .with_description("News articles published, from the composer or an RSS share")
                .build()
        })
    }

    fn news_share_chips_paid_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_news_share_chips_paid_total")
                .with_description("Chips minted as News share rewards")
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

    pub fn record_render(reason: RenderReason) {
        renders_total().add(1, &[KeyValue::new("reason", render_reason_label(reason))]);
    }

    pub fn record_render_skipped_clean() {
        renders_skipped_clean_total().add(1, &[]);
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

    fn daily_win_payout_label(payout: DailyWinPayout) -> &'static str {
        match payout {
            DailyWinPayout::Paid => "paid",
            DailyWinPayout::Unplayed => "unplayed",
            DailyWinPayout::PairDayCapped => "pair_day_capped",
            DailyWinPayout::Failed => "failed",
        }
    }

    pub fn record_daily_win_payout(payout: DailyWinPayout) {
        daily_win_payouts_total().add(
            1,
            &[KeyValue::new("outcome", daily_win_payout_label(payout))],
        );
    }

    /// A share pays a flat reward, so one counter tracks the shares and
    /// another the chips they minted; the two together are the sink-free
    /// half of the News economy.
    fn news_share_reward_label(reward: NewsShareReward) -> &'static str {
        match reward {
            NewsShareReward::Paid => "paid",
            NewsShareReward::RepeatUrl => "repeat_url",
            NewsShareReward::DailyCapReached => "daily_cap",
        }
    }

    pub fn record_news_shared(reward: NewsShareReward) {
        news_shares_total().add(
            1,
            &[KeyValue::new("reward", news_share_reward_label(reward))],
        );
        news_share_chips_paid_total().add(reward.chips() as u64, &[]);
    }

    pub fn record_gild_bought(tier: GildTier) {
        chat_gilds_total().add(1, &[KeyValue::new("tier", gild_tier_label(tier))]);
    }

    pub fn record_gild_refused(refusal: GildRefusal) {
        chat_gilds_refused_total().add(1, &[KeyValue::new("reason", gild_refusal_label(refusal))]);
    }

    /// The price is burned whole, so one counter tracks the takeovers and
    /// another the chips they removed from the supply.
    pub fn record_crown_taken(price: i64) {
        crown_takes_total().add(1, &[]);
        crown_chips_burned_total().add(price.max(0) as u64, &[]);
    }

    pub fn record_crown_take_refused(refusal: CrownRefusal) {
        crown_takes_refused_total()
            .add(1, &[KeyValue::new("reason", crown_refusal_label(refusal))]);
    }

    /// A settled round. The price is burned whole like the crown's, and the
    /// drinks are counted separately from the rounds because the interesting
    /// number is how many of them ever get drunk.
    pub fn record_round_bought(patrons: i64, chips: i64) {
        rounds_total().add(1, &[]);
        round_drinks_granted_total().add(patrons.max(0) as u64, &[]);
        round_chips_burned_total().add(chips.max(0) as u64, &[]);
    }

    pub fn record_round_refused(refusal: RoundRefusal) {
        rounds_refused_total().add(1, &[KeyValue::new("reason", round_refusal_label(refusal))]);
    }

    /// A patron walked up and drank a credit somebody else paid for.
    pub fn record_round_drink_cashed() {
        round_drinks_cashed_total().add(1, &[]);
    }

    /// A settled buy. Two counters, because the burn is only visible as the
    /// gap between what went in and what came out.
    pub fn record_pot_tickets_bought(tickets: i64, chips: i64) {
        pot_tickets_total().add(tickets.max(0) as u64, &[]);
        pot_chips_in_total().add(chips.max(0) as u64, &[]);
    }

    pub fn record_pot_buy_refused(refusal: PotRefusal) {
        pot_buys_refused_total().add(1, &[KeyValue::new("reason", pot_refusal_label(refusal))]);
    }

    /// A settled draw with a winner. A pot that rolled empty records nothing:
    /// no chips moved. The ticket count rides its own counter rather than a
    /// label, since a per-draw number would be unbounded cardinality.
    pub fn record_pot_drawn(payout: i64, tickets: i64) {
        pot_draws_total().add(1, &[]);
        pot_tickets_drawn_total().add(tickets.max(0) as u64, &[]);
        pot_chips_out_total().add(payout.max(0) as u64, &[]);
    }

    fn translation_result_label(result: TranslationResult) -> &'static str {
        match result {
            TranslationResult::CacheHit => "cache_hit",
            TranslationResult::Translated => "translated",
            TranslationResult::SameLanguage => "same_language",
            TranslationResult::Failed => "failed",
            TranslationResult::CapExhausted => "cap_exhausted",
            TranslationResult::Stale => "stale",
        }
    }

    fn chat_translations_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_translations_total")
                .with_description("Chat message translation requests by resolution")
                .build()
        })
    }

    pub fn record_chat_translation(result: TranslationResult) {
        chat_translations_total().add(
            1,
            &[KeyValue::new("result", translation_result_label(result))],
        );
    }

    fn summary_result_label(result: SummaryResult) -> &'static str {
        match result {
            SummaryResult::Summarized => "summarized",
            SummaryResult::Empty => "empty",
            SummaryResult::InFlight => "in_flight",
            SummaryResult::Cooldown => "cooldown",
            SummaryResult::CapExhausted => "cap_exhausted",
            SummaryResult::Unavailable => "unavailable",
            SummaryResult::Failed => "failed",
        }
    }

    fn chat_summaries_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_chat_summaries_total")
                .with_description("Chat /summary catch-up requests by resolution")
                .build()
        })
    }

    pub fn record_chat_summary(result: SummaryResult) {
        chat_summaries_total().add(1, &[KeyValue::new("result", summary_result_label(result))]);
    }

    fn door_ingest_lines_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_door_ingest_lines_total")
                .with_description(
                    "Door host log lines handled by the ingest pipe (cursor advanced) by game",
                )
                .build()
        })
    }

    fn door_ingest_session_failures_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_door_ingest_session_failures_total")
                .with_description(
                    "Door stats-session failures (connect or mid-stream) before a retry, by game",
                )
                .build()
        })
    }

    pub fn record_door_ingest_line(game: DoorGame) {
        // `DoorGame::key` is the closed roster's own exhaustive label map.
        door_ingest_lines_total().add(1, &[KeyValue::new("game", game.key())]);
    }

    pub fn record_door_ingest_session_failure(game: DoorGame) {
        door_ingest_session_failures_total().add(1, &[KeyValue::new("game", game.key())]);
    }

    fn online_time_flush_result_label(result: OnlineTimeFlushResult) -> &'static str {
        match result {
            OnlineTimeFlushResult::Flushed => "flushed",
            OnlineTimeFlushResult::Failed => "failed",
        }
    }

    fn online_time_flushes_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_online_time_flushes_total")
                .with_description(
                    "Online-time flush passes by result; a failed pass retains its batch in memory for retry",
                )
                .build()
        })
    }

    pub fn record_online_time_flush(result: OnlineTimeFlushResult) {
        online_time_flushes_total().add(
            1,
            &[KeyValue::new(
                "result",
                online_time_flush_result_label(result),
            )],
        );
    }
}

#[cfg(not(feature = "otel"))]
mod inner {
    use super::{
        ActivityGame, CrownRefusal, DailyWinPayout, DoorGame, GildRefusal, GildTier,
        NewsShareReward, OnlineTimeFlushResult, PotRefusal, RenderReason, RoundRefusal,
        SummaryResult, TranslationResult,
    };

    pub fn record_ssh_connection() {}
    pub fn record_render(_reason: RenderReason) {}
    pub fn record_render_skipped_clean() {}
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
    pub fn record_daily_win_payout(_payout: DailyWinPayout) {}
    pub fn record_news_shared(_reward: NewsShareReward) {}
    pub fn record_gild_bought(_tier: GildTier) {}
    pub fn record_gild_refused(_refusal: GildRefusal) {}
    pub fn record_crown_taken(_price: i64) {}
    pub fn record_crown_take_refused(_refusal: CrownRefusal) {}
    pub fn record_round_bought(_patrons: i64, _chips: i64) {}
    pub fn record_round_refused(_refusal: RoundRefusal) {}
    pub fn record_round_drink_cashed() {}
    pub fn record_pot_tickets_bought(_tickets: i64, _chips: i64) {}
    pub fn record_pot_buy_refused(_refusal: PotRefusal) {}
    pub fn record_pot_drawn(_payout: i64, _tickets: i64) {}
    pub fn record_chat_translation(_result: TranslationResult) {}
    pub fn record_chat_summary(_result: SummaryResult) {}
    pub fn record_door_ingest_line(_game: DoorGame) {}
    pub fn record_door_ingest_session_failure(_game: DoorGame) {}
    pub fn record_online_time_flush(_result: OnlineTimeFlushResult) {}
}

pub use inner::*;
