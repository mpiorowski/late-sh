use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use late_core::{
    chess_puzzle::{ChessPuzzle, choose_for_date},
    db::Db,
    models::{
        chess_puzzle::{DailyPick, DailyWin, Game, GameParams},
        chips::Difficulty,
        leaderboard::DailyPuzzle,
        profile::fetch_username,
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::state::{DIFFICULTIES, find_puzzle, pack};
use crate::{
    app::{activity::event::ActivityEvent, arcade::daily::activity_game},
    metrics::{self, ArcadeDifficulty, ArcadeFinish, ArcadeMode},
};

/// How long a session load waits for the shared save queue to drain before
/// reading the database anyway. The queue is process-wide, so without a bound
/// one player's login would sit behind every other player's queued moves.
const LOAD_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// The day's puzzles, claimed in the database and found in the pack, one
/// per tier in `DIFFICULTIES` order.
#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub puzzle_date: NaiveDate,
    pub puzzles: [&'static ChessPuzzle; 3],
}

#[derive(Clone)]
pub struct ChessPuzzleService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    game_save_tx: Arc<OnceLock<mpsc::UnboundedSender<GameSaveCommand>>>,
}

enum GameSaveCommand {
    Save(GameParams),
    Complete {
        params: GameParams,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        mistakes: i32,
    },
    Flush(oneshot::Sender<()>),
}

impl ChessPuzzleService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            db,
            activity_feed,
            game_save_tx: Arc::new(OnceLock::new()),
        }
    }

    /// A board ended on this session. Counted for the dashboard, nothing
    /// stored: the daily win itself goes through `complete_game_task`.
    pub fn record_finish(
        &self,
        mode: ArcadeMode,
        difficulty: ArcadeDifficulty,
        finish: ArcadeFinish,
    ) {
        metrics::record_arcade_finish(DailyPuzzle::ChessPuzzle, mode, difficulty, finish);
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    /// Today's puzzles: the claimed row per tier, or new claims made under
    /// the advisory lock, never-used puzzles of the tier first and then the
    /// one used longest ago. A claimed puzzle the pack no longer holds is an
    /// error, so the boards show as unavailable instead of a puzzle nobody
    /// can play.
    pub async fn ensure_daily_puzzles(&self) -> Result<Daily> {
        let mut client = self.db.get().await?;
        let puzzle_date = self.today();

        let mut claimed = DailyPick::find_by_date(&**client, puzzle_date).await?;
        if claimed.len() < DIFFICULTIES.len() {
            let tx = client.transaction().await?;
            DailyPick::lock_daily_creation(&*tx).await?;
            claimed = DailyPick::find_by_date(&*tx, puzzle_date).await?;
            for difficulty in DIFFICULTIES {
                if claimed
                    .iter()
                    .any(|pick| pick.difficulty_key == difficulty.key())
                {
                    continue;
                }
                let last_used = DailyPick::last_used(&*tx, difficulty.key()).await?;
                let puzzle = choose_for_date(pack(difficulty), &last_used, puzzle_date, difficulty);
                let pick =
                    DailyPick::insert_for_date(&*tx, puzzle_date, difficulty.key(), &puzzle.id)
                        .await?;
                claimed.push(pick);
            }
            tx.commit().await?;
        }

        let mut puzzles = [None; 3];
        for (slot, difficulty) in puzzles.iter_mut().zip(DIFFICULTIES) {
            let pick = claimed
                .iter()
                .find(|pick| pick.difficulty_key == difficulty.key())
                .with_context(|| {
                    format!(
                        "no {} chess puzzle claimed for {puzzle_date}",
                        difficulty.key()
                    )
                })?;
            *slot = Some(find_puzzle(difficulty, &pick.puzzle_id).with_context(|| {
                format!(
                    "{} chess puzzle {} claimed for {puzzle_date} is not in the pack",
                    difficulty.key(),
                    pick.puzzle_id
                )
            })?);
        }
        Ok(Daily {
            puzzle_date,
            puzzles: puzzles.map(|puzzle| puzzle.expect("every tier was found above")),
        })
    }

    pub async fn load_games(&self, user_id: Uuid) -> Result<Vec<Game>> {
        // The barrier only orders queued writes ahead of this read. If it
        // cannot answer in time, the database is still authoritative:
        // returning an error here would hand bootstrap an empty restore that
        // overwrites every persisted board on the next save.
        match tokio::time::timeout(LOAD_FLUSH_TIMEOUT, self.flush_game_saves()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(
                    error = ?error,
                    "chess puzzle save queue flush failed; restoring from the database anyway"
                );
            }
            Err(_elapsed) => {
                tracing::error!(
                    timeout_secs = LOAD_FLUSH_TIMEOUT.as_secs(),
                    "chess puzzle save queue flush timed out; restoring from the database anyway"
                );
            }
        }
        let client = self.db.get().await?;
        Game::list_by_user_id(&client, user_id).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Save(params))
            .is_err()
        {
            tracing::error!("failed to enqueue chess puzzle game state save");
        }
    }

    /// Record a solved daily and persist the solved board behind it. The
    /// state only calls this from the finishing move of a daily puzzle, so the
    /// params are trusted; the table's own gates (`mistakes` 0 to 2, one win per
    /// user/date/difficulty) are the checks that remain.
    pub fn complete_game_task(
        &self,
        params: GameParams,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        mistakes: i32,
    ) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Complete {
                params,
                difficulty,
                puzzle_date,
                mistakes,
            })
            .is_err()
        {
            tracing::error!("failed to enqueue chess puzzle completed board");
        }
    }

    pub(crate) async fn flush_game_saves(&self) -> Result<()> {
        let (done_tx, done_rx) = oneshot::channel();
        self.game_save_sender()
            .send(GameSaveCommand::Flush(done_tx))
            .map_err(|_| anyhow!("chess puzzle game save queue is closed"))?;
        done_rx
            .await
            .context("chess puzzle game save worker stopped before flush")?;
        Ok(())
    }

    fn game_save_sender(&self) -> &mpsc::UnboundedSender<GameSaveCommand> {
        self.game_save_tx.get_or_init(|| {
            let (save_tx, save_rx) = mpsc::unbounded_channel();
            tokio::spawn(run_game_save_worker(
                self.db.clone(),
                self.activity_feed.clone(),
                save_rx,
            ));
            save_tx
        })
    }

    #[cfg(test)]
    pub(crate) async fn record_win_and_publish(
        &self,
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        mistakes: i32,
    ) -> Result<()> {
        record_win_and_publish(
            &self.db,
            &self.activity_feed,
            user_id,
            difficulty,
            puzzle_date,
            mistakes,
        )
        .await
    }
}

