use anyhow::Result;
use chrono::NaiveDate;
use late_core::{
    db::Db,
    models::{
        chips::Difficulty,
        leaderboard::DailyPuzzle,
        netwalk::{DailyWin, Game, GameParams},
        profile::fetch_username,
    },
};
use tokio::sync::broadcast;
use uuid::Uuid;

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

#[derive(Clone)]
pub struct NetwalkService {
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

impl NetwalkService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            saves: SaveQueue::new(DailyPuzzle::Netwalk, db.clone(), activity_feed),
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
        metrics::record_arcade_finish(DailyPuzzle::Netwalk, mode, difficulty, finish);
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
    /// state only calls this from the finishing turn of a daily board, so the
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
                    tracing::error!(error = ?error, "failed to save netwalk game state");
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
                                "failed to save completed netwalk state after recording win"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(
                            error = ?error,
                            "failed to record netwalk win; solved state was not persisted"
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
        activity_game(DailyPuzzle::Netwalk),
        Some(difficulty.key().to_string()),
        Some(moves),
        ActivityEvent::occurred_on_utc_date(puzzle_date),
    ));
    Ok(())
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
