use anyhow::{Context, Result};
use chrono::NaiveDate;
use late_core::{
    db::Db,
    models::{
        leaderboard::{DailyPuzzle, SINGLE_TIER_KEY},
        profile::fetch_username,
        quartets::{DailyWall, DailyWin, Game, GameParams},
    },
};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::state::{Puzzle, choose_for_date, find_puzzle, pack};
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

/// The day's wall, claimed in the database and found in the pack.
#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub puzzle_date: NaiveDate,
    pub puzzle: &'static Puzzle,
}

#[derive(Clone)]
pub struct QuartetsService {
    db: Db,
    saves: SaveQueue<GameSaveCommand>,
}

enum GameSaveCommand {
    Save(GameParams),
    /// A solved wall: the win is recorded first and the solved board saved
    /// behind it, so a win that fails to record leaves the wall one try
    /// short instead of finished with no win.
    Complete {
        params: GameParams,
        strikes: i32,
    },
}

impl QuartetsService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            saves: SaveQueue::new(DailyPuzzle::Quartets, db.clone(), activity_feed),
            db,
        }
    }

    /// A wall ended on this session, solved or out of strikes. Counted for
    /// the dashboard, nothing stored: the win itself goes through
    /// `complete_game_task`.
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
        self.saves.settle().await;
        let client = self.db.get().await?;
        Game::find_for_date(&client, user_id, puzzle_date).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        self.saves.send(GameSaveCommand::Save(params));
    }

    /// Record a solved wall and save the solved board behind it, publishing
    /// the win when it is the first. The state sends it once per session;
    /// the table's UNIQUE is the gate across sessions.
    pub fn complete_game_task(&self, params: GameParams, strikes: i32) {
        self.saves
            .send(GameSaveCommand::Complete { params, strikes });
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
                    tracing::error!(error = ?error, "failed to save quartets game state");
                }
            }
            Self::Complete { params, strikes } => {
                match record_win_and_publish(
                    db,
                    activity_feed,
                    params.user_id,
                    params.puzzle_date,
                    strikes,
                )
                .await
                {
                    Ok(()) => {
                        if let Err(error) = save_game(db, params).await {
                            tracing::error!(
                                error = ?error,
                                "failed to save solved quartets wall after recording win"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            error = ?error,
                            "failed to record quartets win; solved wall was not persisted"
                        );
                    }
                }
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
