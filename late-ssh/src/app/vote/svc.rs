use anyhow::Result;
use late_core::{MutexRecover, db::Db, models::vote::Vote};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, broadcast, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use super::liquidsoap;
use crate::metrics;
use crate::state::{ActiveUsers, ActivityEvent};

#[derive(Clone)]
pub struct VoteService {
    db: Db,
    liquidsoap_addr: String,
    switch_interval: Duration,
    snapshot_tx: watch::Sender<VoteSnapshot>,
    snapshot_rx: watch::Receiver<VoteSnapshot>,
    event_tx: broadcast::Sender<VoteEvent>,
    state: Arc<Mutex<RoundState>>,
    active_users: Option<ActiveUsers>,
    activity_tx: Option<broadcast::Sender<ActivityEvent>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Genre {
    Lofi,
    Ambient,
    Classic,
    Jazz,
}

impl Genre {
    pub fn as_str(self) -> &'static str {
        match self {
            Genre::Lofi => "lofi",
            Genre::Ambient => "ambient",
            Genre::Classic => "classic",
            Genre::Jazz => "jazz",
        }
    }
}

impl std::fmt::Display for Genre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for Genre {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "lofi" => Ok(Genre::Lofi),
            "ambient" => Ok(Genre::Ambient),
            "classic" => Ok(Genre::Classic),
            "jazz" => Ok(Genre::Jazz),
            _ => Err(anyhow::anyhow!("unknown genre: {}", value)),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct VoteCount {
    pub lofi: i64,
    pub classic: i64,
    pub ambient: i64,
    pub jazz: i64,
}

impl VoteCount {
    pub fn winner(&self) -> Genre {
        let mut winner = Genre::Lofi;
        let mut max = self.lofi;

        if self.ambient > max {
            max = self.ambient;
            winner = Genre::Ambient;
        }

        if self.classic > max {
            max = self.classic;
            winner = Genre::Classic;
        }

        if self.jazz > max {
            winner = Genre::Jazz;
        }

        winner
    }

    pub fn winner_or(&self, fallback: Genre) -> Genre {
        if self.total() == 0 {
            return fallback;
        }
        self.winner()
    }

    pub fn total(&self) -> i64 {
        self.lofi + self.classic + self.ambient + self.jazz
    }
}

#[derive(Debug, Clone)]
pub struct VoteSnapshot {
    pub counts: VoteCount,
    pub current_genre: Genre,
    pub next_switch_in: Duration,
    pub updated_at: Instant,
    pub round_id: u64,
}

impl Default for VoteSnapshot {
    fn default() -> Self {
        Self {
            counts: VoteCount::default(),
            current_genre: Genre::Lofi,
            next_switch_in: Duration::from_secs(60),
            updated_at: Instant::now(),
            round_id: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum VoteEvent {
    Success { user_id: Uuid, genre: Genre },
    Error { user_id: Uuid, message: String },
}

impl VoteService {
    pub fn new(
        db: Db,
        liquidsoap_addr: String,
        switch_interval: Duration,
        active_users: ActiveUsers,
        activity_tx: broadcast::Sender<ActivityEvent>,
    ) -> Self {
        let initial_snapshot = VoteSnapshot {
            next_switch_in: switch_interval,
            ..VoteSnapshot::default()
        };
        let (snapshot_tx, snapshot_rx) = watch::channel(initial_snapshot);
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
            liquidsoap_addr,
            switch_interval,
            snapshot_tx,
            snapshot_rx,
            event_tx,
            state: Arc::new(Mutex::new(RoundState::new())),
            active_users: Some(active_users),
            activity_tx: Some(activity_tx),
        }
    }

    pub async fn start_background_task(self, shutdown: late_core::shutdown::CancellationToken) {
        tracing::info!("starting vote service background task");
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!("vote service shutting down");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    if let Err(e) = self.tick().await {
                        late_core::error_span!(
                            "vote_tick_failed",
                            error = ?e,
                            "vote service tick failed"
                        );
                    }
                }
            }
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<VoteSnapshot> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<VoteEvent> {
        self.event_tx.subscribe()
    }

    pub fn switch_interval(&self) -> Duration {
        self.switch_interval
    }

    pub async fn get_user_vote(&self, user_id: Uuid) -> Result<Option<Genre>> {
        let client = self.db.get().await?;
        let vote = Vote::find_by_user(&client, user_id).await?;
        Ok(vote.and_then(|v| Genre::try_from(v.genre.as_str()).ok()))
    }

    #[tracing::instrument(skip(self), fields(user_id = %user_id, genre = %genre))]
    pub async fn cast_vote(&self, user_id: Uuid, genre: Genre) -> Result<VoteSnapshot> {
        let vote_changed = {
            let client = self.db.get().await?;
            match Vote::upsert(&client, user_id, genre.as_str()).await {
                Ok((_, changed)) => changed,
                Err(e) => {
                    self.publish_event(VoteEvent::Error {
                        user_id,
                        message: "Vote failed. Please try again.".to_string(),
                    });
                    return Err(e);
                }
            }
        };
        self.publish_event(VoteEvent::Success { user_id, genre });
        metrics::record_vote_cast(genre.as_str());

        if vote_changed {
            self.publish_activity(user_id, genre);
        }

        let counts = self.load_counts().await?;
        Ok(self.publish_status(counts).await)
    }

    pub fn cast_vote_task(&self, user_id: Uuid, genre: Genre) {
        let service = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = service.cast_vote(user_id, genre).await {
                    late_core::error_span!("vote_cast_failed", error = ?e, "failed to cast vote");
                }
            }
            .instrument(info_span!("vote.cast_vote_task", user_id = %user_id, genre = %genre)),
        );
    }

