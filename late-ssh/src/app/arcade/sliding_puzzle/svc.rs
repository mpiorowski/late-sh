use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use late_core::{
    db::Db,
    models::{
        artboard_piece::ArtboardPiece,
        chips::Difficulty,
        leaderboard::DailyPuzzle,
        profile::fetch_username,
        sliding_puzzle::{DailyWin, Game, GameParams},
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::art::PuzzleArt;
use crate::{
    app::activity::event::{ActivityEvent, ActivityGame},
    metrics::{self, ArcadeDifficulty, ArcadeFinish, ArcadeMode},
};

/// How one day's art load ended, for `metrics::record_sliding_puzzle_art`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlidingPuzzleArtLoad {
    Featured,
    Empty,
    Failed,
}

/// What the state gets back from [`SlidingPuzzleService::load_daily_art_task`].
/// The failure is already logged and counted by the task; the state only
/// needs to know to fall back and retry later.
pub enum ArtLoad {
    Featured(PuzzleArt),
    Empty,
    Failed,
}

/// How long a session load waits for the shared save queue to drain before
/// reading the database anyway. The queue is process-wide, so without a bound
/// one player's login would sit behind every other player's queued moves.
pub(crate) const LOAD_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct SlidingPuzzleService {
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
        moves: i32,
    },
    Flush(oneshot::Sender<()>),
}

impl SlidingPuzzleService {
    /// A board ended on this session. Counted for the dashboard, nothing
    /// stored: the daily win itself goes through `record_win_task`.
    pub fn record_finish(
        &self,
        mode: ArcadeMode,
        difficulty: ArcadeDifficulty,
        finish: ArcadeFinish,
    ) {
        metrics::record_arcade_finish(DailyPuzzle::SlidingPuzzle, mode, difficulty, finish);
    }

    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            db,
            activity_feed,
            game_save_tx: Arc::new(OnceLock::new()),
        }
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    /// The gallery piece featured on `day`, claimed for the day if nobody
    /// has yet (`ArtboardPiece::feature_for_day`), decoded off the tick.
    /// The task owns the load's logging and metrics; the receiver gets a
    /// tagged outcome.
    pub(crate) fn load_daily_art_task(&self, day: NaiveDate) -> oneshot::Receiver<ArtLoad> {
        let (tx, rx) = oneshot::channel();
        let db = self.db.clone();
        tokio::spawn(async move {
            let load = match load_daily_art(&db, day).await {
                Ok(Some(art)) => {
                    metrics::record_sliding_puzzle_art(SlidingPuzzleArtLoad::Featured);
                    ArtLoad::Featured(art)
                }
                Ok(None) => {
                    metrics::record_sliding_puzzle_art(SlidingPuzzleArtLoad::Empty);
                    ArtLoad::Empty
                }
                Err(error) => {
                    tracing::warn!(error = ?error, %day, "failed to load Sliding Puzzle art");
                    metrics::record_sliding_puzzle_art(SlidingPuzzleArtLoad::Failed);
                    ArtLoad::Failed
                }
            };
            let _ = tx.send(load);
        });
        rx
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
                    "Sliding Puzzle save queue flush failed; restoring from the database anyway"
                );
            }
            Err(_elapsed) => {
                tracing::error!(
                    timeout_secs = LOAD_FLUSH_TIMEOUT.as_secs(),
                    "Sliding Puzzle save queue flush timed out; restoring from the database anyway"
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
            tracing::error!("failed to enqueue Sliding Puzzle game state save");
        }
    }

    /// Record a solved daily and persist the solved board behind it. The
    /// state only calls this from the finishing move of a daily board, so the
    /// params are trusted; the table's own gates (`moves > 0`, one win per
    /// user/date/difficulty) are the checks that remain.
    pub fn complete_game_task(
        &self,
        params: GameParams,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        moves: i32,
    ) {
        if self
            .game_save_sender()
            .send(GameSaveCommand::Complete {
                params,
                difficulty,
                puzzle_date,
                moves,
            })
            .is_err()
        {
            tracing::error!("failed to enqueue Sliding Puzzle completed move");
        }
    }

    pub(crate) async fn flush_game_saves(&self) -> Result<()> {
        let (done_tx, done_rx) = oneshot::channel();
        self.game_save_sender()
            .send(GameSaveCommand::Flush(done_tx))
            .map_err(|_| anyhow!("Sliding Puzzle game save queue is closed"))?;
        done_rx
            .await
            .context("Sliding Puzzle game save worker stopped before flush")?;
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
        moves: i32,
    ) -> Result<()> {
        record_win_and_publish(
            &self.db,
            &self.activity_feed,
            user_id,
            difficulty,
            puzzle_date,
            moves,
        )
        .await
    }
}

async fn run_game_save_worker(
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    mut save_rx: mpsc::UnboundedReceiver<GameSaveCommand>,
) {
    while let Some(command) = save_rx.recv().await {
        match command {
            GameSaveCommand::Save(params) => {
                if let Err(error) = save_game(&db, params).await {
                    tracing::error!(error = ?error, "failed to save Sliding Puzzle game state");
                }
            }
            GameSaveCommand::Complete {
                params,
                difficulty,
                puzzle_date,
                moves,
            } => {
                let user_id = params.user_id;
                match record_win_and_publish(
                    &db,
                    &activity_feed,
                    user_id,
                    difficulty,
                    puzzle_date,
                    moves,
                )
                .await
                {
                    Ok(()) => {
                        if let Err(error) = save_game(&db, params).await {
                            tracing::error!(
                                error = ?error,
                                "failed to save completed Sliding Puzzle state after recording win"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            error = ?error,
                            "failed to record Sliding Puzzle win; solved state was not persisted"
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

async fn load_daily_art(db: &Db, day: NaiveDate) -> Result<Option<PuzzleArt>> {
    let client = db.get().await?;
    let Some(piece) = ArtboardPiece::feature_for_day(&client, day).await? else {
        return Ok(None);
    };
    let canvas = serde_json::from_value(piece.canvas)
        .with_context(|| format!("decoding canvas of artboard piece {}", piece.id))?;
    let width = usize::try_from(piece.width).context("artboard piece width")?;
    let height = usize::try_from(piece.height).context("artboard piece height")?;
    Ok(Some(PuzzleArt {
        title: piece.title,
        username: piece.username,
        canvas,
        width,
        height,
    }))
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
    moves: i32,
) -> Result<()> {
    let client = db.get().await?;
    let result = DailyWin::record_win(&client, user_id, difficulty, puzzle_date, moves).await?;
    if !result.fresh {
        return Ok(());
    }

    let username = fetch_username(&client, user_id).await;
    let _ = activity_feed.send(ActivityEvent::game_won_at(
        user_id,
        username,
        ActivityGame::SlidingPuzzle,
        Some(difficulty.key().to_string()),
        Some(moves),
        ActivityEvent::occurred_on_utc_date(puzzle_date),
    ));
    Ok(())
}
