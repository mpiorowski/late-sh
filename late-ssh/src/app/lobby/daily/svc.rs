use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use cozy_chess::{Board, GameStatus};
use late_core::{
    db::Db,
    models::{
        chat_room::ChatRoom,
        daily_match::DailyMatch,
        user::User,
        voice_channel::{TARGET_CHAT_ROOM, VoiceChannel},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::activity::publisher::ActivityPublisher;
use crate::app::games::{
    chess_core::{
        rules,
        types::{ChessColor, ChessMoveRecord},
    },
    chips::svc::ChipService,
};

use super::{
    backgammon::DailyBackgammonState,
    battleship::DailyBattleshipState,
    briscola::{self, DailyBriscolaState},
    checkers::DailyCheckersState,
    connect4::DailyConnect4State,
    games::DailyGame,
    reversi::DailyReversiState,
};

// The cap exceeds the sidebar panel's 4 match slots on purpose: with up to 10
// entries not all fit, so the panel shows the 4 most actionable (your-turn
// rows first, nearest deadline within — see `panel::draw_daily_inline`). The
// full set is always visible in the Lobby modal.
pub const DAILY_MAX_ACTIVE_ENTRIES: i64 = 10;
pub const DAILY_MOVE_HOURS: i64 = 24;
const DAILY_STATE_VERSION: u8 = 1;
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// Chat-room reaping is an hourly concern riding the 60s sweeper loop.
const CHAT_CLEANUP_EVERY_TICKS: u64 = 60;

/// Correspondence daily games. One process-global instance; no live actor
/// per match, every mutation loads state from the DB, validates, and
/// persists.
#[derive(Clone)]
pub struct DailyService {
    db: Db,
    chip_svc: ChipService,
    /// #lounge feed sink. The *only* thing daily matches publish to activity:
    /// a single line when a match finishes (win/loss or draw). No post/claim
    /// event, nothing else — see `finish_events`.
    activity: ActivityPublisher,
    snapshot_tx: watch::Sender<Arc<DailySnapshot>>,
    snapshot_rx: watch::Receiver<Arc<DailySnapshot>>,
    event_tx: broadcast::Sender<DailyEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct DailySnapshot {
    pub open_challenges: Vec<DailyChallengeItem>,
    pub active_matches: Vec<DailyMatchItem>,
    /// Finished matches at least one player hasn't acknowledged; each player
    /// sees their own unseen results until they open the board or dismiss
    /// the row. Newest finish first.
    pub finished_matches: Vec<DailyFinishedItem>,
}

#[derive(Clone, Debug)]
pub struct DailyChallengeItem {
    pub id: Uuid,
    pub game: DailyGame,
    pub created: DateTime<Utc>,
    pub challenger_id: Uuid,
    pub challenger_username: Option<String>,
    pub target_user_id: Option<Uuid>,
    pub target_username: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DailyMatchItem {
    pub id: Uuid,
    pub game: DailyGame,
    pub challenger_id: Uuid,
    pub challenger_username: Option<String>,
    pub opponent_id: Uuid,
    pub opponent_username: Option<String>,
    /// Chess only; `None` for games without colors.
    pub white_id: Option<Uuid>,
    pub black_id: Option<Uuid>,
    pub turn_user_id: Option<Uuid>,
    pub turn_deadline_at: Option<DateTime<Utc>>,
    /// Chess moves or battleship shots — "how far along is this match".
    pub move_count: usize,
}

#[derive(Clone, Debug)]
pub struct DailyFinishedItem {
    pub id: Uuid,
    pub game: DailyGame,
    pub challenger_id: Uuid,
    pub challenger_username: Option<String>,
    pub opponent_id: Uuid,
    pub opponent_username: Option<String>,
    /// `None` for draws.
    pub winner_user_id: Option<Uuid>,
    pub result: String,
    /// What the winner's chips did; `None` for draws and for matches finished
    /// before the payout gates existed.
    pub win_payout: Option<DailyWinPayout>,
    pub finished_at: DateTime<Utc>,
    pub challenger_seen: bool,
    pub opponent_seen: bool,
}

/// A finished match's outcome from one player's perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyOutcome {
    Won,
    Lost,
    Draw,
}

impl DailyFinishedItem {
    /// The other player from `user_id`'s perspective.
    pub fn opponent_of(&self, user_id: Uuid) -> (Uuid, Option<String>) {
        if self.challenger_id == user_id {
            (self.opponent_id, self.opponent_username.clone())
        } else {
            (self.challenger_id, self.challenger_username.clone())
        }
    }

    pub fn outcome_for(&self, user_id: Uuid) -> DailyOutcome {
        match self.winner_user_id {
            Some(winner) if winner == user_id => DailyOutcome::Won,
            Some(_) => DailyOutcome::Lost,
            None => DailyOutcome::Draw,
        }
    }
}

/// Half-moves a match has to hold before its win pays. Below this the match was
/// never played (a post-claim-resign loop, a claim nobody moved on), and paying
/// it would make the lobby a faucet. Both players' moves count.
pub const DAILY_WIN_MIN_MOVES: u64 = 5;

/// What the winner's chips did. Decided inline on finish so the banner tells
/// the truth, stored on the row (`daily_matches.win_payout`) so the lingering
/// result row can tell an offline winner the same thing; every arm is one
/// metric label. The amount is not here: `DailyGame::win_payout` is the paid
/// number by the §6 invariant that it equals the seeded template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DailyWinPayout {
    Paid,
    /// Fewer than `DAILY_WIN_MIN_MOVES` half-moves were played.
    Unplayed,
    /// A win against this opponent in this game from the same posting day
    /// already paid (SHOP.md Phase 7's pair-day cap, scoped per roster game).
    PairDayCapped,
    /// The credit call failed; logged, the match is finished all the same.
    Failed,
}

impl DailyWinPayout {
    /// The `daily_matches.win_payout` spelling; the column's CHECK lists the
    /// same four.
    pub const fn db_str(self) -> &'static str {
        match self {
            Self::Paid => "paid",
            Self::Unplayed => "unplayed",
            Self::PairDayCapped => "pair_day_capped",
            Self::Failed => "failed",
        }
    }

    pub fn from_db_str(value: &str) -> Option<Self> {
        match value {
            "paid" => Some(Self::Paid),
            "unplayed" => Some(Self::Unplayed),
            "pair_day_capped" => Some(Self::PairDayCapped),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DailyFinishOutcome {
    Won {
        user_id: Uuid,
        payout: DailyWinPayout,
    },
    Draw,
}

#[derive(Clone, Debug)]
pub enum DailyEvent {
    ChallengePosted {
        match_id: Uuid,
        game: DailyGame,
        challenger_id: Uuid,
        target_user_id: Option<Uuid>,
        target_username: Option<String>,
    },
    ChallengeClaimed {
        match_id: Uuid,
        challenger_id: Uuid,
        opponent_id: Uuid,
    },
    MovePlayed {
        match_id: Uuid,
        by_user_id: Uuid,
        label: String,
    },
    MatchFinished {
        match_id: Uuid,
        game: DailyGame,
        challenger_id: Uuid,
        opponent_id: Option<Uuid>,
        outcome: DailyFinishOutcome,
        result: String,
    },
    Error {
        user_id: Uuid,
        message: String,
    },
}

/// Persisted `daily_matches.state` for chess. Mirrors the proven
/// `ChessRuntimeState` shape minus room concepts (seats, clocks, phase).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyChessState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    pub fen: String,
    pub colors: DailyChessColors,
    pub move_history: Vec<DailyMoveRecord>,
    pub position_history: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct DailyChessColors {
    pub white: Uuid,
    pub black: Uuid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyMoveRecord {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub at: DateTime<Utc>,
}

impl DailyChessState {
    /// `start` is the opening position: `Board::default()` for chess, a
    /// shuffled back rank for chess960. Everything after it is the same game.
    fn new(white: Uuid, black: Uuid, start: &Board) -> Self {
        let fen = rules::fen(start);
        Self {
            version: DAILY_STATE_VERSION,
            revision: 0,
            fen: fen.clone(),
            colors: DailyChessColors { white, black },
            move_history: Vec::new(),
            position_history: vec![fen],
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == DAILY_STATE_VERSION,
            "unsupported daily match state version: {}",
            state.version
        );
        Ok(state)
    }

    pub fn color_of(&self, user_id: Uuid) -> Option<ChessColor> {
        if self.colors.white == user_id {
            Some(ChessColor::White)
        } else if self.colors.black == user_id {
            Some(ChessColor::Black)
        } else {
            None
        }
    }

    pub fn user_for_color(&self, color: ChessColor) -> Uuid {
        match color {
            ChessColor::White => self.colors.white,
            ChessColor::Black => self.colors.black,
        }
    }

    pub fn last_move(&self) -> Option<ChessMoveRecord> {
        self.move_history.last().map(|record| ChessMoveRecord {
            from: record.from,
            to: record.to,
            label: record.label.clone(),
        })
    }
}

/// The claim-time state for either chess variant: the colour coin flip, then
/// the opening position the caller picked. White is on the clock.
fn claim_chess_state(
    challenger_id: Uuid,
    claimer_id: Uuid,
    start: &Board,
) -> Result<(Value, Uuid)> {
    let (white, black) = if rand::random::<bool>() {
        (challenger_id, claimer_id)
    } else {
        (claimer_id, challenger_id)
    };
    Ok((
        serde_json::to_value(DailyChessState::new(white, black, start))?,
        white,
    ))
}

impl DailyService {
    pub fn new(db: Db, chip_svc: ChipService, activity: ActivityPublisher) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(DailySnapshot::default()));
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
            chip_svc,
            activity,
            snapshot_tx,
            snapshot_rx,
            event_tx,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<Arc<DailySnapshot>> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<DailyEvent> {
        self.event_tx.subscribe()
    }

    pub fn refresh_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.refresh().await {
                tracing::error!(error = ?e, "failed to refresh daily matches");
            }
        });
    }

    /// One background loop: forfeit expired turns, then republish the
    /// snapshot. The republish doubles as the slow-poll backstop for any
    /// mutation whose refresh was lost. Once an hour it also reaps match
    /// chat rooms 30+ days past finish/cancel.
    pub fn start_sweeper_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            let mut ticks: u64 = 0;
            loop {
                if let Err(e) = svc.sweep_expired().await {
                    tracing::error!(error = ?e, "failed to sweep expired daily matches");
                }
                if let Err(e) = svc.refresh().await {
                    tracing::error!(error = ?e, "failed to refresh daily matches");
                }
                if ticks.is_multiple_of(CHAT_CLEANUP_EVERY_TICKS) {
                    match svc.cleanup_stale_chat_rooms().await {
                        Ok(0) => {}
                        Ok(deleted) => {
                            tracing::info!(deleted, "reaped stale daily match chat rooms");
                        }
                        Err(e) => {
                            tracing::error!(error = ?e, "failed to reap stale daily match chat rooms");
                        }
                    }
                }
                ticks = ticks.wrapping_add(1);
                tokio::time::sleep(SWEEP_INTERVAL).await;
            }
        });
    }

    async fn cleanup_stale_chat_rooms(&self) -> Result<u64> {
        let client = self.db.get().await?;
        DailyMatch::delete_stale_chat_rooms(&client).await
    }

    pub fn post_challenge_task(
        &self,
        user_id: Uuid,
        game: DailyGame,
        target_user_id: Option<Uuid>,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.post_challenge(user_id, game, target_user_id).await {
                tracing::error!(error = ?e, %user_id, "failed to post daily challenge");
                svc.send_error(user_id, &e);
            }
        });
    }

    /// Directed challenge addressed by username (the modal's directed-draft
    /// prompt path). Resolves against the DB so the target does not need to
    /// be online.
    pub fn post_challenge_to_username_task(
        &self,
        user_id: Uuid,
        game: DailyGame,
        username: String,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = async {
                let client = svc.db.get().await?;
                let target = User::find_by_username(&client, &username)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("no user named {username}"))?;
                drop(client);
                svc.post_challenge(user_id, game, Some(target.id)).await?;
                Ok::<_, anyhow::Error>(())
            }
            .await;
            if let Err(e) = result {
                tracing::error!(error = ?e, %user_id, "failed to post directed daily challenge");
                svc.send_error(user_id, &e);
            }
        });
    }

    /// Read one match row for the board screen. Snapshot items carry only
    /// summaries; the board needs the full `state` JSON.
    pub async fn load_match(&self, match_id: Uuid) -> Result<Option<DailyMatch>> {
        let client = self.db.get().await?;
        DailyMatch::get(&client, match_id).await
    }

    pub fn claim_challenge_task(&self, user_id: Uuid, match_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.claim_challenge(user_id, match_id).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to claim daily challenge");
                svc.send_error(user_id, &e);
            }
        });
    }

    pub fn cancel_challenge_task(&self, user_id: Uuid, match_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.cancel_challenge(user_id, match_id).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to cancel daily challenge");
                svc.send_error(user_id, &e);
            }
        });
    }

    pub fn play_move_task(&self, user_id: Uuid, match_id: Uuid, from: usize, to: usize) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.play_move(user_id, match_id, from, to).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to play daily move");
                svc.send_error(user_id, &e);
            }
        });
    }

    /// Acknowledge a finished match's result (board closed or row dismissed).
    /// Fire-and-forget and silent: failing to ack just leaves the row
    /// lingering, which is safe, so no user-facing error.
    pub fn mark_result_seen_task(&self, user_id: Uuid, match_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.mark_result_seen(user_id, match_id).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to mark daily result seen");
            }
        });
    }

    pub async fn mark_result_seen(&self, user_id: Uuid, match_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let updated = DailyMatch::mark_result_seen(&client, match_id, user_id).await?;
        // A repeat ack touches 0 rows; nothing changed, nothing to publish.
        if updated > 0 {
            self.publish(&client).await?;
        }
        Ok(())
    }

    pub fn resign_task(&self, user_id: Uuid, match_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.resign(user_id, match_id).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to resign daily match");
                svc.send_error(user_id, &e);
            }
        });
    }

    pub async fn post_challenge(
        &self,
        user_id: Uuid,
        game: DailyGame,
        target_user_id: Option<Uuid>,
    ) -> Result<DailyMatch> {
        if target_user_id == Some(user_id) {
            bail!("you cannot challenge yourself");
        }
        let client = self.db.get().await?;
        let target_username = if let Some(target) = target_user_id {
            let user = User::get(&client, target)
                .await?
                .ok_or_else(|| anyhow::anyhow!("challenged user not found"))?;
            Some(user.username)
        } else {
            None
        };
        self.ensure_entry_capacity(&client, user_id).await?;
        let row =
            DailyMatch::create_challenge(&client, game.kind(), user_id, target_user_id).await?;
        let _ = self.event_tx.send(DailyEvent::ChallengePosted {
            match_id: row.id,
            game,
            challenger_id: row.challenger_id,
            target_user_id: row.target_user_id,
            target_username,
        });
        self.publish(&client).await?;
        Ok(row)
    }

    pub async fn claim_challenge(&self, user_id: Uuid, match_id: Uuid) -> Result<DailyMatch> {
        let mut client = self.db.get().await?;
        self.ensure_entry_capacity(&client, user_id).await?;
        let challenge = DailyMatch::get(&client, match_id)
            .await?
            .filter(|row| row.status == DailyMatch::STATUS_OPEN)
            .ok_or_else(|| anyhow::anyhow!("challenge is no longer open"))?;
        if challenge.challenger_id == user_id {
            bail!("you posted this challenge");
        }
        if challenge
            .target_user_id
            .is_some_and(|target| target != user_id)
        {
            bail!("this challenge is directed at someone else");
        }
        let game = DailyGame::from_kind(&challenge.game_kind)
            .ok_or_else(|| anyhow::anyhow!("unknown daily game: {}", challenge.game_kind))?;
        // Fair-start coin flip per game: chess randomizes colors (White
        // moves first), battleship randomizes who fires first.
        let (state_value, first_turn_user) = match game {
            // The only thing chess960 changes is where the pieces start; the
            // shuffle is rolled once, here, and lives on in the stored FEN.
            DailyGame::Chess => {
                claim_chess_state(challenge.challenger_id, user_id, &Board::default())?
            }
            DailyGame::Chess960 => claim_chess_state(
                challenge.challenger_id,
                user_id,
                &rules::random_chess960_board(),
            )?,
            DailyGame::Battleship => {
                let state = DailyBattleshipState::new(challenge.challenger_id, user_id);
                let first = if rand::random::<bool>() {
                    challenge.challenger_id
                } else {
                    user_id
                };
                (serde_json::to_value(state)?, first)
            }
            DailyGame::ConnectFour => {
                // `new` flips the coin for who's red, and red drops first.
                let state = DailyConnect4State::new(challenge.challenger_id, user_id);
                let first = state.red;
                (serde_json::to_value(state)?, first)
            }
            DailyGame::Reversi => {
                // `new` flips the coin for who's black, and black moves first.
                let state = DailyReversiState::new(challenge.challenger_id, user_id);
                let first = state.black;
                (serde_json::to_value(state)?, first)
            }
            DailyGame::Checkers => {
                // `new` flips the coin for who's red, and red moves first.
                let state = DailyCheckersState::new(challenge.challenger_id, user_id);
                let first = state.red;
                (serde_json::to_value(state)?, first)
            }
            DailyGame::Backgammon => {
                // `new` flips the coin for who's white, and white plays the
                // server-rolled opening (stored in the state as `next_roll`).
                let state = DailyBackgammonState::new(challenge.challenger_id, user_id);
                let first = state.white;
                (serde_json::to_value(state)?, first)
            }
            DailyGame::Briscola => {
                // `new` shuffles the deck and flips the coin for seat 0, who
                // leads the first trick. That shuffle is the match's whole
                // supply of randomness: every draw after it replays this deal.
                let state = DailyBriscolaState::new(challenge.challenger_id, user_id);
                let first = state.user_of(0);
                (serde_json::to_value(state)?, first)
            }
        };
        // Usernames for the voice channel label, loaded before the claim
        // transaction opens.
        let challenger_name = User::get(&client, challenge.challenger_id)
            .await?
            .map(|user| user.username)
            .ok_or_else(|| anyhow::anyhow!("challenger not found"))?;
        let claimer_name = User::get(&client, user_id)
            .await?
            .map(|user| user.username)
            .ok_or_else(|| anyhow::anyhow!("claiming user not found"))?;
        // One transaction: the guarded claim plus the match's private chat
        // room, both memberships, and its voice channel. If any piece fails
        // the challenge stays open instead of leaving a half-wired match.
        let tx = client.transaction().await?;
        let mut claimed = DailyMatch::claim(
            &tx,
            match_id,
            user_id,
            first_turn_user,
            Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
            &state_value,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("challenge is no longer open"))?;
        let chat_room = ChatRoom::create_daily_match_room(
            &tx,
            &claimed.game_kind,
            &format!("daily-{}", claimed.id),
            claimed.challenger_id,
            user_id,
        )
        .await?;
        VoiceChannel::upsert_for_target(
            &tx,
            TARGET_CHAT_ROOM,
            chat_room.id,
            &format!("{}: {challenger_name} v {claimer_name}", game.label()),
            true,
        )
        .await?;
        DailyMatch::set_chat_room(&tx, claimed.id, chat_room.id).await?;
        tx.commit().await?;
        claimed.chat_room_id = Some(chat_room.id);
        let _ = self.event_tx.send(DailyEvent::ChallengeClaimed {
            match_id: claimed.id,
            challenger_id: claimed.challenger_id,
            opponent_id: user_id,
        });
        self.publish(&client).await?;
        Ok(claimed)
    }

    pub async fn cancel_challenge(&self, user_id: Uuid, match_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let cancelled = DailyMatch::cancel_challenge(&client, match_id, user_id).await?;
        if cancelled == 0 {
            bail!("challenge is no longer open");
        }
        self.publish(&client).await?;
        Ok(())
    }

    /// Shared move prelude: load the active row and enforce turn + the 24h
    /// clock on the move path itself, not only in the 60s sweeper (a move
    /// landing after flag fall must be rejected and must not reset the clock;
    /// the sweeper stays the forfeit executor). Both the single-cell
    /// `play_move` and checkers' path channel go through this.
    async fn move_prelude(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
        match_id: Uuid,
    ) -> Result<(DailyMatch, DailyGame)> {
        let row = DailyMatch::get(client, match_id)
            .await?
            .filter(|row| row.status == DailyMatch::STATUS_ACTIVE)
            .ok_or_else(|| anyhow::anyhow!("match is not active"))?;
        if row.turn_user_id != Some(user_id) {
            bail!("not your turn");
        }
        if row
            .turn_deadline_at
            .is_some_and(|deadline| deadline <= Utc::now())
        {
            bail!("your time to move has expired");
        }
        let game = DailyGame::from_kind(&row.game_kind)
            .ok_or_else(|| anyhow::anyhow!("unknown daily game: {}", row.game_kind))?;
        Ok((row, game))
    }

    /// Single-cell move channel: chess `from`/`to`, or one square in `to` for
    /// the games where a move is a single cell/column. Checkers is the one
    /// game whose move is a variable-length path, so it uses
    /// `play_checkers_move` instead.
    pub async fn play_move(
        &self,
        user_id: Uuid,
        match_id: Uuid,
        from: usize,
        to: usize,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let (row, game) = self.move_prelude(&client, user_id, match_id).await?;
        match game {
            DailyGame::Chess | DailyGame::Chess960 => {
                self.play_chess_move(&client, row, game, user_id, from, to)
                    .await
            }
            // A battleship "move" is one square; `to` carries the target cell.
            DailyGame::Battleship => self.play_battleship_shot(&client, row, user_id, to).await,
            // A connect-four "move" is one column; `to` carries it.
            DailyGame::ConnectFour => self.play_connect4_drop(&client, row, user_id, to).await,
            // A reversi "move" is one square; `to` carries the target cell.
            DailyGame::Reversi => self.play_reversi_move(&client, row, user_id, to).await,
            // Checkers routes through `play_checkers_move` (a path won't fit in
            // two usizes); this arm is defensive and never reached in practice.
            DailyGame::Checkers => bail!("checkers moves use the path channel"),
            // Backgammon likewise: a turn is up to four hops.
            DailyGame::Backgammon => bail!("backgammon moves use the turn channel"),
            // A briscola "move" is one card; `to` carries its id.
            DailyGame::Briscola => self.play_briscola_card(&client, row, user_id, to).await,
        }
    }

    /// Checkers move channel: the full path (start plus each landing square, as
    /// `row * 8 + col` indices). A variable-length jump chain can't ride the
    /// two-usize `play_move`, and the server must re-validate the whole path.
    pub fn play_checkers_move_task(&self, user_id: Uuid, match_id: Uuid, path: Vec<usize>) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.play_checkers_move(user_id, match_id, path).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to play daily checkers move");
                svc.send_error(user_id, &e);
            }
        });
    }

    pub async fn play_checkers_move(
        &self,
        user_id: Uuid,
        match_id: Uuid,
        path: Vec<usize>,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let (row, game) = self.move_prelude(&client, user_id, match_id).await?;
        ensure!(game == DailyGame::Checkers, "not a checkers match");
        self.play_checkers(&client, row, user_id, &path).await
    }

    /// Backgammon turn channel: the full turn as `(from, to)` hops (point
    /// indices with the `BAR`/`OFF` sentinels). Like checkers, a
    /// variable-length turn can't ride the two-usize `play_move`, and the
    /// server re-validates the whole sequence against the stored roll.
    pub fn play_backgammon_move_task(
        &self,
        user_id: Uuid,
        match_id: Uuid,
        hops: Vec<super::backgammon::Hop>,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.play_backgammon_move(user_id, match_id, hops).await {
                tracing::error!(error = ?e, %user_id, %match_id, "failed to play daily backgammon move");
                svc.send_error(user_id, &e);
            }
        });
    }

    pub async fn play_backgammon_move(
        &self,
        user_id: Uuid,
        match_id: Uuid,
        hops: Vec<super::backgammon::Hop>,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let (row, game) = self.move_prelude(&client, user_id, match_id).await?;
        ensure!(game == DailyGame::Backgammon, "not a backgammon match");
        self.play_backgammon(&client, row, user_id, &hops).await
    }

    async fn play_chess_move(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        game: DailyGame,
        user_id: Uuid,
        from: usize,
        to: usize,
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyChessState::parse(&row.state)?;
        let board: Board = state
            .fen
            .parse()
            .map_err(|_| anyhow::anyhow!("corrupt daily match position"))?;
        let mover_color = state
            .color_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        ensure!(
            rules::chess_color(board.side_to_move()) == mover_color,
            "not your turn"
        );
        let Some(mv) = rules::legal_move_for(&board, from, to) else {
            bail!("illegal move");
        };

        let label = rules::san_label(&board, mv);
        let mut board = board;
        board.play(mv);
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        state.fen = rules::fen(&board);
        state.position_history.push(state.fen.clone());
        // The resolved move's own squares, not the pair the client sent: a
        // castle played as a two-square king push stores as the
        // king-captures-rook encoding every other castle in the history uses.
        state.move_history.push(DailyMoveRecord {
            from: mv.from as usize,
            to: mv.to as usize,
            label: label.clone(),
            at: Utc::now(),
        });

        let outcome = match board.status() {
            GameStatus::Won => Some((Some(user_id), DailyMatch::RESULT_CHECKMATE)),
            GameStatus::Drawn => Some((None, DailyMatch::RESULT_DRAW)),
            GameStatus::Ongoing => {
                let history: Vec<Board> = state
                    .position_history
                    .iter()
                    .filter_map(|fen| fen.parse().ok())
                    .collect();
                if rules::repetition_count(&history, &board) >= 3 {
                    Some((None, DailyMatch::RESULT_DRAW))
                } else {
                    None
                }
            }
        };

        let state_value = serde_json::to_value(&state)?;
        match outcome {
            Some((winner, result)) => {
                let updated = DailyMatch::finish(
                    client,
                    match_id,
                    winner,
                    result,
                    &state_value,
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(&row, game, winner, result, state.revision)
                    .await;
            }
            None => {
                let next_turn = state.user_for_color(mover_color.other());
                let updated = DailyMatch::update_state(
                    client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
            }
        }
        self.publish(client).await?;
        Ok(())
    }

    /// One shot at `cell`. A hit keeps the turn (classic battleship); a miss
    /// passes it. Either way the 24h deadline resets, and sinking the last
    /// ship finishes the match.
    async fn play_battleship_shot(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        user_id: Uuid,
        cell: usize,
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyBattleshipState::parse(&row.state)?;
        let shooter = state
            .side_index_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        let outcome = state.apply_shot(shooter, cell, Utc::now())?;
        let label = outcome.label(cell);
        let state_value = serde_json::to_value(&state)?;

        if outcome.fleet_sunk {
            let updated = DailyMatch::finish(
                client,
                match_id,
                Some(user_id),
                DailyMatch::RESULT_FLEET_SUNK,
                &state_value,
                base_revision,
            )
            .await?;
            ensure!(updated == 1, "move was superseded, reload the match");
            let _ = self.event_tx.send(DailyEvent::MovePlayed {
                match_id,
                by_user_id: user_id,
                label,
            });
            self.finish_events(
                &row,
                DailyGame::Battleship,
                Some(user_id),
                DailyMatch::RESULT_FLEET_SUNK,
                state.revision,
            )
            .await;
        } else {
            let next_turn = if outcome.hit {
                user_id
            } else {
                let opponent = DailyBattleshipState::opponent_index(shooter);
                state.side(opponent).user_id
            };
            let updated = DailyMatch::update_state(
                client,
                match_id,
                &state_value,
                user_id,
                next_turn,
                Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                base_revision,
            )
            .await?;
            ensure!(updated == 1, "move was superseded, reload the match");
            let _ = self.event_tx.send(DailyEvent::MovePlayed {
                match_id,
                by_user_id: user_id,
                label,
            });
        }
        self.publish(client).await?;
        Ok(())
    }

    /// One disc into `column`. The turn always passes (no fire-again);
    /// connecting four finishes the match, filling the board draws it.
    async fn play_connect4_drop(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        user_id: Uuid,
        column: usize,
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyConnect4State::parse(&row.state)?;
        let disc = state
            .disc_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        // The prelude checked `next_turn`; the drop history is the deeper
        // truth, so a disagreement must fail loudly, not corrupt it.
        ensure!(state.turn() == disc, "not your turn");
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        let outcome = state.apply_drop(column)?;
        let label = outcome.label(column);
        let state_value = serde_json::to_value(&state)?;

        let finished = if outcome.connected {
            Some((Some(user_id), DailyMatch::RESULT_FOUR_IN_A_ROW))
        } else if outcome.draw {
            Some((None, DailyMatch::RESULT_DRAW))
        } else {
            None
        };
        match finished {
            Some((winner, result)) => {
                let updated = DailyMatch::finish(
                    client,
                    match_id,
                    winner,
                    result,
                    &state_value,
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(&row, DailyGame::ConnectFour, winner, result, state.revision)
                    .await;
            }
            None => {
                let next_turn = state.user_of(disc.other());
                let updated = DailyMatch::update_state(
                    client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
            }
        }
        self.publish(client).await?;
        Ok(())
    }

    /// One disc at `cell`. The turn passes to whoever can move next — a forced
    /// pass is resolved by `state.turn()`, so the mover can come back up at
    /// once. Holding the most discs when neither side can move finishes the
    /// match; an equal split draws it.
    async fn play_reversi_move(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        user_id: Uuid,
        cell: usize,
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyReversiState::parse(&row.state)?;
        let disc = state
            .disc_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        ensure!(state.turn() == disc, "not your turn");
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        let (move_row, move_col) = (cell / super::reversi::SIZE, cell % super::reversi::SIZE);
        let outcome = state.apply_move(move_row, move_col)?;
        let label = outcome.label(move_row, move_col);
        let state_value = serde_json::to_value(&state)?;

        let finished = outcome.finished.then(|| {
            let result = if outcome.draw {
                DailyMatch::RESULT_DRAW
            } else {
                DailyMatch::RESULT_MOST_DISCS
            };
            (outcome.winner.map(|disc| state.user_of(disc)), result)
        });
        match finished {
            Some((winner, result)) => {
                let updated = DailyMatch::finish(
                    client,
                    match_id,
                    winner,
                    result,
                    &state_value,
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(&row, DailyGame::Reversi, winner, result, state.revision)
                    .await;
            }
            None => {
                // `turn()` already skips a blocked opponent, so this can point
                // back at the mover for another move.
                let next_turn = state.user_of(state.turn());
                let updated = DailyMatch::update_state(
                    client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
            }
        }
        self.publish(client).await?;
        Ok(())
    }

    /// One move path (a slide or a full jump chain, as `row * 8 + col` cell
    /// indices). Blocking or capturing every enemy piece finishes the match;
    /// the forty-move rule draws it. The turn always passes — checkers has no
    /// forced pass, a side with no move simply loses.
    async fn play_checkers(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        user_id: Uuid,
        path: &[usize],
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyCheckersState::parse(&row.state)?;
        let color = state
            .color_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        ensure!(state.turn() == color, "not your turn");
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        let cells: Vec<(usize, usize)> = path
            .iter()
            .map(|&i| (i / super::checkers::SIZE, i % super::checkers::SIZE))
            .collect();
        let outcome = state.apply_move(&cells)?;
        let label = outcome.label(&cells);
        let state_value = serde_json::to_value(&state)?;

        let finished = outcome.finished.then(|| {
            let result = if outcome.draw {
                DailyMatch::RESULT_DRAW
            } else {
                DailyMatch::RESULT_NO_MOVES
            };
            (outcome.winner.map(|color| state.user_of(color)), result)
        });
        match finished {
            Some((winner, result)) => {
                let updated = DailyMatch::finish(
                    client,
                    match_id,
                    winner,
                    result,
                    &state_value,
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(&row, DailyGame::Checkers, winner, result, state.revision)
                    .await;
            }
            None => {
                let next_turn = state.user_of(state.turn());
                let updated = DailyMatch::update_state(
                    client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
            }
        }
        self.publish(client).await?;
        Ok(())
    }

    /// One full turn (up to four `(from, to)` hops with the stored roll).
    /// The state validates the hops, records the turn, and the service rolls
    /// for the next mover — forced passes are recorded server-side, so the
    /// turn can bounce straight back to the mover. Bearing off all fifteen
    /// finishes the match; the defensive stall cap draws it.
    async fn play_backgammon(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        user_id: Uuid,
        hops: &[super::backgammon::Hop],
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyBackgammonState::parse(&row.state)?;
        let color = state
            .color_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        ensure!(state.turn() == color, "not your turn");
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        let outcome = state.apply_turn(hops)?;
        let label = outcome.label(hops);
        // Roll for the next mover (recording any forced passes); the stall
        // cap can finish the match here even though the turn itself didn't.
        if !outcome.finished {
            state.roll_next();
        }
        let state_value = serde_json::to_value(&state)?;

        let finished = state.is_finished().then(|| {
            match state.status() {
                super::backgammon::BackgammonStatus::Win(winner) => {
                    (Some(state.user_of(winner)), DailyMatch::RESULT_BORNE_OFF)
                }
                // The stall cap: nobody wins, nobody is paid.
                _ => (None, DailyMatch::RESULT_DRAW),
            }
        });
        match finished {
            Some((winner, result)) => {
                let updated = DailyMatch::finish(
                    client,
                    match_id,
                    winner,
                    result,
                    &state_value,
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(&row, DailyGame::Backgammon, winner, result, state.revision)
                    .await;
            }
            None => {
                // `turn()` reflects any recorded passes, so this can point
                // back at the mover for another roll.
                let next_turn = state.user_of(state.turn());
                let updated = DailyMatch::update_state(
                    client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
            }
        }
        self.publish(client).await?;
        Ok(())
    }

    /// One card onto the table. The turn passes to the follower mid-trick and
    /// to the trick winner once the trick closes, so the mover can come
    /// straight back up. Passing 60 of the 120 points finishes the match; an
    /// even split after the last trick draws it.
    async fn play_briscola_card(
        &self,
        client: &tokio_postgres::Client,
        row: DailyMatch,
        user_id: Uuid,
        card_id: usize,
    ) -> Result<()> {
        let match_id = row.id;
        let mut state = DailyBriscolaState::parse(&row.state)?;
        let seat = state
            .seat_of(user_id)
            .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
        // The prelude checked `next_turn`; the play history is the deeper
        // truth, so a disagreement must fail loudly, not corrupt it.
        ensure!(state.table().turn == seat, "not your turn");
        let card = u8::try_from(card_id)
            .ok()
            .and_then(briscola::Card::from_id)
            .ok_or_else(|| anyhow::anyhow!("that is not a card"))?;
        let base_revision = state.revision as i64;
        state.revision = state.revision.saturating_add(1);
        // Holding the card is checked here, against the replayed hand.
        let outcome = state.apply_play(card)?;
        let label = outcome.label();
        let state_value = serde_json::to_value(&state)?;

        let finished = match outcome.finish {
            Some(briscola::MatchEnd::Winner(seat)) => {
                Some((Some(state.user_of(seat)), DailyMatch::RESULT_MOST_POINTS))
            }
            Some(briscola::MatchEnd::Draw) => Some((None, DailyMatch::RESULT_DRAW)),
            None => None,
        };
        match finished {
            Some((winner, result)) => {
                let updated = DailyMatch::finish(
                    client,
                    match_id,
                    winner,
                    result,
                    &state_value,
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(&row, DailyGame::Briscola, winner, result, state.revision)
                    .await;
            }
            None => {
                let next_turn = state.turn_user();
                let updated = DailyMatch::update_state(
                    client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
                    base_revision,
                )
                .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
            }
        }
        self.publish(client).await?;
        Ok(())
    }

    /// Game-agnostic: the winner is simply the other player on the row, and
    /// the revision bump happens on the raw state JSON, so resign never needs
    /// to know which game it is quitting.
    pub async fn resign(&self, user_id: Uuid, match_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        // `finish` is revision-guarded, so a resign that raced the opponent's
        // just-committed move sees 0 rows updated; reload the fresh state and
        // retry rather than clobbering their move out of the history.
        for _ in 0..8 {
            let row = DailyMatch::get(&client, match_id)
                .await?
                .filter(|row| row.status == DailyMatch::STATUS_ACTIVE)
                .ok_or_else(|| anyhow::anyhow!("match is not active"))?;
            let game = DailyGame::from_kind(&row.game_kind)
                .ok_or_else(|| anyhow::anyhow!("unknown daily game: {}", row.game_kind))?;
            let winner = if row.challenger_id == user_id {
                row.opponent_id
            } else if row.opponent_id == Some(user_id) {
                Some(row.challenger_id)
            } else {
                bail!("you are not playing in this match");
            };
            let winner = winner.ok_or_else(|| anyhow::anyhow!("match has no opponent yet"))?;
            let mut state_value = row.state.clone();
            let base_revision = state_value
                .get("revision")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if let Some(object) = state_value.as_object_mut() {
                object.insert("revision".to_string(), Value::from(base_revision + 1));
            }
            let updated = DailyMatch::finish(
                &client,
                match_id,
                Some(winner),
                DailyMatch::RESULT_RESIGN,
                &state_value,
                base_revision,
            )
            .await?;
            if updated == 1 {
                // A resignation is not a move: the half-moves played are the
                // revision the board had before it.
                self.finish_events(
                    &row,
                    game,
                    Some(winner),
                    DailyMatch::RESULT_RESIGN,
                    base_revision as u64,
                )
                .await;
                self.publish(&client).await?;
                return Ok(());
            }
        }
        bail!("resign kept racing the opponent's move, try again")
    }

    /// Forfeit every active match whose deadline passed. Durable by
    /// construction: deadlines are DB timestamps, so this survives restarts.
    pub async fn sweep_expired(&self) -> Result<Vec<DailyMatch>> {
        let client = self.db.get().await?;
        let forfeited = DailyMatch::forfeit_expired(&client).await?;
        for row in &forfeited {
            tracing::info!(match_id = %row.id, "daily match forfeited on move deadline");
            let Some(game) = DailyGame::from_kind(&row.game_kind) else {
                tracing::error!(
                    match_id = %row.id,
                    game_kind = row.game_kind,
                    "forfeited daily match has unknown game kind, skipping payout"
                );
                continue;
            };
            let moves_played = row
                .state
                .get("revision")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            self.finish_events(
                row,
                game,
                row.winner_user_id,
                DailyMatch::RESULT_TIMEOUT,
                moves_played,
            )
            .await;
        }
        if !forfeited.is_empty() {
            self.publish(&client).await?;
        }
        Ok(forfeited)
    }

    async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        self.publish(&client).await
    }

    async fn ensure_entry_capacity(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
    ) -> Result<()> {
        let count = DailyMatch::count_active_entries(client, user_id).await?;
        if count >= DAILY_MAX_ACTIVE_ENTRIES {
            bail!(
                "daily games limit reached: max {} open challenges and active matches",
                DAILY_MAX_ACTIVE_ENTRIES
            );
        }
        Ok(())
    }

    /// The single finish choke point: every decisive or drawn finish (move,
    /// resign, sweeper) lands here once the row is written. Pays the winner
    /// behind the two lobby gates, then broadcasts the outcome so the banner
    /// can say what the chips did, then posts the #lounge line.
    async fn finish_events(
        &self,
        row: &DailyMatch,
        game: DailyGame,
        winner_user_id: Option<Uuid>,
        result: &str,
        moves_played: u64,
    ) {
        let outcome = match winner_user_id {
            Some(winner) => {
                let payout = self.pay_winner(row, game, winner, moves_played).await;
                self.record_win_payout(row.id, payout).await;
                DailyFinishOutcome::Won {
                    user_id: winner,
                    payout,
                }
            }
            None => DailyFinishOutcome::Draw,
        };
        let _ = self.event_tx.send(DailyEvent::MatchFinished {
            match_id: row.id,
            game,
            challenger_id: row.challenger_id,
            opponent_id: row.opponent_id,
            outcome,
            result: result.to_string(),
        });
        // Announce the finished match to #lounge, one line per match, whether
        // decisive (win/loss) or a draw. This is the only activity daily games
        // publish; posting/claiming stay silent. `opponent_id` is always set on
        // a finished (claimed) match, but guard rather than assume.
        if let Some(opponent) = row.opponent_id {
            self.activity.daily_result_task(
                row.id,
                game.display_name(),
                row.challenger_id,
                opponent,
                winner_user_id,
            );
        }
    }

    /// The win payout behind its two gates (SHOP.md Phase 7). Gate 1 is the
    /// match itself: fewer than `DAILY_WIN_MIN_MOVES` half-moves and nothing is
    /// even asked of the DB. Gate 2 is the pair: one paid win per opponent per
    /// game per UTC day the match was posted, enforced inside the grant (the
    /// claim row carries the template's `game`, so each roster game is its
    /// own cap). Every outcome is logged and counted here and nowhere else.
    async fn pay_winner(
        &self,
        row: &DailyMatch,
        game: DailyGame,
        winner: Uuid,
        moves_played: u64,
    ) -> DailyWinPayout {
        let payout = if moves_played < DAILY_WIN_MIN_MOVES {
            DailyWinPayout::Unplayed
        } else {
            let loser = if winner == row.challenger_id {
                row.opponent_id
                    .expect("a finished daily match has an opponent")
            } else {
                row.challenger_id
            };
            let pair_day_key = format!("{loser}:{}", row.created.date_naive());
            match self
                .chip_svc
                .credit_per_event_pair_day_reward_template(
                    winner,
                    game.reward_key(),
                    &row.id.to_string(),
                    &pair_day_key,
                    game.chip_move(),
                )
                .await
            {
                Ok(grant) if grant.credited => DailyWinPayout::Paid,
                // The match key cannot collide on a first finish (`finish` is
                // revision-guarded and the sweeper hands each row out once), so
                // a refusal here is the pair-day key.
                Ok(_) => DailyWinPayout::PairDayCapped,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %winner,
                        match_id = %row.id,
                        game = game.label(),
                        "failed to credit daily win chips"
                    );
                    DailyWinPayout::Failed
                }
            }
        };
        match payout {
            DailyWinPayout::Paid | DailyWinPayout::Failed => {}
            DailyWinPayout::Unplayed | DailyWinPayout::PairDayCapped => {
                tracing::info!(
                    user_id = %winner,
                    match_id = %row.id,
                    game = game.label(),
                    moves_played,
                    ?payout,
                    "daily win refused, no chips"
                );
            }
        }
        crate::metrics::record_daily_win_payout(payout);
        payout
    }

    /// Persist the payout outcome on the finished row for the lingering
    /// result line. A write failure loses only that line (the chips and the
    /// finish are already durable), so it is logged here and goes no further.
    async fn record_win_payout(&self, match_id: Uuid, payout: DailyWinPayout) {
        let written = match self.db.get().await {
            Ok(client) => DailyMatch::set_win_payout(&client, match_id, payout.db_str()).await,
            Err(error) => Err(error),
        };
        if let Err(error) = written {
            tracing::error!(
                ?error,
                match_id = %match_id,
                ?payout,
                "failed to record daily win payout on the match row"
            );
        }
    }

    fn send_error(&self, user_id: Uuid, error: &anyhow::Error) {
        let _ = self.event_tx.send(DailyEvent::Error {
            user_id,
            message: error.root_cause().to_string(),
        });
    }

    async fn publish(&self, client: &tokio_postgres::Client) -> Result<()> {
        let open = DailyMatch::list_open(client).await?;
        let active = DailyMatch::list_active(client).await?;
        let finished = DailyMatch::list_finished_unseen(client).await?;
        let mut user_ids: Vec<Uuid> = open
            .iter()
            .flat_map(|row| [Some(row.challenger_id), row.target_user_id])
            .chain(
                active
                    .iter()
                    .chain(finished.iter())
                    .flat_map(|row| [Some(row.challenger_id), row.opponent_id]),
            )
            .flatten()
            .collect();
        user_ids.sort();
        user_ids.dedup();
        let usernames = User::list_usernames_by_ids(client, &user_ids).await?;

        // Rows whose game kind this build doesn't know (from a newer deploy)
        // stay in the DB untouched but are hidden from the snapshot.
        let open_challenges = open
            .into_iter()
            .filter_map(|row| {
                let game = DailyGame::from_kind(&row.game_kind)?;
                Some(DailyChallengeItem {
                    id: row.id,
                    game,
                    created: row.created,
                    challenger_id: row.challenger_id,
                    challenger_username: usernames.get(&row.challenger_id).cloned(),
                    target_user_id: row.target_user_id,
                    target_username: row
                        .target_user_id
                        .and_then(|id| usernames.get(&id).cloned()),
                })
            })
            .collect();
        let active_matches = active
            .into_iter()
            .filter_map(|row| {
                let opponent_id = row.opponent_id?;
                let game = DailyGame::from_kind(&row.game_kind)?;
                let (white_id, black_id, move_count) = match game {
                    DailyGame::Chess | DailyGame::Chess960 => {
                        let state = DailyChessState::parse(&row.state).ok();
                        (
                            state.as_ref().map(|state| state.colors.white),
                            state.as_ref().map(|state| state.colors.black),
                            state
                                .as_ref()
                                .map(|state| state.move_history.len())
                                .unwrap_or(0),
                        )
                    }
                    DailyGame::Battleship => {
                        let state = DailyBattleshipState::parse(&row.state).ok();
                        (
                            None,
                            None,
                            state
                                .as_ref()
                                .map(DailyBattleshipState::shot_count)
                                .unwrap_or(0),
                        )
                    }
                    DailyGame::ConnectFour => {
                        let state = DailyConnect4State::parse(&row.state).ok();
                        (
                            None,
                            None,
                            state
                                .as_ref()
                                .map(DailyConnect4State::move_count)
                                .unwrap_or(0),
                        )
                    }
                    DailyGame::Reversi => {
                        let state = DailyReversiState::parse(&row.state).ok();
                        (
                            None,
                            None,
                            state
                                .as_ref()
                                .map(DailyReversiState::move_count)
                                .unwrap_or(0),
                        )
                    }
                    DailyGame::Checkers => {
                        let state = DailyCheckersState::parse(&row.state).ok();
                        (
                            None,
                            None,
                            state
                                .as_ref()
                                .map(DailyCheckersState::move_count)
                                .unwrap_or(0),
                        )
                    }
                    DailyGame::Backgammon => {
                        let state = DailyBackgammonState::parse(&row.state).ok();
                        (
                            None,
                            None,
                            state
                                .as_ref()
                                .map(DailyBackgammonState::move_count)
                                .unwrap_or(0),
                        )
                    }
                    DailyGame::Briscola => {
                        let state = DailyBriscolaState::parse(&row.state).ok();
                        (
                            None,
                            None,
                            state
                                .as_ref()
                                .map(DailyBriscolaState::move_count)
                                .unwrap_or(0),
                        )
                    }
                };
                Some(DailyMatchItem {
                    id: row.id,
                    game,
                    challenger_id: row.challenger_id,
                    challenger_username: usernames.get(&row.challenger_id).cloned(),
                    opponent_id,
                    opponent_username: usernames.get(&opponent_id).cloned(),
                    white_id,
                    black_id,
                    turn_user_id: row.turn_user_id,
                    turn_deadline_at: row.turn_deadline_at,
                    move_count,
                })
            })
            .collect();
        let finished_matches = finished
            .into_iter()
            .filter_map(|row| {
                let opponent_id = row.opponent_id?;
                let game = DailyGame::from_kind(&row.game_kind)?;
                Some(DailyFinishedItem {
                    id: row.id,
                    game,
                    challenger_id: row.challenger_id,
                    challenger_username: usernames.get(&row.challenger_id).cloned(),
                    opponent_id,
                    opponent_username: usernames.get(&opponent_id).cloned(),
                    winner_user_id: row.winner_user_id,
                    result: row.result,
                    // The column is CHECKed to these four spellings, so an
                    // unreadable value is a corrupt row, not a case.
                    win_payout: row.win_payout.as_deref().map(|value| {
                        DailyWinPayout::from_db_str(value)
                            .expect("daily_matches.win_payout holds a checked spelling")
                    }),
                    // `finish`/`forfeit_expired`/`set_win_payout` were the
                    // last writers, so `updated` is the finish time.
                    finished_at: row.updated,
                    challenger_seen: row.challenger_result_seen_at.is_some(),
                    opponent_seen: row.opponent_result_seen_at.is_some(),
                })
            })
            .collect();
        let _ = self.snapshot_tx.send(Arc::new(DailySnapshot {
            open_challenges,
            active_matches,
            finished_matches,
        }));
        Ok(())
    }
}
