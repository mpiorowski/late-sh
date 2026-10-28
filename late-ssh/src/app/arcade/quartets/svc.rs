use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use late_core::{
    db::Db,
    models::{
        leaderboard::{DailyPuzzle, SINGLE_TIER_KEY},
        profile::fetch_username,
        quartets::{DailyWall, DailyWin, Game, GameParams},
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::state::{Puzzle, choose_for_date, find_puzzle, pack};
use crate::{
    app::{activity::event::ActivityEvent, arcade::daily::activity_game},
    metrics::{self, ArcadeDifficulty, ArcadeFinish, ArcadeMode},
};

/// How long a session load waits for the shared save queue to drain before
/// reading the database anyway. The queue is process-wide, so without a bound
/// one player's login would sit behind every other player's queued tries.
const LOAD_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// The day's wall, claimed in the database and found in the pack.
#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub puzzle_date: NaiveDate,
    pub puzzle: &'static Puzzle,
}

#[derive(Clone)]
pub struct QuartetsService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    game_save_tx: Arc<OnceLock<mpsc::UnboundedSender<GameSaveCommand>>>,
}

enum GameSaveCommand {
    Save(GameParams),
    Bank {
        user_id: Uuid,
        puzzle_date: NaiveDate,
        strikes: i32,
    },
    Flush(oneshot::Sender<()>),
}

impl QuartetsService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            db,
            activity_feed,
            game_save_tx: Arc::new(OnceLock::new()),
        }
    }

    /// A wall ended on this session, solved or out of strikes. Counted for
    /// the dashboard, nothing stored: the win itself goes through
    /// `bank_task`.
    pub fn record_finish(
        &self,
        mode: ArcadeMode,
        difficulty: ArcadeDifficulty,
        finish: ArcadeFinish,
    ) {
        metrics::record_arcade_finish(DailyPuzzle::Quartets, mode, difficulty, finish);
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    /// Today's wall: the claimed row, or a new claim made under the
    /// advisory lock, never-used puzzles first and then the one used
    /// longest ago. A claimed puzzle the pack no longer holds is an error,
    /// so the board shows as unavailable instead of a wall nobody can check.
    pub async fn ensure_daily_wall(&self) -> Result<Daily> {
        let mut client = self.db.get().await?;
        let puzzle_date = self.today();

        let claimed = match DailyWall::find_by_date(&**client, puzzle_date).await? {
            Some(claimed) => claimed,
            None => {
                let tx = client.transaction().await?;
                DailyWall::lock_daily_creation(&*tx).await?;
                let claimed = match DailyWall::find_by_date(&*tx, puzzle_date).await? {
                    Some(claimed) => claimed,
                    None => {
                        let last_used = DailyWall::last_used(&*tx).await?;
                        let puzzle = choose_for_date(pack(), &last_used, puzzle_date);
                        DailyWall::insert_for_date(&*tx, puzzle_date, puzzle.id).await?
                    }
                };
                tx.commit().await?;
                claimed
            }
        };
        let puzzle = find_puzzle(claimed.puzzle_id).with_context(|| {
            format!(
                "quartets puzzle {} claimed for {puzzle_date} is not in the pack",
                claimed.puzzle_id
            )
        })?;
        Ok(Daily {
            puzzle_date: claimed.puzzle_date,
            puzzle,
        })
    }

    pub async fn load_game(&self, user_id: Uuid, puzzle_date: NaiveDate) -> Result<Option<Game>> {
        // The barrier only orders queued writes ahead of this read. If it
        // cannot answer in time, the database is still authoritative, and
        // a save never shortens the stored tries, so a late one loses
        // nothing.
        match tokio::time::timeout(LOAD_FLUSH_TIMEOUT, self.flush_game_saves()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(
                    error = ?error,
                    "quartets save queue flush failed; restoring from the database anyway"
                );
            }
            Err(_elapsed) => {
                tracing::error!(
                    timeout_secs = LOAD_FLUSH_TIMEOUT.as_secs(),
                    "quartets save queue flush timed out; restoring from the database anyway"
                );
            }
        }
        let client = self.db.get().await?;
        Game::find_for_date(&client, user_id, puzzle_date).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Save(params))
            .is_err()
        {
            tracing::error!("failed to enqueue quartets game state save");
        }
    }

    /// Record a solved wall and publish it when it is the first. The state
    /// sends it once per session; the table's UNIQUE is the gate across
    /// sessions.
    pub fn bank_task(&self, user_id: Uuid, puzzle_date: NaiveDate, strikes: i32) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Bank {
                user_id,
                puzzle_date,
                strikes,
            })
            .is_err()
        {
            tracing::error!("failed to enqueue quartets win");
        }
    }

    pub(crate) async fn flush_game_saves(&self) -> Result<()> {
        let (done_tx, done_rx) = oneshot::channel();
        self.game_save_sender()
            .send(GameSaveCommand::Flush(done_tx))
            .map_err(|_| anyhow!("quartets game save queue is closed"))?;
        done_rx
            .await
            .context("quartets game save worker stopped before flush")?;
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
        puzzle_date: NaiveDate,
        strikes: i32,
    ) -> Result<()> {
        record_win_and_publish(&self.db, &self.activity_feed, user_id, puzzle_date, strikes).await
    }
}

/// The one writer for every session's tries and wins, so a player's saves
/// land in the order they were made.
async fn run_game_save_worker(
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    mut save_rx: mpsc::UnboundedReceiver<GameSaveCommand>,
) {
    while let Some(command) = save_rx.recv().await {
        match command {
            GameSaveCommand::Save(params) => {
                if let Err(error) = save_game(&db, params).await {
                    tracing::error!(error = ?error, "failed to save quartets game state");
                }
            }
            GameSaveCommand::Bank {
                user_id,
                puzzle_date,
                strikes,
            } => {
                if let Err(error) =
                    record_win_and_publish(&db, &activity_feed, user_id, puzzle_date, strikes).await
                {
                    tracing::error!(error = ?error, "failed to record quartets win");
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
    Game::upsert(&client, params).await
}

async fn record_win_and_publish(
    db: &Db,
    activity_feed: &broadcast::Sender<ActivityEvent>,
    user_id: Uuid,
    puzzle_date: NaiveDate,
    strikes: i32,
) -> Result<()> {
    let client = db.get().await?;
    let result = DailyWin::record_win(&client, user_id, puzzle_date, strikes).await?;
    if !result.fresh {
        return Ok(());
    }

    let username = fetch_username(&client, user_id).await;
    let _ = activity_feed.send(ActivityEvent::game_won_at(
        user_id,
        username,
        activity_game(DailyPuzzle::Quartets),
        Some(SINGLE_TIER_KEY.to_string()),
        None,
        ActivityEvent::occurred_on_utc_date(puzzle_date),
    ));
    Ok(())
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