    fn send_command(&self, command: &str) {
        let addr = self.liquidsoap_addr.clone();
        let cmd = command.to_string();
        let span_addr = addr.clone();
        let span_cmd = cmd.clone();
        tokio::spawn(
            async move {
                if let Err(err) = liquidsoap::send_command(&addr, &cmd).await {
                    late_core::error_span!(
                        "liquidsoap_command_failed",
                        error = ?err,
                        "failed to send command to Liquidsoap"
                    );
                }
            }
            .instrument(info_span!(
                "vote.send_liquidsoap_command",
                command = %span_cmd,
                addr = %span_addr
            )),
        );
    }

    async fn load_counts(&self) -> Result<VoteCount> {
        let client = self.db.get().await?;
        let (lofi, classic, ambient, jazz) = Vote::tally(&client).await?;
        Ok(VoteCount {
            lofi,
            classic,
            ambient,
            jazz,
        })
    }

    /// Called periodically by the background task
    #[tracing::instrument(skip(self))]
    async fn tick(&self) -> Result<()> {
        let client = self.db.get().await?;
        let (lofi, classic, ambient, jazz) = Vote::tally(&client).await?;
        let counts = self
            .publish_status(VoteCount {
                lofi,
                classic,
                ambient,
                jazz,
            })
            .await
            .counts;

        let (should_switch, current_genre) = {
            let state = self.state.lock().await;
            (
                state.should_switch(self.switch_interval),
                state.current_genre,
            )
        };

        if should_switch {
            let winner = counts.winner_or(current_genre);
            self.switch_to_winner(winner).await;
            self.state.lock().await.advance(winner);
            Vote::clear_all(&client).await?;
            let _ = self.publish_status(VoteCount::default()).await;
        }

        Ok(())
    }

    #[tracing::instrument(skip(self), fields(winner = %winner))]
    async fn switch_to_winner(&self, winner: Genre) {
        tracing::info!(?winner, "switching to winning genre");
        let command = format!("vibe.set {}", winner.as_str());
        self.send_command(&command);
    }

