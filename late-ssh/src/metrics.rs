use late_core::models::article::NewsShareReward;
use late_core::models::chat_message_gild::GildTier;
use late_core::models::leaderboard::{DailyPuzzle, DoorGame};
use late_core::models::media_queue_item::SongQueueReward;

use crate::app::activity::event::ActivityGame;
use crate::app::arcade::share::ShareCardKind;
use crate::app::arcade::sliding_puzzle::svc::SlidingPuzzleArtLoad;
use crate::app::bonsai::state::BonsaiAction;
use crate::app::bonsai::svc::BonsaiActionResult;
use crate::app::chat::news::svc::XMediaLookup;
use crate::app::chat::svc::GildRefusal;
use crate::app::clubhouse::nightcap::svc::{NightcapHouseFailure, NightcapOrderResult};
use crate::app::common::primitives::Screen;
use crate::app::crown::svc::CrownRefusal;
use crate::app::deadchannel::haunt::state::GateVerdict;
use crate::app::games::chips::svc::RoundRefusal;
use crate::app::lobby::daily::svc::{DailyWinPayout, PoolShotOutcome};
use crate::app::pot::svc::{PotRefusal, PotReminderOutcome};
use crate::pg_listener::Refresh;

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

/// How one page of The Late Edition (`app/paper`) came off the press.
/// `Printed` is the variant that spent a model call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperPrintResult {
    Printed,
    /// Under the message threshold, or nothing to write about; no call.
    Quiet,
    /// Another replica held the claim; no call.
    Lost,
    Failed,
}

/// How a request to open the paper resolved. `Login` and `Command` are the
/// two ways a reader got it; the rest are why they did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaperOpenResult {
    Login,
    Command,
    /// Nothing printed for today's edition.
    Empty,
    /// This account's login pop for the edition was already claimed.
    AlreadyShown,
    /// The paper's kill switch is off, or AI is unconfigured here.
    Unavailable,
    Failed,
}

/// How one source's fetch in the nightly job press (`app/jobs`) went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsFetchResult {
    Fetched,
    Failed,
    /// One item of a source (an HN comment, a Jobicy tag) that failed and
    /// was skipped; the rest of the source still lands.
    ItemSkipped,
}

/// How one posting's model read ended. Every variant but `Failed` settles
/// the row; `Failed` keeps it pending for the next night.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsReadResult {
    Queued,
    Active,
    Dropped,
    Dead,
    Failed,
}

/// How a replica's check of the day's press run ended. `Ran` is the one
/// that spent the calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsPressResult {
    Ran,
    /// Another replica holds or held the day's claim; nothing spent.
    Lost,
    Failed,
}

/// How a person's own write on the Jobs shelf ended: a posting saved, one
/// taken down, refused at the per-person cap, or failed in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobsPostResult {
    Posted,
    Retracted,
    AtCap,
    Failed,
}

/// How a hang attempt on the Artboard gallery ended. `Hung` is the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryHangResult {
    Hung,
    /// The account's pieces for the UTC day were already up.
    DailyCap,
    /// The same cells already hang this month.
    Duplicate,
    Failed,
}

/// How an applause toggle on a gallery piece ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryApplauseResult {
    Applauded,
    Withdrawn,
    OwnPiece,
    NotFound,
    Closed,
    Failed,
}

/// How a hanger's own take-down resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalleryTakeDownResult {
    TakenDown,
    NotFound,
    NotYours,
    Closed,
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

/// One rung of the first-contact ladder actually reaching a person (GAME.md,
/// First contact). The two hit beats count won DB claims, never local dice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstContactBeat {
    GlitchBurst,
    NameFlicker,
    WhisperDelivered,
    /// The breakthrough played on a won invitation claim; the DM follows.
    Breakthrough,
    /// The invitation accepted: `/join #deadchannel` created the runner.
    RunnerCreated,
}

/// One fight command settling on the runner row (`deadchannel/fight`).
/// `Refused` is a command the row turned down (no rations, signal down,
/// the armorer's or patch's no); `Outfitted` is a piece bought at the
/// armorer; `Patched` is the signal bought back at patch; `Failed` is
/// the write not landing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FightBeat {
    Started,
    Resumed,
    Round,
    Won,
    /// The Old Signal put down: a mark and the reset.
    Slain,
    Lost,
    Escaped,
    Outfitted,
    Patched,
    Refused,
    Failed,
}

/// A look written at the tailor's mirror (`app/deadchannel/tailor`):
/// `Worn` landed, `NoRunner` found no standing runner to dress, `Failed`
/// is the write not landing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailorBeat {
    Worn,
    NoRunner,
    Failed,
}

/// A stage of a session's start, from TCP accept to the first frame on
/// the user's screen. `Total` is the whole span; the other three add up
/// to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStartStage {
    /// Accept to key accepted: SSH handshake plus the user lookup.
    Auth,
    /// Key accepted to the `App` built: channel and pty requests plus every
    /// preload in `pty_request`.
    Bootstrap,
    /// `App` built to the first frame written to the channel.
    FirstFrame,
    Total,
}

/// Whether the session's account was created by this connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionUser {
    New,
    Returning,
}

/// Which board of a daily puzzle ended: the shared daily or a personal one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcadeMode {
    Daily,
    Personal,
}

/// The difficulty of a finished Arcade board. `Single` is a game with one
/// board a day and no difficulty (Le Word, Rubik's Cube).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcadeDifficulty {
    Easy,
    Medium,
    Hard,
    DrawOne,
    DrawThree,
    Single,
}

/// How an Arcade board ended. Only Le Word (out of guesses) and
/// Minesweeper (out of lives) can be lost; the rest end solved or not at
/// all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcadeFinish {
    Won,
    Lost,
}

