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
use tokio::sync::{broadcast, oneshot};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use super::state::{self, Outcome, Puzzle, choose_for_date, find_puzzle, pack};
use crate::{
    app::{
        activity::event::ActivityEvent,
        arcade::{
            daily::activity_game,
            daily_load,
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

/// Today's wall with this user's saved tries on it, as `load_today` hands
/// it over.
#[derive(Debug)]
pub struct Today {
    pub daily: Daily,
    pub saved: Option<Game>,
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

    /// Today's wall and this user's tries on it, read for the wall's own
    /// date so a load across midnight never pairs a wall with another day's
    /// tries. Logged and counted by `daily_load`; `None` when it failed.
    pub async fn load_today(&self, user_id: Uuid) -> Option<Today> {
        daily_load::load(DailyPuzzle::Quartets, self.clone().fetch_today(user_id)).await
    }

    /// [`Self::load_today`] off the tick, for a board left without a wall.
    pub fn load_today_task(&self, user_id: Uuid) -> oneshot::Receiver<Option<Today>> {
        daily_load::load_task(DailyPuzzle::Quartets, self.clone().fetch_today(user_id))
    }

    async fn fetch_today(self, user_id: Uuid) -> Result<Today> {
        let daily = self
            .ensure_daily_wall()
            .await
            .context("claiming today's quartets wall")?;
        let saved = self
            .load_game(user_id, daily.puzzle_date)
            .await
            .context("loading the saved quartets tries")?;
        Ok(Today { daily, saved })
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
                let user_id = params.user_id;
                match save_game(db, params).await {
                    Ok(Saved::Stored) => {}
                    Ok(Saved::KeptLost) => tracing::info!(
                        %user_id,
                        "kept a quartets wall another session ran out of strikes on"
                    ),
                    Err(error) => {
                        tracing::error!(error = ?error, "failed to save quartets game state");
                    }
                }
            }
            Self::Complete { params, strikes } => {
                let user_id = params.user_id;
                match complete(db, activity_feed, params, strikes).await {
                    Ok(Completion::Recorded) => {}
                    Ok(Completion::AlreadyLost) => tracing::warn!(
                        %user_id,
                        "refused a quartets win over a wall another session ran out of strikes on"
                    ),
                    Err(error) => tracing::error!(
                        error = ?error,
                        "failed to record quartets win; solved wall was not persisted"
                    ),
                }
            }
        }
    }
}

/// What a `Save` came to.
enum Saved {
    Stored,
    /// Another session had already run the wall out of strikes, so it
    /// stays lost: a stale session's longer list must not replace it.
    KeptLost,
}

/// What a `Complete` came to.
enum Completion {
    /// The win is recorded and the solved wall saved behind it.
    Recorded,
    /// Another session had already run the wall out of strikes, so there is
    /// no win and nothing is saved.
    AlreadyLost,
}

/// How the stored wall of the same day and puzzle stands, locked until `tx`
/// ends so no other session's save lands in between.
async fn stored_outcome(tx: &impl GenericClient, params: &GameParams) -> Result<Outcome> {
    match Game::lock_for_date(tx, params.user_id, params.puzzle_date).await? {
        Some(stored) if stored.puzzle_id == params.puzzle_id => {
            Ok(state::stored_outcome(&stored.tries))
        }
        Some(_) | None => Ok(Outcome::Playing),
    }
}

async fn save_game(db: &Db, params: GameParams) -> Result<Saved> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    match stored_outcome(&*tx, &params).await? {
        Outcome::OutOfStrikes => return Ok(Saved::KeptLost),
        Outcome::Playing | Outcome::Solved => {}
    }
    Game::upsert(&*tx, params).await?;
    tx.commit().await?;
    Ok(Saved::Stored)
}

/// Record a solved wall and save it, in one transaction: a win that fails
/// to record leaves the wall one try short, and a wall another session
/// already lost takes neither. The win is published once committed, and
/// only the day's first.
async fn complete(
    db: &Db,
    activity_feed: &broadcast::Sender<ActivityEvent>,
    params: GameParams,
    strikes: i32,
) -> Result<Completion> {
    let (user_id, puzzle_date) = (params.user_id, params.puzzle_date);
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    match stored_outcome(&*tx, &params).await? {
        Outcome::OutOfStrikes => return Ok(Completion::AlreadyLost),
        Outcome::Playing | Outcome::Solved => {}
    }
    let record = DailyWin::record_win(&*tx, user_id, puzzle_date, strikes)
        .await
        .context("recording the win")?;
    Game::upsert(&*tx, params)
        .await
        .context("saving the solved wall")?;
    tx.commit().await?;

    if record.fresh {
        let username = fetch_username(&client, user_id).await;
        let _ = activity_feed.send(ActivityEvent::game_won_at(
            user_id,
            username,
            activity_game(DailyPuzzle::Quartets),
            Some(SINGLE_TIER_KEY.to_string()),
            None,
            ActivityEvent::occurred_on_utc_date(puzzle_date),
        ));
    }
    Ok(Completion::Recorded)
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
