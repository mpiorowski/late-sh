use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use cozy_chess::{Board, GameStatus};
use late_core::{
    db::Db,
    models::{daily_match::DailyMatch, reward::DAILY_CHESS_WIN_REWARD_KEY, user::User},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::games::{
    chess_core::{
        rules,
        types::{ChessColor, ChessMoveRecord},
    },
    chips::svc::ChipService,
};

pub const DAILY_MAX_ACTIVE_ENTRIES: i64 = 5;
pub const DAILY_MOVE_HOURS: i64 = 24;
pub const DAILY_CHESS_WIN_CHIP_PAYOUT: i64 = 500;
const DAILY_CHESS_WIN_LEDGER_REASON: &str = "daily_chess_win";
const DAILY_STATE_VERSION: u8 = 1;
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// Correspondence daily games. One process-global instance like
/// `RoomsService`; no live actor per match, every mutation loads state from
/// the DB, validates, and persists.
#[derive(Clone)]
pub struct DailyService {
    db: Db,
    chip_svc: ChipService,
    snapshot_tx: watch::Sender<Arc<DailySnapshot>>,
    snapshot_rx: watch::Receiver<Arc<DailySnapshot>>,
    event_tx: broadcast::Sender<DailyEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct DailySnapshot {
    pub open_challenges: Vec<DailyChallengeItem>,
    pub active_matches: Vec<DailyMatchItem>,
}

#[derive(Clone, Debug)]
pub struct DailyChallengeItem {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub challenger_id: Uuid,
    pub challenger_username: Option<String>,
    pub target_user_id: Option<Uuid>,
    pub target_username: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DailyMatchItem {
    pub id: Uuid,
    pub challenger_id: Uuid,
    pub challenger_username: Option<String>,
    pub opponent_id: Uuid,
    pub opponent_username: Option<String>,
    pub white_id: Option<Uuid>,
    pub black_id: Option<Uuid>,
    pub turn_user_id: Option<Uuid>,
    pub turn_deadline_at: Option<DateTime<Utc>>,
    pub move_count: usize,
}

#[derive(Clone, Debug)]
pub enum DailyEvent {
    ChallengePosted {
        match_id: Uuid,
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
        winner_user_id: Option<Uuid>,
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
    fn new(white: Uuid, black: Uuid) -> Self {
        let fen = format!("{}", Board::default());
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

impl DailyService {
    pub fn new(db: Db, chip_svc: ChipService) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(DailySnapshot::default()));
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
            chip_svc,
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
    /// mutation whose refresh was lost.
    pub fn start_sweeper_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = svc.sweep_expired().await {
                    tracing::error!(error = ?e, "failed to sweep expired daily matches");
                }
                if let Err(e) = svc.refresh().await {
                    tracing::error!(error = ?e, "failed to refresh daily matches");
                }
                tokio::time::sleep(SWEEP_INTERVAL).await;
            }
        });
    }

    pub fn post_challenge_task(&self, user_id: Uuid, target_user_id: Option<Uuid>) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.post_challenge(user_id, target_user_id).await {
                tracing::error!(error = ?e, %user_id, "failed to post daily challenge");
                svc.send_error(user_id, &e);
            }
        });
    }

    /// Directed challenge addressed by username (the `/challenge @user` and
    /// modal prompt path). Resolves against the DB so the target does not
    /// need to be online.
    pub fn post_challenge_to_username_task(&self, user_id: Uuid, username: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            let result = async {
                let client = svc.db.get().await?;
                let target = User::find_by_username(&client, &username)
                    .await?
                    .ok_or_else(|| anyhow::anyhow!("no user named {username}"))?;
                drop(client);
                svc.post_challenge(user_id, Some(target.id)).await?;
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
        let row = DailyMatch::create_challenge(&client, user_id, target_user_id).await?;
        let _ = self.event_tx.send(DailyEvent::ChallengePosted {
            match_id: row.id,
            challenger_id: row.challenger_id,
            target_user_id: row.target_user_id,
            target_username,
        });
        self.publish(&client).await?;
        Ok(row)
    }

    pub async fn claim_challenge(&self, user_id: Uuid, match_id: Uuid) -> Result<DailyMatch> {
        let client = self.db.get().await?;
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
        let (white, black) = if rand::random::<bool>() {
            (challenge.challenger_id, user_id)
        } else {
            (user_id, challenge.challenger_id)
        };
        let state = DailyChessState::new(white, black);
        let state_value = serde_json::to_value(&state)?;
        let claimed = DailyMatch::claim(
            &client,
            match_id,
            user_id,
            white,
            Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
            &state_value,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("challenge is no longer open"))?;
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

    pub async fn play_move(
        &self,
        user_id: Uuid,
        match_id: Uuid,
        from: usize,
        to: usize,
    ) -> Result<()> {
        let client = self.db.get().await?;
        let row = DailyMatch::get(&client, match_id)
            .await?
            .filter(|row| row.status == DailyMatch::STATUS_ACTIVE)
            .ok_or_else(|| anyhow::anyhow!("match is not active"))?;
        if row.turn_user_id != Some(user_id) {
            bail!("not your turn");
        }
        // Enforce the 24h clock on the move path itself, not only in the 60s
        // sweeper: a move landing after flag fall must be rejected (and must
        // not reset the deadline). The sweeper stays the forfeit executor.
        if row
            .turn_deadline_at
            .is_some_and(|deadline| deadline <= Utc::now())
        {
            bail!("your time to move has expired");
        }
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
        state.fen = format!("{}", board);
        state.position_history.push(state.fen.clone());
        state.move_history.push(DailyMoveRecord {
            from,
            to,
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
                let updated =
                    DailyMatch::finish(&client, match_id, winner, result, &state_value, base_revision)
                        .await?;
                ensure!(updated == 1, "move was superseded, reload the match");
                let _ = self.event_tx.send(DailyEvent::MovePlayed {
                    match_id,
                    by_user_id: user_id,
                    label,
                });
                self.finish_events(match_id, winner, result);
            }
            None => {
                let next_turn = state.user_for_color(mover_color.other());
                let updated = DailyMatch::update_state(
                    &client,
                    match_id,
                    &state_value,
                    user_id,
                    next_turn,
                    Utc::now() + chrono::Duration::hours(DAILY_MOVE_HOURS),
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
        self.publish(&client).await?;
        Ok(())
    }

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
            let mut state = DailyChessState::parse(&row.state)?;
            let resigning_color = state
                .color_of(user_id)
                .ok_or_else(|| anyhow::anyhow!("you are not playing in this match"))?;
            let winner = state.user_for_color(resigning_color.other());
            let base_revision = state.revision as i64;
            state.revision = state.revision.saturating_add(1);
            let state_value = serde_json::to_value(&state)?;
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
                self.finish_events(match_id, Some(winner), DailyMatch::RESULT_RESIGN);
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
            self.finish_events(row.id, row.winner_user_id, DailyMatch::RESULT_TIMEOUT);
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

    fn finish_events(&self, match_id: Uuid, winner_user_id: Option<Uuid>, result: &str) {
        let _ = self.event_tx.send(DailyEvent::MatchFinished {
            match_id,
            winner_user_id,
            result: result.to_string(),
        });
        let Some(winner) = winner_user_id else {
            return;
        };
        let chip_svc = self.chip_svc.clone();
        tokio::spawn(async move {
            match chip_svc
                .credit_per_event_reward_template(
                    winner,
                    DAILY_CHESS_WIN_REWARD_KEY,
                    &match_id.to_string(),
                    DAILY_CHESS_WIN_LEDGER_REASON,
                )
                .await
            {
                Ok(payout) => {
                    if !payout.credited {
                        tracing::info!(
                            user_id = %winner,
                            match_id = %match_id,
                            payout = payout.amount,
                            "daily chess win already paid for this match"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %winner,
                        "failed to credit daily chess win chips"
                    );
                }
            }
        });
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
        let mut user_ids: Vec<Uuid> = open
            .iter()
            .flat_map(|row| [Some(row.challenger_id), row.target_user_id])
            .chain(
                active
                    .iter()
                    .flat_map(|row| [Some(row.challenger_id), row.opponent_id]),
            )
            .flatten()
            .collect();
        user_ids.sort();
        user_ids.dedup();
        let usernames = User::list_usernames_by_ids(client, &user_ids).await?;

        let open_challenges = open
            .into_iter()
            .map(|row| DailyChallengeItem {
                id: row.id,
                created: row.created,
                challenger_id: row.challenger_id,
                challenger_username: usernames.get(&row.challenger_id).cloned(),
                target_user_id: row.target_user_id,
                target_username: row
                    .target_user_id
                    .and_then(|id| usernames.get(&id).cloned()),
            })
            .collect();
        let active_matches = active
            .into_iter()
            .filter_map(|row| {
                let opponent_id = row.opponent_id?;
                let state = DailyChessState::parse(&row.state).ok();
                Some(DailyMatchItem {
                    id: row.id,
                    challenger_id: row.challenger_id,
                    challenger_username: usernames.get(&row.challenger_id).cloned(),
                    opponent_id,
                    opponent_username: usernames.get(&opponent_id).cloned(),
                    white_id: state.as_ref().map(|state| state.colors.white),
                    black_id: state.as_ref().map(|state| state.colors.black),
                    turn_user_id: row.turn_user_id,
                    turn_deadline_at: row.turn_deadline_at,
                    move_count: state
                        .as_ref()
                        .map(|state| state.move_history.len())
                        .unwrap_or(0),
                })
            })
            .collect();
        let _ = self.snapshot_tx.send(Arc::new(DailySnapshot {
            open_challenges,
            active_matches,
        }));
        Ok(())
    }
}