/// Whether a session's second of attention had a key press behind it
/// recently (`Active`) or is a terminal left open (`Idle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    Active,
    Idle,
}

/// How one attempt of a notify-driven re-read (`pg_listener::read_until_ok`)
/// ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshOutcome {
    Ok,
    Failed,
}

/// A runner going through the door after the ladder is done. Leaving keeps
/// the character (the row stays, `left_at` is stamped), so the two sides
/// are one counter: the gap between them is how many runners are standing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunnerDoor {
    /// `/leave #deadchannel`: the gate closed on every replica.
    Left,
    /// An invited rejoin brought a runner who had left back, same look.
    Returned,
}

/// How one bio screen (the first-contact eligibility gate's AI leg)
/// resolved. `Passed` and `Failed` are the verdicts that spent an API call
/// and landed; the rest are the reasons a claim produced no verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BioScreenOutcome {
    Passed,
    Failed,
    /// The model answered with nothing usable; the pending claim stays and
    /// is retried after the stale window.
    Unavailable,
    /// The call itself broke (network, API error).
    CallFailed,
    /// Another session on any replica already holds the claim.
    LostClaim,
    /// The bio changed while the call was in flight; the verdict was
    /// dropped and the new text gets its own screen.
    TextChanged,
}

/// Why an inbound SSH TCP connection was closed before the SSH handshake.
/// Every variant is a socket the accept loop dropped on the floor; none of
/// them ever reached russh, so they cost no permit, task, or buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SshRejectReason {
    /// The trusted proxy never delivered a PROXY protocol header in time.
    ProxyHeader,
    /// The per-IP attempt rate limiter refused the connection.
    RateLimited,
    /// The IP already holds `max_conns_per_ip` open connections.
    PerIpLimit,
    /// The global `max_conns_global` semaphore is exhausted.
    GlobalLimit,
}

/// The band count a paired CLI's `viz` frame arrived with. CLIs from before
/// the 16-band analyzer send 8, which the pair socket stretches to 16.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VizWireBands {
    Eight,
    Sixteen,
}

#[cfg(feature = "otel")]
mod inner {
    use std::sync::{Arc, OnceLock};

    use opentelemetry::{
        KeyValue, global,
        metrics::{Counter, Gauge, Histogram, ObservableGauge, UpDownCounter},
    };
    use tokio::sync::Semaphore;

    use crate::app::activity::event::GameFamily;

    use super::ShareCardKind;
    use super::SlidingPuzzleArtLoad;
    use super::XMediaLookup;
    use super::{
        ActivityGame, ArcadeDifficulty, ArcadeFinish, ArcadeMode, BioScreenOutcome, CrownRefusal,
        DailyPuzzle, DailyWinPayout, DoorGame, FightBeat, FirstContactBeat, GalleryApplauseResult,
        GalleryHangResult, GalleryTakeDownResult, GateVerdict, GildRefusal, GildTier,
        JobsFetchResult, JobsPostResult, JobsPressResult, JobsReadResult, NewsShareReward,
        NightcapHouseFailure, NightcapOrderResult, OnlineTimeFlushResult, PaperOpenResult,
        PaperPrintResult, PoolShotOutcome, PotRefusal, PotReminderOutcome, Presence, Refresh,
        RefreshOutcome, RenderReason, RoundRefusal, RunnerDoor, Screen, SessionStartStage,
        SessionUser, SongQueueReward, SshRejectReason, SummaryResult, TailorBeat,
        TranslationResult, VizWireBands,
    };
    use super::{BonsaiAction, BonsaiActionResult};
    use crate::app::bonsai::state::BranchAction;

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

    fn pair_viz_frames_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pair_viz_frames_total")
                .with_description("Spectrum frames accepted from paired CLIs, by wire band count")
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