    async fn publish_status(&self, counts: VoteCount) -> VoteSnapshot {
        let (current_genre, next_switch_in, round_id) = {
            let state = self.state.lock().await;
            (
                state.current_genre,
                state.next_switch_in(self.switch_interval),
                state.round_id,
            )
        };
        let status = VoteSnapshot {
            counts,
            current_genre,
            next_switch_in,
            updated_at: Instant::now(),
            round_id,
        };
        tracing::debug!(?status, "publishing vote status update");
        let _ = self.snapshot_tx.send(status.clone());
        status
    }

    fn publish_event(&self, event: VoteEvent) {
        let _ = self.event_tx.send(event);
    }

    fn publish_activity(&self, user_id: Uuid, genre: Genre) {
        let (Some(active_users), Some(activity_tx)) = (&self.active_users, &self.activity_tx)
        else {
            return;
        };
        let username = {
            let guard = active_users.lock_recover();
            match guard.get(&user_id) {
                Some(u) => u.username.clone(),
                None => return,
            }
        };
        let _ = activity_tx.send(ActivityEvent {
            username,
            action: format!("voted {genre}"),
            at: Instant::now(),
        });
    }
}

struct RoundState {
    last_switch: Instant,
    current_genre: Genre,
    round_id: u64,
}

impl RoundState {
    fn new() -> Self {
        Self {
            last_switch: Instant::now(),
            current_genre: Genre::Lofi,
            round_id: 0,
        }
    }

    fn should_switch(&self, interval: Duration) -> bool {
        self.last_switch.elapsed() >= interval
    }

    fn advance(&mut self, winner: Genre) {
        self.last_switch = Instant::now();
        self.current_genre = winner;
        self.round_id = self.round_id.saturating_add(1);
    }

    fn next_switch_in(&self, interval: Duration) -> Duration {
        interval.saturating_sub(self.last_switch.elapsed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vote_count_winner_picks_highest() {
        let counts = VoteCount {
            lofi: 5,
            classic: 9,
            ambient: 2,
            jazz: 2,
        };
        assert_eq!(counts.winner(), Genre::Classic);
    }

    #[test]
    fn vote_count_winner_tie_prefers_first_in_order() {
        let counts = VoteCount {
            lofi: 3,
            classic: 3,
            ambient: 3,
            jazz: 1,
        };
        assert_eq!(counts.winner(), Genre::Lofi);
    }

    #[test]
    fn vote_count_winner_all_zero_defaults_to_lofi() {
        let counts = VoteCount::default();
        assert_eq!(counts.winner(), Genre::Lofi);
    }

    #[test]
    fn genre_as_str_values() {
        assert_eq!(Genre::Lofi.as_str(), "lofi");
        assert_eq!(Genre::Classic.as_str(), "classic");
        assert_eq!(Genre::Ambient.as_str(), "ambient");
        assert_eq!(Genre::Jazz.as_str(), "jazz");
    }

    #[test]
    fn genre_try_from_str() {
        assert_eq!(Genre::try_from("lofi").unwrap(), Genre::Lofi);
        assert_eq!(Genre::try_from("classic").unwrap(), Genre::Classic);
        assert_eq!(Genre::try_from("ambient").unwrap(), Genre::Ambient);
        assert_eq!(Genre::try_from("jazz").unwrap(), Genre::Jazz);
        assert!(Genre::try_from("nope").is_err());
    }

    #[test]
    fn vote_count_total_adds_all_genres() {
        let counts = VoteCount {
            lofi: 2,
            classic: 3,
            ambient: 4,
            jazz: 5,
        };
        assert_eq!(counts.total(), 14);
    }

    #[test]
    fn winner_or_uses_fallback_when_empty() {
        let counts = VoteCount::default();
        assert_eq!(counts.winner_or(Genre::Jazz), Genre::Jazz);
    }

    #[test]
    fn vote_snapshot_default_round_and_interval() {
        let snapshot = VoteSnapshot::default();
        assert_eq!(snapshot.round_id, 0);
        assert_eq!(snapshot.current_genre, Genre::Lofi);
        assert!(snapshot.next_switch_in <= Duration::from_secs(60));
    }
}
