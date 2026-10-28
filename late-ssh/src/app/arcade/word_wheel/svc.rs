use anyhow::{Context, Result};
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
use tokio::sync::broadcast;
use uuid::Uuid;

use super::state::{find_wheel, pack};
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

/// The day's wheel, claimed in the database and found in the pack.
#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub puzzle_date: NaiveDate,
    pub wheel: &'static WordWheel,
}

#[derive(Clone)]
pub struct WordWheelService {
    db: Db,
    saves: SaveQueue<GameSaveCommand>,
}

enum GameSaveCommand {
    Save(GameParams),
    Bank {
        user_id: Uuid,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        score: i32,
    },
}

impl WordWheelService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        Self {
            saves: SaveQueue::new(DailyPuzzle::WordWheel, db.clone(), activity_feed),
            db,
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
        self.saves.settle().await;
        let client = self.db.get().await?;
        Game::find_for_date(&client, user_id, puzzle_date).await
    }

    pub fn save_game_task(&self, params: GameParams) {
        self.saves.send(GameSaveCommand::Save(params));
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
        self.saves.send(GameSaveCommand::Bank {
            user_id,
            difficulty,
            puzzle_date,
            score,
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
                    tracing::error!(error = ?error, "failed to save word wheel game state");
                }
            }
            Self::Bank {
                user_id,
                difficulty,
                puzzle_date,
                score,
            } => {
                if let Err(error) = record_win_and_publish(
                    db,
                    activity_feed,
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