    fn bonsai_action_label(action: BonsaiAction) -> &'static str {
        match action {
            BonsaiAction::Water => "water",
            BonsaiAction::Branch(BranchAction::Bend { .. }) => "bend",
            BonsaiAction::Branch(BranchAction::Prune) => "prune",
            BonsaiAction::Branch(BranchAction::Split) => "split",
            BonsaiAction::Branch(BranchAction::Pinch) => "pinch",
        }
    }

    fn bonsai_action_result_label(result: BonsaiActionResult) -> &'static str {
        match result {
            BonsaiActionResult::Stored => "stored",
            BonsaiActionResult::Refused => "refused",
            BonsaiActionResult::Failed => "failed",
        }
    }

    fn bonsai_actions_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_bonsai_actions_total")
                .with_description("Bonsai care actions, by action and how they settled")
                .build()
        })
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

    fn nightcap_order_label(result: NightcapOrderResult) -> &'static str {
        match result {
            NightcapOrderResult::Poured => "poured",
            NightcapOrderResult::Comped => "comped",
            NightcapOrderResult::Bounced => "bounced",
            NightcapOrderResult::Failed => "failed",
        }
    }

    fn nightcap_orders_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_nightcap_orders_total")
                .with_description("Single-drink orders at the Nightcap bar, by how they settled")
                .build()
        })
    }

    fn nightcap_house_failure_label(failure: NightcapHouseFailure) -> &'static str {
        match failure {
            NightcapHouseFailure::CreditCount => "credit_count",
            NightcapHouseFailure::HouseLine => "house_line",
        }
    }

    fn nightcap_house_failures_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_nightcap_house_failures_total")
                .with_description(
                    "Nightcap off-thread work that failed (a stale free-drink count, a house line never posted), by which",
                )
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

    fn pot_reminder_label(outcome: PotReminderOutcome) -> &'static str {
        match outcome {
            PotReminderOutcome::Posted => "posted",
            PotReminderOutcome::Failed => "failed",
        }
    }

    fn pot_reminders_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pot_reminders_total")
                .with_description(
                    "The pot's last call in #lounge, by outcome; a quiet week with no posted line is a reminder that never fired",
                )
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

    fn pool_shots_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_pool_shots_total")
                .with_description(
                    "Daily pool shots by how they ended. `truncated` is a physics bug reaching production: the simulator gave up at its guard rails and the half-played rack was still written as the match",
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

    fn news_x_media_lookups_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_news_x_media_lookups_total")
                .with_description(
                    "fxtwitter lookups behind X shares, the only NSFW gate on that path",
                )
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

    fn songs_queued_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_songs_queued_total")
                .with_description("YouTube tracks queued, from the booth, a URL, or history")
                .build()
        })
    }

    fn song_queue_chips_paid_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_song_queue_chips_paid_total")
                .with_description("Chips minted as jukebox submission rewards")
                .build()
        })
    }

    fn game_wins_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_game_wins_total")
                .with_description("Games won by game name and family (Lateania counts mob kills)")
                .build()
        })
    }

    fn pg_refresh_seconds() -> &'static Histogram<f64> {
        static METRIC: OnceLock<Histogram<f64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .f64_histogram("late_ssh_pg_refresh_seconds")
                .with_description(
                    "Notify-driven re-reads of shared state, one attempt each, by domain and outcome",
                )
                .with_unit("s")
                .with_boundaries(vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0])
                .build()
        })
    }

    fn session_start_seconds() -> &'static Histogram<f64> {
        static METRIC: OnceLock<Histogram<f64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .f64_histogram("late_ssh_session_start_seconds")
                .with_description(
                    "Time from TCP accept to the first frame on screen, by stage and new or returning user",
                )
                .with_unit("s")
                .with_boundaries(vec![
                    0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 3.0, 5.0, 10.0, 20.0, 30.0, 60.0,
                ])
                .build()
        })
    }

    fn arcade_finishes_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_arcade_finishes_total")
                .with_description(
                    "Arcade daily-puzzle boards ended on a session, by game, mode, difficulty and finish",
                )
                .build()
        })
    }

    fn attention_seconds_total() -> &'static Counter<f64> {
        static METRIC: OnceLock<Counter<f64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .f64_counter("late_ssh_attention_seconds_total")
                .with_description(
                    "Session seconds spent per screen (and Arcade game), split by recent input",
                )
                .with_unit("s")
                .build()
        })
    }

    fn first_contact_beats_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_first_contact_beats_total")
                .with_description("First-contact ladder beats delivered, by beat")
                .build()
        })
    }

    fn deadchannel_fights_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_deadchannel_fights_total")
                .with_description("deadchannel fight commands settled on the runner row, by beat")
                .build()
        })
    }

    fn deadchannel_tailor_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_deadchannel_tailor_total")
                .with_description("deadchannel looks written at the tailor's mirror, by beat")
                .build()
        })
    }

    fn runner_door_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_runner_door_total")
                .with_description("Runners leaving and returning to #deadchannel, by direction")
                .build()
        })
    }

    fn first_contact_bio_screens_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_first_contact_bio_screens_total")
                .with_description("First-contact bio screens, by outcome")
                .build()
        })
    }

    fn first_contact_gate_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_first_contact_gate_total")
                .with_description(
                    "First-contact eligibility gate evaluations at connect (one per session, not per person), by verdict and audience",
                )
                .build()
        })
    }

    fn first_contact_gate_verdict_label(verdict: GateVerdict) -> &'static str {
        match verdict {
            GateVerdict::Passed => "passed",
            GateVerdict::TooFewHours => "too_few_hours",
            GateVerdict::TooFewSettings => "too_few_settings",
            GateVerdict::BioTooShort => "bio_too_short",
            GateVerdict::BioAiOff => "bio_ai_off",
            GateVerdict::BioUnscreened => "bio_unscreened",
            GateVerdict::BioPending => "bio_pending",
            GateVerdict::BioFailed => "bio_failed",
        }
    }

    fn first_contact_beat_label(beat: FirstContactBeat) -> &'static str {
        match beat {
            FirstContactBeat::GlitchBurst => "glitch_burst",
            FirstContactBeat::NameFlicker => "name_flicker",
            FirstContactBeat::WhisperDelivered => "whisper_delivered",
            FirstContactBeat::Breakthrough => "breakthrough",
            FirstContactBeat::RunnerCreated => "runner_created",
        }
    }

    fn runner_door_label(door: RunnerDoor) -> &'static str {
        match door {
            RunnerDoor::Left => "left",
            RunnerDoor::Returned => "returned",
        }
    }

    fn bio_screen_outcome_label(outcome: BioScreenOutcome) -> &'static str {
        match outcome {
            BioScreenOutcome::Passed => "passed",
            BioScreenOutcome::Failed => "failed",
            BioScreenOutcome::Unavailable => "unavailable",
            BioScreenOutcome::CallFailed => "call_failed",
            BioScreenOutcome::LostClaim => "lost_claim",
            BioScreenOutcome::TextChanged => "text_changed",
        }
    }

    pub fn record_first_contact_beat(beat: FirstContactBeat) {
        first_contact_beats_total()
            .add(1, &[KeyValue::new("beat", first_contact_beat_label(beat))]);
    }

    pub fn record_runner_door(door: RunnerDoor) {
        runner_door_total().add(1, &[KeyValue::new("direction", runner_door_label(door))]);
    }

    fn fight_beat_label(beat: FightBeat) -> &'static str {
        match beat {
            FightBeat::Started => "started",
            FightBeat::Resumed => "resumed",
            FightBeat::Round => "round",
            FightBeat::Won => "won",
            FightBeat::Slain => "slain",
            FightBeat::Lost => "lost",
            FightBeat::Escaped => "escaped",
            FightBeat::Outfitted => "outfitted",
            FightBeat::Patched => "patched",
            FightBeat::Refused => "refused",
            FightBeat::Failed => "failed",
        }
    }

    fn tailor_beat_label(beat: TailorBeat) -> &'static str {
        match beat {
            TailorBeat::Worn => "worn",
            TailorBeat::NoRunner => "no_runner",
            TailorBeat::Failed => "failed",
        }
    }

    pub fn record_deadchannel_tailor(beat: TailorBeat) {
        deadchannel_tailor_total().add(1, &[KeyValue::new("beat", tailor_beat_label(beat))]);
    }

    pub fn record_deadchannel_fight(beat: FightBeat) {
        deadchannel_fights_total().add(1, &[KeyValue::new("beat", fight_beat_label(beat))]);
    }

    pub fn record_first_contact_bio_screen(outcome: BioScreenOutcome) {
        first_contact_bio_screens_total().add(
            1,
            &[KeyValue::new("outcome", bio_screen_outcome_label(outcome))],
        );
    }

    /// One gate evaluation at connect. `staff` splits admins and
    /// moderators (haunted while the fuse is unlit) from everyone else.
    pub fn record_first_contact_gate(verdict: GateVerdict, staff: bool) {
        let audience = if staff { "staff" } else { "public" };
        first_contact_gate_total().add(
            1,
            &[
                KeyValue::new("verdict", first_contact_gate_verdict_label(verdict)),
                KeyValue::new("audience", audience),
            ],
        );
    }

    fn ssh_connections_rejected_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_connections_rejected_total")
                .with_description(
                    "Inbound SSH TCP connections closed before the handshake, by reason",
                )
                .build()
        })
    }

    fn ssh_reject_reason_label(reason: SshRejectReason) -> &'static str {
        match reason {
            SshRejectReason::ProxyHeader => "proxy_header",
            SshRejectReason::RateLimited => "rate_limited",
            SshRejectReason::PerIpLimit => "per_ip_limit",
            SshRejectReason::GlobalLimit => "global_limit",
        }
    }

    pub fn record_ssh_connection_rejected(reason: SshRejectReason) {
        ssh_connections_rejected_total().add(
            1,
            &[KeyValue::new("reason", ssh_reject_reason_label(reason))],
        );
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

    fn viz_wire_bands_label(bands: VizWireBands) -> &'static str {
        match bands {
            VizWireBands::Eight => "8",
            VizWireBands::Sixteen => "16",
        }
    }

    pub fn record_pair_viz_frame(bands: VizWireBands) {
        pair_viz_frames_total().add(1, &[KeyValue::new("bands", viz_wire_bands_label(bands))]);
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

    fn game_family_label(family: GameFamily) -> &'static str {
        match family {
            GameFamily::ArcadeDaily => "arcade_daily",
            GameFamily::ArcadeScore => "arcade_score",
            GameFamily::Door => "door",
            GameFamily::Lateania => "lateania",
            GameFamily::Table => "table",
            GameFamily::Match => "match",
        }
    }

    pub fn record_game_win(game: ActivityGame) {
        game_wins_total().add(
            1,
            &[
                KeyValue::new("game", game_label(game)),
                KeyValue::new("family", game_family_label(game.family())),
            ],
        );
    }

    fn session_start_stage_label(stage: SessionStartStage) -> &'static str {
        match stage {
            SessionStartStage::Auth => "auth",
            SessionStartStage::Bootstrap => "bootstrap",
            SessionStartStage::FirstFrame => "first_frame",
            SessionStartStage::Total => "total",
        }
    }

    fn session_user_label(user: SessionUser) -> &'static str {
        match user {
            SessionUser::New => "new",
            SessionUser::Returning => "returning",
        }
    }

    pub fn record_session_start(stage: SessionStartStage, user: SessionUser, seconds: f64) {
        session_start_seconds().record(
            seconds,
            &[
                KeyValue::new("stage", session_start_stage_label(stage)),
                KeyValue::new("user", session_user_label(user)),
            ],
        );
    }

    fn refresh_label(refresh: Refresh) -> &'static str {
        match refresh {
            Refresh::AppFlags => "app_flags",
            Refresh::RunnerLooks => "runner_looks",
            Refresh::CrownHolder => "crown_holder",
            Refresh::Pot => "pot",
            Refresh::Articles => "articles",
            Refresh::ActiveQuestBoards => "active_quest_boards",
            Refresh::ShopFlairDirectory => "shop_flair_directory",
        }
    }

    fn refresh_outcome_label(outcome: RefreshOutcome) -> &'static str {
        match outcome {
            RefreshOutcome::Ok => "ok",
            RefreshOutcome::Failed => "failed",
        }
    }

    pub fn record_pg_refresh(refresh: Refresh, outcome: RefreshOutcome, seconds: f64) {
        pg_refresh_seconds().record(
            seconds,
            &[
                KeyValue::new("domain", refresh_label(refresh)),
                KeyValue::new("outcome", refresh_outcome_label(outcome)),
            ],
        );
    }

    /// Report how many of chat's read permits are held on every metric
    /// export. At `total` every room-tail and discover load queues behind
    /// the semaphore. Call once per process.
    pub fn observe_chat_read_permits(permits: Arc<Semaphore>, total: usize) {
        static GAUGE: OnceLock<ObservableGauge<u64>> = OnceLock::new();
        GAUGE.get_or_init(|| {
            meter()
                .u64_observable_gauge("late_ssh_chat_read_permits_in_use")
                .with_description("Chat read permits held, out of the semaphore's fixed total")
                .with_callback(move |observer| {
                    let held = total.saturating_sub(permits.available_permits()) as u64;
                    observer.observe(held, &[]);
                })
                .build()
        });
    }

    fn arcade_mode_label(mode: ArcadeMode) -> &'static str {
        match mode {
            ArcadeMode::Daily => "daily",
            ArcadeMode::Personal => "personal",
        }
    }

    fn arcade_difficulty_label(difficulty: ArcadeDifficulty) -> &'static str {
        match difficulty {
            ArcadeDifficulty::Easy => "easy",
            ArcadeDifficulty::Medium => "medium",
            ArcadeDifficulty::Hard => "hard",
            ArcadeDifficulty::DrawOne => "draw_1",
            ArcadeDifficulty::DrawThree => "draw_3",
            ArcadeDifficulty::Single => "single",
        }
    }

    fn arcade_finish_label(finish: ArcadeFinish) -> &'static str {
        match finish {
            ArcadeFinish::Won => "won",
            ArcadeFinish::Lost => "lost",
        }
    }

    pub fn record_arcade_finish(
        puzzle: DailyPuzzle,
        mode: ArcadeMode,
        difficulty: ArcadeDifficulty,
        finish: ArcadeFinish,
    ) {
        // `DailyPuzzle::key` is the roster's own exhaustive label map.
        arcade_finishes_total().add(
            1,
            &[
                KeyValue::new("game", puzzle.key()),
                KeyValue::new("mode", arcade_mode_label(mode)),
                KeyValue::new("difficulty", arcade_difficulty_label(difficulty)),
                KeyValue::new("finish", arcade_finish_label(finish)),
            ],
        );
    }

    fn screen_label(screen: Screen) -> &'static str {
        match screen {
            Screen::Dashboard => "dashboard",
            Screen::Arcade => "arcade",
            Screen::Games => "games",
            Screen::Lateania => "lateania",
            Screen::Rebels => "rebels",
            Screen::Nethack => "nethack",
            Screen::Dcss => "dcss",
            Screen::Brogue => "brogue",
            Screen::Dopewars => "dopewars",
            Screen::Bashquest => "bashquest",
            Screen::Codekeep => "codekeep",
            Screen::Usurper => "usurper",
            Screen::GreenDragon => "greendragon",
            Screen::Darkroom => "darkroom",
            Screen::Artboard => "artboard",
            Screen::Profiles => "profiles",
            Screen::Leaderboard => "leaderboard",
            Screen::Clubhouse => "clubhouse",
            Screen::Nightcap => "nightcap",
            Screen::City => "city",
            Screen::Zen => "zen",
            Screen::DailyMatch => "daily_match",
            Screen::HouseTable => "house_table",
            Screen::Scratchpad => "scratchpad",
        }
    }

    fn presence_label(presence: Presence) -> &'static str {
        match presence {
            Presence::Active => "active",
            Presence::Idle => "idle",
        }
    }

    pub fn record_attention(
        screen: Screen,
        arcade_game: Option<ActivityGame>,
        presence: Presence,
        seconds: f64,
    ) {
        let game = match arcade_game {
            Some(game) => game_label(game),
            None => "none",
        };
        attention_seconds_total().add(
            seconds,
            &[
                KeyValue::new("screen", screen_label(screen)),
                KeyValue::new("game", game),
                KeyValue::new("presence", presence_label(presence)),
            ],
        );
    }

    fn share_cards_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_share_cards_total")
                .with_description("Share cards copied to the clipboard")
                .build()
        })
    }

    fn share_card_kind_label(kind: ShareCardKind) -> &'static str {
        match kind {
            ShareCardKind::LeWord => "le_word",
            ShareCardKind::Nonogram => "nonogram",
            ShareCardKind::Sudoku => "sudoku",
            ShareCardKind::Minesweeper => "minesweeper",
            ShareCardKind::Solitaire => "solitaire",
            ShareCardKind::RubiksCube => "rubiks_cube",
            ShareCardKind::SlidingPuzzle => "sliding_puzzle",
            ShareCardKind::Day => "day",
        }
    }

    /// One share card copied.
    pub fn record_share_card(kind: ShareCardKind) {
        share_cards_total().add(1, &[KeyValue::new("card", share_card_kind_label(kind))]);
    }

    fn sliding_puzzle_art_load_label(load: SlidingPuzzleArtLoad) -> &'static str {
        match load {
            SlidingPuzzleArtLoad::Featured => "featured",
            SlidingPuzzleArtLoad::Empty => "empty",
            SlidingPuzzleArtLoad::Failed => "failed",
        }
    }

    fn sliding_puzzle_art_loads_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_sliding_puzzle_art_loads_total")
                .with_description(
                    "Sliding Puzzle daily art loads: a gallery piece featured, an empty backlog, or a failure",
                )
                .build()
        })
    }

    /// One session asked for the day's Sliding Puzzle art.
    pub fn record_sliding_puzzle_art(load: SlidingPuzzleArtLoad) {
        sliding_puzzle_art_loads_total().add(
            1,
            &[KeyValue::new(
                "outcome",
                sliding_puzzle_art_load_label(load),
            )],
        );
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

    fn pool_shot_outcome_label(outcome: PoolShotOutcome) -> &'static str {
        match outcome {
            PoolShotOutcome::Settled => "settled",
            PoolShotOutcome::Truncated => "truncated",
            PoolShotOutcome::Rejected => "rejected",
        }
    }

    /// One daily pool shot reached the end of the only path it has. `settled`
    /// is the denominator the other two are read against: a little `rejected`
    /// is players and clients disagreeing, a rising `rejected` is a desync,
    /// and any `truncated` at all is the physics.
    pub fn record_pool_shot(outcome: PoolShotOutcome) {
        pool_shots_total().add(
            1,
            &[KeyValue::new("outcome", pool_shot_outcome_label(outcome))],
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

    /// `unavailable` is a share the gate rejected without a verdict; a run of
    /// them is an fxtwitter outage.
    fn news_x_media_lookup_label(lookup: XMediaLookup) -> &'static str {
        match lookup {
            XMediaLookup::Clean => "clean",
            XMediaLookup::Sensitive => "sensitive",
            XMediaLookup::Unavailable => "unavailable",
        }
    }

    pub fn record_news_x_media_lookup(lookup: XMediaLookup) {
        news_x_media_lookups_total().add(
            1,
            &[KeyValue::new("outcome", news_x_media_lookup_label(lookup))],
        );
    }

    /// Same shape as the News share: one counter for the submissions and one
    /// for the chips they minted, so the tracks that came in past the day's
    /// cap are visible beside the paid ones.
    fn song_queue_reward_label(reward: SongQueueReward) -> &'static str {
        match reward {
            SongQueueReward::Paid => "paid",
            SongQueueReward::DailyCapReached => "daily_cap",
        }
    }

    pub fn record_song_queued(reward: SongQueueReward) {
        songs_queued_total().add(
            1,
            &[KeyValue::new("reward", song_queue_reward_label(reward))],
        );
        song_queue_chips_paid_total().add(reward.chips() as u64, &[]);
    }

    pub fn record_gild_bought(tier: GildTier) {
        chat_gilds_total().add(1, &[KeyValue::new("tier", gild_tier_label(tier))]);
    }

    pub fn record_gild_refused(refusal: GildRefusal) {
        chat_gilds_refused_total().add(1, &[KeyValue::new("reason", gild_refusal_label(refusal))]);
    }

    pub fn record_bonsai_action(action: BonsaiAction, result: BonsaiActionResult) {
        bonsai_actions_total().add(
            1,
            &[
                KeyValue::new("action", bonsai_action_label(action)),
                KeyValue::new("result", bonsai_action_result_label(result)),
            ],
        );
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

    /// A pour ordered off the Nightcap menu (rounds count under
    /// `record_round_bought`, whichever bar they were bought at).
    pub fn record_nightcap_order(result: NightcapOrderResult) {
        nightcap_orders_total().add(1, &[KeyValue::new("result", nightcap_order_label(result))]);
    }

    /// A patron walked up and drank a credit somebody else paid for.
    pub fn record_round_drink_cashed() {
        round_drinks_cashed_total().add(1, &[]);
    }

    /// The house's off-thread work at the Nightcap failed. Orders are counted
    /// under `record_nightcap_order`; this is what a healthy order counter
    /// cannot see: a quiet bar or a stale menu.
    pub fn record_nightcap_house_failure(failure: NightcapHouseFailure) {
        nightcap_house_failures_total().add(
            1,
            &[KeyValue::new("work", nightcap_house_failure_label(failure))],
        );
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

    /// The sweep's reminder arm. Only the claims that happened or errored
    /// count; a sweep with no pot in its window is silence, not an outcome.
    pub fn record_pot_reminder(outcome: PotReminderOutcome) {
        pot_reminders_total().add(1, &[KeyValue::new("outcome", pot_reminder_label(outcome))]);
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

    fn paper_print_result_label(result: PaperPrintResult) -> &'static str {
        match result {
            PaperPrintResult::Printed => "printed",
            PaperPrintResult::Quiet => "quiet",
            PaperPrintResult::Lost => "lost",
            PaperPrintResult::Failed => "failed",
        }
    }

    fn paper_prints_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_paper_prints_total")
                .with_description("Daily paper pages (rooms and sections) by print result")
                .build()
        })
    }

    pub fn record_paper_print(result: PaperPrintResult) {
        paper_prints_total().add(
            1,
            &[KeyValue::new("result", paper_print_result_label(result))],
        );
    }

    fn paper_open_result_label(result: PaperOpenResult) -> &'static str {
        match result {
            PaperOpenResult::Login => "login",
            PaperOpenResult::Command => "command",
            PaperOpenResult::Empty => "empty",
            PaperOpenResult::AlreadyShown => "already_shown",
            PaperOpenResult::Unavailable => "unavailable",
            PaperOpenResult::Failed => "failed",
        }
    }

    fn paper_opens_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_paper_opens_total")
                .with_description("Daily paper open requests (login pop and /paper) by resolution")
                .build()
        })
    }

    pub fn record_paper_open(result: PaperOpenResult) {
        paper_opens_total().add(
            1,
            &[KeyValue::new("result", paper_open_result_label(result))],
        );
    }

    fn jobs_fetch_result_label(result: JobsFetchResult) -> &'static str {
        match result {
            JobsFetchResult::Fetched => "fetched",
            JobsFetchResult::Failed => "failed",
            JobsFetchResult::ItemSkipped => "item_skipped",
        }
    }

    fn jobs_fetches_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_jobs_fetches_total")
                .with_description("Job feed source fetches by source and result")
                .build()
        })
    }

    pub fn record_jobs_fetch(
        source: late_core::models::job_posting::JobSource,
        result: JobsFetchResult,
    ) {
        jobs_fetches_total().add(
            1,
            &[
                KeyValue::new("source", source.as_str()),
                KeyValue::new("result", jobs_fetch_result_label(result)),
            ],
        );
    }

    fn jobs_read_result_label(result: JobsReadResult) -> &'static str {
        match result {
            JobsReadResult::Queued => "queued",
            JobsReadResult::Active => "active",
            JobsReadResult::Dropped => "dropped",
            JobsReadResult::Dead => "dead",
            JobsReadResult::Failed => "failed",
        }
    }

    fn jobs_reads_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_jobs_reads_total")
                .with_description("Job postings read by the model, by how the row settled")
                .build()
        })
    }

    pub fn record_jobs_read(result: JobsReadResult) {
        jobs_reads_total().add(
            1,
            &[KeyValue::new("result", jobs_read_result_label(result))],
        );
    }

    fn jobs_press_result_label(result: JobsPressResult) -> &'static str {
        match result {
            JobsPressResult::Ran => "ran",
            JobsPressResult::Lost => "lost",
            JobsPressResult::Failed => "failed",
        }
    }

    fn jobs_press_runs_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_jobs_press_runs_total")
                .with_description("Nightly job press runs by result")
                .build()
        })
    }

    pub fn record_jobs_press(result: JobsPressResult) {
        jobs_press_runs_total().add(
            1,
            &[KeyValue::new("result", jobs_press_result_label(result))],
        );
    }

    fn jobs_released_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_jobs_released_total")
                .with_description("HN job postings released onto the shelf by the drip")
                .build()
        })
    }

    pub fn record_jobs_released(count: usize) {
        jobs_released_total().add(count as u64, &[]);
    }

    fn jobs_post_result_label(result: JobsPostResult) -> &'static str {
        match result {
            JobsPostResult::Posted => "posted",
            JobsPostResult::Retracted => "retracted",
            JobsPostResult::AtCap => "at_cap",
            JobsPostResult::Failed => "failed",
        }
    }

    fn jobs_posts_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_jobs_posts_total")
                .with_description("Job postings written or taken down on the shelf by result")
                .build()
        })
    }

    pub fn record_jobs_post(result: JobsPostResult) {
        jobs_posts_total().add(
            1,
            &[KeyValue::new("result", jobs_post_result_label(result))],
        );
    }

    fn gallery_hang_result_label(result: GalleryHangResult) -> &'static str {
        match result {
            GalleryHangResult::Hung => "hung",
            GalleryHangResult::DailyCap => "daily_cap",
            GalleryHangResult::Duplicate => "duplicate",
            GalleryHangResult::Failed => "failed",
        }
    }

    fn gallery_hangs_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_artboard_gallery_hangs_total")
                .with_description("Artboard gallery hang attempts by result")
                .build()
        })
    }

    pub fn record_gallery_hang(result: GalleryHangResult) {
        gallery_hangs_total().add(
            1,
            &[KeyValue::new("result", gallery_hang_result_label(result))],
        );
    }

    fn gallery_applause_result_label(result: GalleryApplauseResult) -> &'static str {
        match result {
            GalleryApplauseResult::Applauded => "applauded",
            GalleryApplauseResult::Withdrawn => "withdrawn",
            GalleryApplauseResult::OwnPiece => "own_piece",
            GalleryApplauseResult::NotFound => "not_found",
            GalleryApplauseResult::Closed => "closed",
            GalleryApplauseResult::Failed => "failed",
        }
    }

    fn gallery_take_down_result_label(result: GalleryTakeDownResult) -> &'static str {
        match result {
            GalleryTakeDownResult::TakenDown => "taken_down",
            GalleryTakeDownResult::NotFound => "not_found",
            GalleryTakeDownResult::NotYours => "not_yours",
            GalleryTakeDownResult::Closed => "closed",
            GalleryTakeDownResult::Failed => "failed",
        }
    }

    fn gallery_take_downs_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_artboard_gallery_take_downs_total")
                .with_description("Artboard gallery take-downs by the hanger, by result")
                .build()
        })
    }

    pub fn record_gallery_take_down(result: GalleryTakeDownResult) {
        gallery_take_downs_total().add(
            1,
            &[KeyValue::new(
                "result",
                gallery_take_down_result_label(result),
            )],
        );
    }

    fn gallery_splash_queue_depth() -> &'static Gauge<u64> {
        static METRIC: OnceLock<Gauge<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_gauge("late_ssh_artboard_gallery_splash_queue_depth")
                .with_description(
                    "Artboard gallery pieces still waiting for a day on the splash wall",
                )
                .build()
        })
    }

    pub fn record_gallery_splash_queue_depth(depth: i64) {
        gallery_splash_queue_depth().record(depth as u64, &[]);
    }

    fn gallery_applause_total() -> &'static Counter<u64> {
        static METRIC: OnceLock<Counter<u64>> = OnceLock::new();
        METRIC.get_or_init(|| {
            meter()
                .u64_counter("late_ssh_artboard_gallery_applause_total")
                .with_description("Artboard gallery applause toggles by result")
                .build()
        })
    }

    pub fn record_gallery_applause(result: GalleryApplauseResult) {
        gallery_applause_total().add(
            1,
            &[KeyValue::new(
                "result",
                gallery_applause_result_label(result),
            )],
        );
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
    use super::ShareCardKind;
    use super::SlidingPuzzleArtLoad;
    use super::XMediaLookup;
    use super::{
        ActivityGame, ArcadeDifficulty, ArcadeFinish, ArcadeMode, BioScreenOutcome, CrownRefusal,
        DailyPuzzle, DailyWinPayout, DoorGame, FightBeat, FirstContactBeat, GalleryApplauseResult,
        GalleryHangResult, GalleryTakeDownResult, GateVerdict, GildRefusal, GildTier,
        JobsFetchResult, JobsPostResult, JobsPressResult, JobsReadResult, NewsShareReward,
        NightcapHouseFailure, NightcapOrderResult, OnlineTimeFlushResult, PaperOpenResult,
        PaperPrintResult, PoolShotOutcome, PotRefusal, PotReminderOutcome, Presence, Refresh,
        RefreshOutcome, RenderReason, RoundRefusal, RunnerDoor, Screen, SessionStartStage,
        SessionUser, SongQueueReward, SshRejectReason, SummaryResult, TailorBeat,
        TranslationResult, VizWireBands,
    };
    use super::{BonsaiAction, BonsaiActionResult};

    pub fn record_ssh_connection() {}
    pub fn record_ssh_connection_rejected(_reason: SshRejectReason) {}
    pub fn record_first_contact_beat(_beat: FirstContactBeat) {}
    pub fn record_deadchannel_fight(_beat: FightBeat) {}
    pub fn record_deadchannel_tailor(_beat: TailorBeat) {}
    pub fn record_runner_door(_door: RunnerDoor) {}
    pub fn record_first_contact_bio_screen(_outcome: BioScreenOutcome) {}
    pub fn record_first_contact_gate(_verdict: GateVerdict, _staff: bool) {}
    pub fn record_render(_reason: RenderReason) {}
    pub fn record_render_skipped_clean() {}
    pub fn add_ssh_session(_delta: i64) {}
    pub fn record_ws_pair_success() {}
    pub fn record_ws_pair_rejected_unknown_token() {}
    pub fn record_pair_viz_frame(_bands: VizWireBands) {}
    pub fn record_cli_pair_usage(_ssh_mode: &str, _platform: &str) {}
    pub fn add_cli_pair_active(_delta: i64, _ssh_mode: &str, _platform: &str) {}
    pub fn record_render_frame_drop() {}
    pub fn record_render_stall_skip() {}
    pub fn record_render_stall_disconnect() {}
    pub fn record_chat_message_sent() {}
    pub fn record_chat_message_edited() {}
    pub fn record_game_win(_game: ActivityGame) {}
    pub fn record_session_start(_stage: SessionStartStage, _user: SessionUser, _seconds: f64) {}
    pub fn record_pg_refresh(_refresh: Refresh, _outcome: RefreshOutcome, _seconds: f64) {}
    pub fn observe_chat_read_permits(
        _permits: std::sync::Arc<tokio::sync::Semaphore>,
        _total: usize,
    ) {
    }
    pub fn record_arcade_finish(
        _puzzle: DailyPuzzle,
        _mode: ArcadeMode,
        _difficulty: ArcadeDifficulty,
        _finish: ArcadeFinish,
    ) {
    }
    pub fn record_attention(
        _screen: Screen,
        _arcade_game: Option<ActivityGame>,
        _presence: Presence,
        _seconds: f64,
    ) {
    }
    pub fn record_share_card(_kind: ShareCardKind) {}
    pub fn record_sliding_puzzle_art(_load: SlidingPuzzleArtLoad) {}
    pub fn record_daily_win_payout(_payout: DailyWinPayout) {}
    pub fn record_pool_shot(_outcome: PoolShotOutcome) {}
    pub fn record_news_shared(_reward: NewsShareReward) {}
    pub fn record_news_x_media_lookup(_lookup: XMediaLookup) {}
    pub fn record_song_queued(_reward: SongQueueReward) {}
    pub fn record_gild_bought(_tier: GildTier) {}
    pub fn record_gild_refused(_refusal: GildRefusal) {}
    pub fn record_bonsai_action(_action: BonsaiAction, _result: BonsaiActionResult) {}
    pub fn record_crown_taken(_price: i64) {}
    pub fn record_crown_take_refused(_refusal: CrownRefusal) {}
    pub fn record_round_bought(_patrons: i64, _chips: i64) {}
    pub fn record_round_refused(_refusal: RoundRefusal) {}
    pub fn record_nightcap_order(_result: NightcapOrderResult) {}
    pub fn record_nightcap_house_failure(_failure: NightcapHouseFailure) {}
    pub fn record_round_drink_cashed() {}
    pub fn record_pot_tickets_bought(_tickets: i64, _chips: i64) {}
    pub fn record_pot_buy_refused(_refusal: PotRefusal) {}
    pub fn record_pot_drawn(_payout: i64, _tickets: i64) {}
    pub fn record_pot_reminder(_outcome: PotReminderOutcome) {}
    pub fn record_chat_translation(_result: TranslationResult) {}
    pub fn record_chat_summary(_result: SummaryResult) {}
    pub fn record_paper_print(_result: PaperPrintResult) {}
    pub fn record_paper_open(_result: PaperOpenResult) {}
    pub fn record_jobs_fetch(
        _source: late_core::models::job_posting::JobSource,
        _result: JobsFetchResult,
    ) {
    }
    pub fn record_jobs_read(_result: JobsReadResult) {}
    pub fn record_jobs_press(_result: JobsPressResult) {}
    pub fn record_jobs_released(_count: usize) {}
    pub fn record_jobs_post(_result: JobsPostResult) {}
    pub fn record_gallery_hang(_result: GalleryHangResult) {}
    pub fn record_gallery_applause(_result: GalleryApplauseResult) {}
    pub fn record_gallery_take_down(_result: GalleryTakeDownResult) {}
    pub fn record_gallery_splash_queue_depth(_depth: i64) {}
    pub fn record_door_ingest_line(_game: DoorGame) {}
    pub fn record_door_ingest_session_failure(_game: DoorGame) {}
    pub fn record_online_time_flush(_result: OnlineTimeFlushResult) {}
}

pub use inner::*;
