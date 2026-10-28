//! Loading today's puzzle for a daily whose puzzle is claimed in the
//! database (Le Word, Chess Puzzle, Word Wheel, Quartets), together with the
//! player's saved progress on it: both or neither, because a board shown
//! without its saved progress would take a fresh run over it. One load runs
//! at connect (`session_bootstrap`), and another whenever a board is left
//! without a puzzle: the connect-time load failed, or the UTC rollover
//! cleared yesterday's. Each such state holds a [`DailyLoad`] and walks it
//! from its `poll_load` on every tick (`daily.rs::poll_daily_loads`), board
//! open or not, since a board with no puzzle has no move to interrupt. A
//! failed load waits [`LOAD_RETRY`] before the next. Every load is logged
//! and counted here, once for all four games.

use std::{
    future::Future,
    time::{Duration, Instant},
};

use anyhow::Result;
use late_core::models::leaderboard::DailyPuzzle;
use tokio::sync::oneshot;

use crate::metrics::{self, ArcadeDailyLoad};

/// How long a failed load waits before the next attempt.
pub const LOAD_RETRY: Duration = Duration::from_secs(30);

/// Run one load of today's `puzzle`, logging and counting how it ended. The
/// caller only learns whether it landed.
pub async fn load<T>(puzzle: DailyPuzzle, load: impl Future<Output = Result<T>>) -> Option<T> {
    match load.await {
        Ok(today) => {
            metrics::record_arcade_daily_load(puzzle, ArcadeDailyLoad::Loaded);
            Some(today)
        }
        Err(error) => {
            tracing::error!(error = ?error, game = puzzle.key(), "failed to load today's daily");
            metrics::record_arcade_daily_load(puzzle, ArcadeDailyLoad::Failed);
            None
        }
    }
}

/// [`load`] off the tick, for a state's `poll_load` to collect.
pub fn load_task<T: Send + 'static>(
    puzzle: DailyPuzzle,
    today: impl Future<Output = Result<T>> + Send + 'static,
) -> oneshot::Receiver<Option<T>> {
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send(load(puzzle, today).await);
    });
    rx
}

/// A board's load: nothing pending, in flight, or waiting out a failure.
pub enum DailyLoad<T> {
    Idle,
    Loading(oneshot::Receiver<Option<T>>),
    Failed { retry_after: Instant },
}

/// What one [`DailyLoad::poll`] saw.
pub enum Polled<T> {
    /// Nothing landed: idle, still in flight, or waiting out a failure.
    Waiting,
    Loaded(T),
    Failed,
}

impl<T> DailyLoad<T> {
    /// The load a state starts from after the connect-time one: nothing
    /// pending when it landed, the retry wait when it did not.
    pub fn after_connect(landed: bool) -> Self {
        match landed {
            true => Self::Idle,
            false => Self::Failed {
                retry_after: Instant::now() + LOAD_RETRY,
            },
        }
    }

    /// Report what landed since the last poll, and start a load from `start`
    /// when the board `needs` one and none is in flight or waiting. Pure
    /// state tests run without a runtime, so there nothing starts.
    pub fn poll(
        &mut self,
        needs: bool,
        start: impl FnOnce() -> oneshot::Receiver<Option<T>>,
    ) -> Polled<T> {
        let now = Instant::now();
        match std::mem::replace(self, Self::Idle) {
            Self::Loading(mut rx) => match rx.try_recv() {
                Ok(Some(today)) => Polled::Loaded(today),
                Ok(None) | Err(oneshot::error::TryRecvError::Closed) => {
                    *self = Self::Failed {
                        retry_after: now + LOAD_RETRY,
                    };
                    Polled::Failed
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    *self = Self::Loading(rx);
                    Polled::Waiting
                }
            },
            Self::Failed { retry_after } if now < retry_after => {
                *self = Self::Failed { retry_after };
                Polled::Waiting
            }
            Self::Idle | Self::Failed { .. } => {
                if needs && tokio::runtime::Handle::try_current().is_ok() {
                    *self = Self::Loading(start());
                }
                Polled::Waiting
            }
        }
    }
}

#[cfg(test)]
#[path = "daily_load_test.rs"]
mod daily_load_test;