/// The one writer for every session's boards, so a player's moves land in
/// the order they were made.
async fn run_game_save_worker(
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    mut save_rx: mpsc::UnboundedReceiver<GameSaveCommand>,
) {
    while let Some(command) = save_rx.recv().await {
        match command {
            GameSaveCommand::Save(params) => {
                if let Err(error) = save_game(&db, params).await {
                    tracing::error!(error = ?error, "failed to save chess puzzle game state");
                }
            }
            GameSaveCommand::Complete {
                params,
                difficulty,
                puzzle_date,
                mistakes,
            } => {
                let user_id = params.user_id;
                match record_win_and_publish(
                    &db,
                    &activity_feed,
                    user_id,
                    difficulty,
                    puzzle_date,
                    mistakes,
                )
                .await
                {
                    Ok(()) => {
                        if let Err(error) = save_game(&db, params).await {
                            tracing::error!(
                                error = ?error,
                                "failed to save completed chess puzzle state after recording win"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            error = ?error,
                            "failed to record chess puzzle win; solved state was not persisted"
                        );
                    }
                }
            }
            GameSaveCommand::Flush(done_tx) => {
                let _ = done_tx.send(());
            }
        }
    }
}

async fn save_game(db: &Db, params: GameParams) -> Result<()> {
    let client = db.get().await?;
    Game::upsert(&client, params).await?;
    Ok(())
}

async fn record_win_and_publish(
    db: &Db,
    activity_feed: &broadcast::Sender<ActivityEvent>,
    user_id: Uuid,
    difficulty: Difficulty,
    puzzle_date: NaiveDate,
    mistakes: i32,
) -> Result<()> {
    let client = db.get().await?;
    let result = DailyWin::record_win(&client, user_id, difficulty, puzzle_date, mistakes).await?;
    if !result.fresh {
        return Ok(());
    }

    let username = fetch_username(&client, user_id).await;
    let _ = activity_feed.send(ActivityEvent::game_won_at(
        user_id,
        username,
        activity_game(DailyPuzzle::ChessPuzzle),
        Some(difficulty.key().to_string()),
        Some(mistakes),
        ActivityEvent::occurred_on_utc_date(puzzle_date),
    ));
    Ok(())
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
