use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use late_core::{
    db::Db,
    models::{
        chips::Difficulty,
        leaderboard::DailyPuzzle,
        profile::fetch_username,
        word_wheel::{DailyWheel, DailyWin, Game, GameParams},
    },
    word_wheel::{WordWheel, choose_for_date},
};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::state::{find_wheel, pack};
use crate::{
    app::{activity::event::ActivityEvent, arcade::daily::activity_game},
    metrics::{self, ArcadeDifficulty, ArcadeFinish, ArcadeMode},
};

/// How long a session load waits for the shared save queue to drain before
/// reading the database anyway. The queue is process-wide, so without a bound
/// one player's login would sit behind every other player's queued words.
const LOAD_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// The day's wheel, claimed in the database and found in the pack.
#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub puzzle_date: NaiveDate,
    pub wheel: &'static WordWheel,
}

#[derive(Clone)]
pub struct WordWheelService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    game_save_tx: Arc<OnceLock<mpsc::UnboundedSender<GameSaveCommand>>>,
}

enum GameSaveCommand {
    Save(GameParams),
    Bank {
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        score: i32,
    },
    Flush(oneshot::Sender<()>),
}

impl WordWheelService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            db,
            activity_feed,
            game_save_tx: Arc::new(OnceLock::new()),
        }
    }

    /// A rank was reached on this session. Counted for the dashboard,
    /// nothing stored: the win itself goes through `bank_task`.
    pub fn record_finish(
        &self,
        mode: ArcadeMode,
        difficulty: ArcadeDifficulty,
        finish: ArcadeFinish,
    ) {
        metrics::record_arcade_finish(DailyPuzzle::WordWheel, mode, difficulty, finish);
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    /// Today's wheel: the claimed row, or a new claim made under the
    /// advisory lock, never-used wheels first and then the one used longest
    /// ago. A claimed wheel the pack no longer holds is an error, so the
    /// board shows as unavailable instead of a wheel nobody can check.
    pub async fn ensure_daily_wheel(&self) -> Result<Daily> {
        let mut client = self.db.get().await?;
        let puzzle_date = self.today();

        let claimed = match DailyWheel::find_by_date(&**client, puzzle_date).await? {
            Some(claimed) => claimed,
            None => {
                let tx = client.transaction().await?;
                DailyWheel::lock_daily_creation(&*tx).await?;
                let claimed = match DailyWheel::find_by_date(&*tx, puzzle_date).await? {
                    Some(claimed) => claimed,
                    None => {
                        let last_used = DailyWheel::last_used(&*tx).await?;
                        let wheel = choose_for_date(pack(), &last_used, puzzle_date);
                        DailyWheel::insert_for_date(&*tx, puzzle_date, &wheel.letters).await?
                    }
                };
                tx.commit().await?;
                claimed
            }
        };
        let wheel = find_wheel(&claimed.letters).with_context(|| {
            format!(
                "word wheel {} claimed for {puzzle_date} is not in the pack",
                claimed.letters
            )
        })?;
        Ok(Daily {
            puzzle_date: claimed.puzzle_date,
            wheel,
        })
    }

    pub async fn load_game(&self, user_id: Uuid, puzzle_date: NaiveDate) -> Result<Option<Game>> {
        // The barrier only orders queued writes ahead of this read. If it
        // cannot answer in time, the database is still authoritative, and
        // saves merge, so a late one loses nothing.
        match tokio::time::timeout(LOAD_FLUSH_TIMEOUT, self.flush_game_saves()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(
                    error = ?error,
                    "word wheel save queue flush failed; restoring from the database anyway"
                );
            }
            Err(_elapsed) => {
                tracing::error!(
                    timeout_secs = LOAD_FLUSH_TIMEOUT.as_secs(),
                    "word wheel save queue flush timed out; restoring from the database anyway"
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
            tracing::error!("failed to enqueue word wheel game state save");
        }
    }

    /// Record a rank reached on the day's wheel and publish it when it is
    /// the first. The state sends each rank once per session; the table's
    /// UNIQUE is the gate across sessions.
    pub fn bank_task(
        &self,
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        score: i32,
    ) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Bank {
                user_id,
                difficulty,
                puzzle_date,
                score,
            })
            .is_err()
        {
            tracing::error!("failed to enqueue word wheel rank");
        }
    }

    pub(crate) async fn flush_game_saves(&self) -> Result<()> {
        let (done_tx, done_rx) = oneshot::channel();
        self.game_save_sender()
            .send(GameSaveCommand::Flush(done_tx))
            .map_err(|_| anyhow!("word wheel game save queue is closed"))?;
        done_rx
            .await
            .context("word wheel game save worker stopped before flush")?;
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
        score: i32,
    ) -> Result<()> {
        record_win_and_publish(
            &self.db,
            &self.activity_feed,
            user_id,
            difficulty,
            puzzle_date,
            score,
        )
        .await
    }
}

/// The one writer for every session's words and ranks, so a player's saves
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
                    tracing::error!(error = ?error, "failed to save word wheel game state");
                }
            }
            GameSaveCommand::Bank {
                user_id,
                difficulty,
                puzzle_date,
                score,
            } => {
                if let Err(error) = record_win_and_publish(
                    &db,
                    &activity_feed,
                    user_id,
                    difficulty,
                    puzzle_date,
                    score,
                )
                .await
                {
                    tracing::error!(error = ?error, "failed to record word wheel rank");
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
    score: i32,
) -> Result<()> {
    let client = db.get().await?;
    let result = DailyWin::record_win(&client, user_id, difficulty, puzzle_date, score).await?;
    if !result.fresh {
        return Ok(());
    }

    let username = fetch_username(&client, user_id).await;
    let _ = activity_feed.send(ActivityEvent::game_won_at(
        user_id,
        username,
        activity_game(DailyPuzzle::WordWheel),
        Some(difficulty.key().to_string()),
        Some(score),
        ActivityEvent::occurred_on_utc_date(puzzle_date),
    ));
    Ok(())
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
