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
use tokio::sync::{broadcast, oneshot};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use super::state::{DIFFICULTIES, find_puzzle, out_of_lives, pack};
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

/// The day's puzzles, claimed in the database and found in the pack, one
/// per tier in `DIFFICULTIES` order.
#[derive(Clone, Copy, Debug)]
pub struct Daily {
    pub puzzle_date: NaiveDate,
    pub puzzles: [&'static ChessPuzzle; 3],
}

/// Today's puzzles with this user's saved rows, as `load_today` hands them
/// over. The state takes only the daily rows that belong to the puzzles.
#[derive(Debug)]
pub struct Today {
    pub daily: Daily,
    pub games: Vec<Game>,
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

    /// Today's puzzles and this user's saved rows. Logged and counted by
    /// `daily_load`; `None` when it failed.
    pub async fn load_today(&self, user_id: Uuid) -> Option<Today> {
        daily_load::load(DailyPuzzle::ChessPuzzle, self.clone().fetch_today(user_id)).await
    }

    /// [`Self::load_today`] off the tick, for daily boards left without
    /// puzzles.
    pub fn load_today_task(&self, user_id: Uuid) -> oneshot::Receiver<Option<Today>> {
        daily_load::load_task(DailyPuzzle::ChessPuzzle, self.clone().fetch_today(user_id))
    }

    async fn fetch_today(self, user_id: Uuid) -> Result<Today> {
        let daily = self
            .ensure_daily_puzzles()
            .await
            .context("claiming today's chess puzzles")?;
        let games = self
            .load_games(user_id)
            .await
            .context("loading the saved chess puzzle games")?;
        Ok(Today { daily, games })
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
                let user_id = params.user_id;
                match save_game(db, params).await {
                    Ok(Saved::Stored) => {}
                    Ok(Saved::KeptLost) => tracing::info!(
                        %user_id,
                        "kept a chess puzzle another session ran out of lives on"
                    ),
                    Err(error) => {
                        tracing::error!(error = ?error, "failed to save chess puzzle game state");
                    }
                }
            }
            Self::Complete {
                params,
                difficulty,
                puzzle_date,
                mistakes,
            } => {
                let user_id = params.user_id;
                match complete(db, activity_feed, params, difficulty, puzzle_date, mistakes).await {
                    Ok(Completion::Recorded) => {}
                    Ok(Completion::AlreadyLost) => tracing::warn!(
                        %user_id,
                        "refused a chess puzzle win over a puzzle another session ran out of lives on"
                    ),
                    Err(error) => tracing::error!(
                        error = ?error,
                        "failed to record chess puzzle win; solved state was not persisted"
                    ),
                }
            }
        }
    }
}

/// What a `Save` came to.
enum Saved {
    Stored,
    /// Another session had already run the day's puzzle out of lives, so it
    /// stays lost: a stale session's row must not hand the lives back.
    KeptLost,
}

/// What a `Complete` came to.
enum Completion {
    /// The win is recorded and the solved board saved behind it.
    Recorded,
    /// Another session had already run the puzzle out of lives, so there is
    /// no win and nothing is saved.
    AlreadyLost,
}

/// Whether the stored row of the slot is this same daily puzzle run out of
/// lives, locked until `tx` ends so no other session's save lands in
/// between. Personal rows never pay, so they are never held.
async fn stored_daily_lost(tx: &impl GenericClient, params: &GameParams) -> Result<bool> {
    let Some(puzzle_date) = params.puzzle_date else {
        return Ok(false);
    };
    match Game::lock_slot(tx, params.user_id, &params.difficulty_key, &params.mode).await? {
        Some(stored) => Ok(stored.puzzle_date == Some(puzzle_date)
            && stored.puzzle_id == params.puzzle_id
            && out_of_lives(stored.mistake_plies.len())),
        None => Ok(false),
    }
}

async fn save_game(db: &Db, params: GameParams) -> Result<Saved> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    if stored_daily_lost(&*tx, &params).await? {
        return Ok(Saved::KeptLost);
    }
    Game::upsert(&*tx, params).await?;
    tx.commit().await?;
    Ok(Saved::Stored)
}

/// Record a solved daily and save the solved board, in one transaction: a
/// win that fails to record leaves the board unsolved, and a puzzle another
/// session already lost takes neither. The win is published once committed,
/// and only the day's first.
async fn complete(
    db: &Db,
    activity_feed: &broadcast::Sender<ActivityEvent>,
    params: GameParams,
    difficulty: Difficulty,
    puzzle_date: NaiveDate,
    mistakes: i32,
) -> Result<Completion> {
    let user_id = params.user_id;
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    if stored_daily_lost(&*tx, &params).await? {
        return Ok(Completion::AlreadyLost);
    }
    let record = DailyWin::record_win(&*tx, user_id, difficulty, puzzle_date, mistakes)
        .await
        .context("recording the win")?;
    Game::upsert(&*tx, params)
        .await
        .context("saving the solved board")?;
    tx.commit().await?;

    if record.fresh {
        let username = fetch_username(&client, user_id).await;
        let _ = activity_feed.send(ActivityEvent::game_won_at(
            user_id,
            username,
            activity_game(DailyPuzzle::ChessPuzzle),
            Some(difficulty.key().to_string()),
            Some(mistakes),
            ActivityEvent::occurred_on_utc_date(puzzle_date),
        ));
    }
    Ok(Completion::Recorded)
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
