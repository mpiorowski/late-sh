use std::sync::{Arc, OnceLock};

use anyhow::{Context, Result, anyhow};
use chrono::NaiveDate;
use late_core::{
    db::Db,
    models::{
        chips::Difficulty,
        profile::fetch_username,
        sliding_puzzle::{DailyWin, Game, GameParams},
    },
};
use tokio::sync::{broadcast, mpsc, oneshot};
use uuid::Uuid;

use super::state::{daily_seed, solved_board};
use crate::app::activity::event::{ActivityEvent, ActivityGame};

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

    pub async fn load_games(&self, user_id: Uuid) -> Result<Vec<Game>> {
        // The barrier only orders queued writes ahead of this read. If it
        // cannot answer, the database is still authoritative — returning an
        // error here would hand bootstrap an empty restore that overwrites
        // every persisted board on the next save.
        if let Err(error) = self.flush_game_saves().await {
            tracing::error!(
                error = ?error,
                "Sliding Puzzle save queue flush failed; restoring from the database anyway"
            );
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

    pub fn complete_game_task(
        &self,
        params: GameParams,
        difficulty: Difficulty,
        puzzle_date: NaiveDate,
        moves: i32,
    ) {
        let canonical_seed = daily_seed(puzzle_date, difficulty) as i64;
        let canonical_tiles: Vec<i32> = solved_board(difficulty)
            .into_iter()
            .map(i32::from)
            .collect();
        if params.mode != "daily"
            || params.puzzle_date != Some(puzzle_date)
            || params.difficulty_key != difficulty.key()
            || params.moves != moves
            || moves <= 0
            || params.puzzle_seed != canonical_seed
            || params.tiles != canonical_tiles
        {
            tracing::error!(
                mode = %params.mode,
                params_date = ?params.puzzle_date,
                completion_date = %puzzle_date,
                params_difficulty = %params.difficulty_key,
                completion_difficulty = difficulty.key(),
                params_moves = params.moves,
                completion_moves = moves,
                params_seed = params.puzzle_seed,
                canonical_seed,
                solved = params.tiles == canonical_tiles,
                "rejected inconsistent Sliding Puzzle completion"
            );
            return;
        }
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
