use anyhow::{Context, Result};
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
use tokio::sync::broadcast;
use uuid::Uuid;

use super::state::{DIFFICULTIES, find_puzzle, pack};
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
    saves: SaveQueue<GameSaveCommand>,
}

enum GameSaveCommand {
    Save(GameParams),
    Complete {
        params: GameParams,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        mistakes: i32,
    },
}

impl ChessPuzzleService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            saves: SaveQueue::new(DailyPuzzle::ChessPuzzle, db.clone(), activity_feed),
            db,
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
        self.saves.settle().await;
        let client = self.db.get().await?;
        Game::list_by_user_id(&client, user_id).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        self.saves.send(GameSaveCommand::Save(params));
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
        self.saves.send(GameSaveCommand::Complete {
            params,
            difficulty,
            puzzle_date,
            mistakes,
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
                    tracing::error!(error = ?error, "failed to save chess puzzle game state");
                }
            }
            Self::Complete {
                params,
                difficulty,
                puzzle_date,
                mistakes,
            } => {
                let user_id = params.user_id;
                match record_win_and_publish(
                    db,
                    activity_feed,
                    user_id,
                    difficulty,
                    puzzle_date,
                    mistakes,
                )
                .await
                {
                    Ok(()) => {
                        if let Err(error) = save_game(db, params).await {
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
