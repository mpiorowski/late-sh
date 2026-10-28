use anyhow::{Context, Result};
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
use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

use super::art::PuzzleArt;
use crate::{
    app::{
        activity::event::ActivityEvent,
        arcade::{
            daily::activity_game,
            save_queue::{SaveCommand, SaveQueue},
        },
    },
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

#[derive(Clone)]
pub struct SlidingPuzzleService {
    db: Db,
    saves: SaveQueue<GameSaveCommand>,
}

enum GameSaveCommand {
    Save(GameParams),
    Complete {
        params: GameParams,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        moves: i32,
    },
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
            saves: SaveQueue::new(DailyPuzzle::SlidingPuzzle, db.clone(), activity_feed),
            db,
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
        self.saves.settle().await;
        let client = self.db.get().await?;
        Game::list_by_user_id(&client, user_id).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        self.saves.send(GameSaveCommand::Save(params));
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
        self.saves.send(GameSaveCommand::Complete {
            params,
            difficulty,
            puzzle_date,
            moves,
        });
    }

    #[cfg(test)]
    pub(crate) async fn flush_game_saves(&self) -> Result<()> {
        self.saves.flush().await
    }
}

impl SaveCommand for GameSaveCommand {
    async fn run(self, db: &Db, activity_feed: &broadcast::Sender<ActivityEvent>) {
        match self {
            Self::Save(params) => {
                if let Err(error) = save_game(db, params).await {
                    tracing::error!(error = ?error, "failed to save Sliding Puzzle game state");
                }
            }
            Self::Complete {
                params,
                difficulty,
                puzzle_date,
                moves,
            } => {
                let user_id = params.user_id;
                match record_win_and_publish(
                    db,
                    activity_feed,
                    user_id,
                    difficulty,
                    puzzle_date,
                    moves,
                )
                .await
                {
                    Ok(()) => {
                        if let Err(error) = save_game(db, params).await {
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
        activity_game(DailyPuzzle::SlidingPuzzle),
        Some(difficulty.key().to_string()),
        Some(moves),
        ActivityEvent::occurred_on_utc_date(puzzle_date),
    ));
    Ok(())
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
