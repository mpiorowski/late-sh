use std::sync::{Arc, OnceLock};
use std::time::Duration;

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

use crate::app::activity::event::{ActivityEvent, ActivityGame};

/// How long a session load waits for the shared save queue to drain before
/// reading the database anyway. The queue is process-wide, so without a bound
/// one player's login would sit behind every other player's queued moves.
pub(crate) const LOAD_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct SlidingPuzzleService {
    pub(crate) dev_art_preview: bool,
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    game_save_tx: Arc<OnceLock<mpsc::UnboundedSender<GameSaveCommand>>>,
}

enum GameSaveCommand {
    Save(GameParams),
    SaveImageMode {
        user_id: Uuid,
        enabled: bool,
    },
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
            dev_art_preview: false,
            db,
            activity_feed,
            game_save_tx: Arc::new(OnceLock::new()),
        }
    }

    pub fn with_dev_art_preview(mut self, enabled: bool) -> Self {
        self.dev_art_preview = enabled;
        self
    }

    /// Preview tomorrow's approved pool locally without changing the shared
    /// daily assignment, its availability dates, or any player's saved board.
    pub(crate) async fn next_preview_artwork(
        &self,
        current_id: Option<Uuid>,
    ) -> Result<late_core::models::sliding_puzzle_artwork::Artwork> {
        anyhow::ensure!(
            self.dev_art_preview,
            "art preview is only available in development"
        );
        let client = self.db.get().await?;
        let row = client
            .query_one(
                "SELECT id, title, credit, image_url, embedded_key
             FROM sliding_puzzle_artworks
             WHERE active AND approved_at IS NOT NULL
             ORDER BY CASE WHEN (created, id) >
                 (SELECT created, id FROM sliding_puzzle_artworks WHERE id = $1)
                 THEN 0 ELSE 1 END, created, id
             LIMIT 1",
                &[&current_id],
            )
            .await?;
        Ok(row.into())
    }

    pub(crate) async fn daily_artwork(
        &self,
        date: NaiveDate,
    ) -> Result<late_core::models::sliding_puzzle_artwork::Artwork> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let artwork =
            late_core::models::sliding_puzzle_artwork::Artwork::assign_daily(&tx, date).await?;
        if let Some(key) = &artwork.embedded_key {
            anyhow::ensure!(
                super::artwork::embedded_index(key).is_some(),
                "unknown embedded artwork"
            );
        } else {
            anyhow::ensure!(artwork.image_url.is_some(), "community artwork has no URL");
        }
        tx.commit().await?;
        Ok(artwork)
    }

    pub fn today(&self) -> NaiveDate {
        chrono::Utc::now().date_naive()
    }

    pub async fn load_image_mode(&self, user_id: Uuid) -> Result<bool> {
        // Read after prior toggles settle, including an immediate reconnect.
        tokio::time::timeout(LOAD_FLUSH_TIMEOUT, self.flush_game_saves())
            .await
            .context("timed out waiting for puzzle preference saves")??;
        let client = self.db.get().await?;
        let user = late_core::models::user::User::get(&client, user_id)
            .await?
            .context("user not found")?;
        Ok(late_core::models::user::extract_sliding_puzzle_image_mode(
            &user.settings,
        ))
    }

    pub(crate) fn save_image_mode_task(&self, user_id: Uuid, enabled: bool) {
        // The existing FIFO keeps quick i/i toggles from finishing backwards.
        if self
            .game_save_sender()
            .send(GameSaveCommand::SaveImageMode { user_id, enabled })
            .is_err()
        {
            tracing::error!("failed to enqueue Sliding Puzzle image preference");
        }
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
            GameSaveCommand::SaveImageMode { user_id, enabled } => {
                let result = async {
                    let client = db.get().await?;
                    late_core::models::user::User::set_sliding_puzzle_image_mode(
                        &client, user_id, enabled,
                    )
                    .await
                }
                .await;
                if let Err(error) = result {
                    tracing::error!(error = ?error, "failed to save Sliding Puzzle image preference");
                }
            }
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
