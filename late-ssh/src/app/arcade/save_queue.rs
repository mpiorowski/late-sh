//! The process-wide save queue behind a daily's `svc.rs`. Every session's
//! saves and wins for one game go through one FIFO worker, so a player's
//! writes land in the order they were made, and a login can wait for the
//! queue to drain before it reads the database. What each command does is the
//! game's own `SaveCommand::run`; the transport, the flush barrier and the
//! timeout rule live here once.

use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use late_core::{db::Db, models::leaderboard::DailyPuzzle};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::app::activity::event::ActivityEvent;

/// How long a session load waits for the queue to drain before reading the
/// database anyway. The queue is process-wide, so without a bound one
/// player's login would sit behind every other player's queued saves.
const LOAD_FLUSH_TIMEOUT: Duration = Duration::from_secs(2);

/// One game's closed set of queued writes. `run` logs its own failures: the
/// queue is fire-and-forget, so nobody upstream sees them.
pub trait SaveCommand: Send + 'static {
    fn run(
        self,
        db: &Db,
        activity_feed: &broadcast::Sender<ActivityEvent>,
    ) -> impl Future<Output = ()> + Send;
}

enum Message<C> {
    Run(C),
    Flush(oneshot::Sender<()>),
}

pub struct SaveQueue<C> {
    puzzle: DailyPuzzle,
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    /// Started on the first send, so a service is built without a runtime.
    tx: Arc<OnceLock<mpsc::UnboundedSender<Message<C>>>>,
}

impl<C> Clone for SaveQueue<C> {
    fn clone(&self) -> Self {
        Self {
            puzzle: self.puzzle,
            db: self.db.clone(),
            activity_feed: self.activity_feed.clone(),
            tx: Arc::clone(&self.tx),
        }
    }
}

impl<C: SaveCommand> SaveQueue<C> {
    pub fn new(
        puzzle: DailyPuzzle,
        db: Db,
        activity_feed: broadcast::Sender<ActivityEvent>,
    ) -> Self {
        Self {
            puzzle,
            db,
            activity_feed,
            tx: Arc::new(OnceLock::new()),
        }
    }

    /// Queue a command behind every one sent before it.
    pub fn send(&self, command: C) {
        if self.sender().send(Message::Run(command)).is_err() {
            tracing::error!(game = self.puzzle.key(), "failed to enqueue arcade save");
        }
    }

    /// Wait until every command queued so far has run.
    pub async fn flush(&self) -> Result<()> {
        let (done_tx, done_rx) = oneshot::channel();
        self.sender()
            .send(Message::Flush(done_tx))
            .map_err(|_| anyhow!("{} save queue is closed", self.puzzle.key()))?;
        done_rx
            .await
            .with_context(|| format!("{} save worker stopped before flush", self.puzzle.key()))
    }

    /// Order the queued writes ahead of a load's read. The barrier only
    /// orders: when it cannot answer in time the database is still
    /// authoritative, so the failure is logged and the read goes ahead.
    /// Failing the load instead would hand the session an empty restore that
    /// overwrites the stored boards on its next save.
    pub async fn settle(&self) {
        match tokio::time::timeout(LOAD_FLUSH_TIMEOUT, self.flush()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(
                    error = ?error,
                    game = self.puzzle.key(),
                    "save queue flush failed; loading from the database anyway"
                );
            }
            Err(_elapsed) => {
                tracing::error!(
                    timeout_secs = LOAD_FLUSH_TIMEOUT.as_secs(),
                    game = self.puzzle.key(),
                    "save queue flush timed out; loading from the database anyway"
                );
            }
        }
    }

    fn sender(&self) -> &mpsc::UnboundedSender<Message<C>> {
        self.tx.get_or_init(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            tokio::spawn(run_worker(self.db.clone(), self.activity_feed.clone(), rx));
            tx
        })
    }
}

async fn run_worker<C: SaveCommand>(
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    mut rx: mpsc::UnboundedReceiver<Message<C>>,
) {
    while let Some(message) = rx.recv().await {
        match message {
            Message::Run(command) => command.run(&db, &activity_feed).await,
            Message::Flush(done_tx) => {
                let _ = done_tx.send(());
            }
        }
    }
}
