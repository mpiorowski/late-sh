use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    db::Db,
    models::{
        media_queue_item::MediaQueueItem,
        media_queue_vote::{CastVoteOutcome, MediaQueueVote},
        media_source::MediaSource,
        user::User,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast, oneshot, watch};
use uuid::Uuid;

use super::youtube::YoutubeClient;
use crate::paired_clients::PairedClientRegistry;

const QUEUE_SNAPSHOT_LIMIT: i64 = 50;
const MAX_SUBMISSIONS_PER_WINDOW: i64 = 3;
const SUBMISSION_WINDOW: chrono::Duration = chrono::Duration::minutes(5);
const FALLBACK_DEBOUNCE: Duration = Duration::from_secs(10);
const PLAYBACK_SYNC_INTERVAL: Duration = Duration::from_secs(10);
const PLAYBACK_END_GRACE: Duration = Duration::from_secs(5);
const STREAM_CAP: Duration = Duration::from_secs(60 * 60);

#[derive(Clone)]
pub struct AudioService {
    db: Db,
    youtube: YoutubeClient,
    ws_tx: broadcast::Sender<AudioWsMessage>,
    event_tx: broadcast::Sender<AudioEvent>,
    snapshot_tx: watch::Sender<QueueSnapshot>,
    state: Arc<Mutex<QueueState>>,
    paired_clients: PairedClientRegistry,
}

#[derive(Default)]
struct QueueState {
    mode: AudioMode,
    current_item_id: Option<Uuid>,
    sequence: u64,
    playback_cancel: Option<oneshot::Sender<()>>,
    fallback_cancel: Option<oneshot::Sender<()>>,
    skip_votes: HashSet<Uuid>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioMode {
    #[default]
    Icecast,
    Youtube,
}

impl AudioMode {
    pub fn as_str(self) -> &'static str {
        match self {
            AudioMode::Icecast => "icecast",
            AudioMode::Youtube => "youtube",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AudioWsMessage {
    LoadVideo {
        item_id: Uuid,
        video_id: String,
        started_at_ms: i64,
        offset_ms: u64,
        is_stream: bool,
    },
    Seek {
        offset_ms: u64,
    },
    SourceChanged {
        audio_mode: AudioMode,
    },
    QueueUpdate {
        current: Option<QueueItemView>,
        queue: Vec<QueueItemView>,
        sequence: u64,
        skip_progress: Option<SkipProgress>,
    },
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SkipProgress {
    pub votes: u32,
    pub threshold: u32,
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    TrustedSubmitQueued {
        user_id: Uuid,
        position: i64,
    },
    TrustedSubmitFailed {
        user_id: Uuid,
        message: String,
    },
    YoutubeFallbackSet {
        user_id: Uuid,
    },
    YoutubeFallbackFailed {
        user_id: Uuid,
        message: String,
    },
    BoothSubmitQueued {
        user_id: Uuid,
        position: i64,
    },
    BoothSubmitFailed {
        user_id: Uuid,
        message: String,
    },
    BoothVoteApplied {
        user_id: Uuid,
        item_id: Uuid,
        score: i32,
    },
    BoothVoteFailed {
        user_id: Uuid,
        message: String,
    },
    BoothSkipFired {
        user_id: Uuid,
    },
    BoothSkipProgress {
        user_id: Uuid,
        votes: u32,
        threshold: u32,
    },
    /// The spawned DB persist for `users.settings.audio_source` failed. The
    /// caller has already optimistically updated local state; this surfaces
    /// the failure as a banner so the user knows their pref didn't save.
    AudioSourcePersistFailed {
        user_id: Uuid,
        message: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct CastSkipResult {
    pub progress: SkipProgress,
    pub fired: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueSnapshot {
    pub audio_mode: AudioMode,
    pub current: Option<QueueItemView>,
    pub queue: Vec<QueueItemView>,
    #[serde(default)]
    pub skip_progress: Option<SkipProgress>,
}

impl QueueSnapshot {
    pub fn skip_progress(&self) -> Option<SkipProgress> {
        self.skip_progress
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueItemView {
    pub id: Uuid,
    pub video_id: String,
    pub title: Option<String>,
    pub channel: Option<String>,
    pub duration_ms: Option<i32>,
    pub started_at_ms: Option<i64>,
    pub is_stream: bool,
    pub submitter: String,
    pub submitter_id: Uuid,
    #[serde(default)]
    pub vote_score: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubmitQueueResponse {
    pub id: Uuid,
    pub title: Option<String>,
    pub duration_ms: Option<i32>,
    pub position_in_queue: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerPlaybackState {
    Playing,
    Paused,
    Buffering,
    Ended,
    Error,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerStateReport {
    pub item_id: Uuid,
    pub state: PlayerPlaybackState,
    #[serde(default)]
    pub offset_ms: Option<u64>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub autoplay_blocked: bool,
    #[serde(default)]
    pub error: Option<String>,
}

impl AudioService {
    pub fn new(
        db: Db,
        youtube_api_key: Option<String>,
        paired_clients: PairedClientRegistry,
    ) -> Self {
        let (ws_tx, _) = broadcast::channel(512);
        let (event_tx, _) = broadcast::channel(256);
        let (snapshot_tx, _) = watch::channel(QueueSnapshot {
            audio_mode: AudioMode::Icecast,
            current: None,
            queue: Vec::new(),
            skip_progress: None,
        });
        Self {
            db,
            youtube: YoutubeClient::new(youtube_api_key),
            ws_tx,
            event_tx,
            snapshot_tx,
            state: Arc::new(Mutex::new(QueueState::default())),
            paired_clients,
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<QueueSnapshot> {
        self.snapshot_tx.subscribe()
    }

    /// True once the YouTube Data API key is configured. The booth disables
    /// public submissions when this returns false; staff `/audio` keeps
    /// working through the trusted path.
    pub fn booth_submit_enabled(&self) -> bool {
        self.youtube.has_api_key()
    }

    pub fn subscribe_ws(&self) -> broadcast::Receiver<AudioWsMessage> {
        self.ws_tx.subscribe()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<AudioEvent> {
        self.event_tx.subscribe()
    }

    pub async fn start_background_task(self, shutdown: late_core::shutdown::CancellationToken) {
        if let Err(err) = self.sweep_orphan_playing().await {
            late_core::error_span!(
                "audio_orphan_sweep_failed",
                error = ?err,
                "failed to sweep orphan playing rows"
            );
        }
        if let Err(err) = self.resume_from_db().await {
            late_core::error_span!(
                "audio_resume_failed",
                error = ?err,
                "failed to resume audio queue from database"
            );
        }

        shutdown.cancelled().await;
        self.cancel_timers().await;
        tracing::info!("audio service shutting down");
    }

    pub async fn submit_url(&self, user_id: Uuid, url: &str) -> Result<SubmitQueueResponse> {
        let video = self.youtube.validate_url(url).await?;
        self.submit_video(user_id, video, true).await
    }

    pub async fn submit_trusted_url(
        &self,
        user_id: Uuid,
        url: &str,
    ) -> Result<SubmitQueueResponse> {
        let video = super::youtube::trusted_video_from_url(url)?;
        self.submit_video(user_id, video, false).await
    }

    pub async fn set_trusted_youtube_fallback(&self, user_id: Uuid, url: &str) -> Result<()> {
        let video = super::youtube::trusted_video_from_url(url)?;
        let mut state = self.state.lock().await;
        let client = self.db.get().await?;
        let source = MediaSource::upsert_youtube_fallback(
            &client,
            &video.video_id,
            video.title.as_deref(),
            video.channel.as_deref(),
            user_id,
        )
        .await?;

        if state.current_item_id.is_none() && MediaQueueItem::first_queued(&client).await?.is_none()
        {
            self.cancel_playback(&mut state);
            self.cancel_fallback(&mut state);
            state.mode = AudioMode::Youtube;
            self.publish_source_change(AudioMode::Youtube);
            self.publish_load_fallback(&source);
            self.publish_queue_update_with_guard(&mut state).await?;
        }

        Ok(())
    }

    async fn submit_video(
        &self,
        user_id: Uuid,
        video: super::youtube::YoutubeVideo,
        enforce_rate_limit: bool,
    ) -> Result<SubmitQueueResponse> {
        let mut state = self.state.lock().await;

        let item = {
            let client = self.db.get().await?;
            if enforce_rate_limit {
                let since = Utc::now() - SUBMISSION_WINDOW;
                let recent =
                    MediaQueueItem::recent_submission_count(&client, user_id, since).await?;
                if recent >= MAX_SUBMISSIONS_PER_WINDOW {
                    anyhow::bail!("submission rate limit exceeded");
                }
            }

            MediaQueueItem::insert_youtube(
                &client,
                user_id,
                &video.video_id,
                video.title.as_deref(),
                video.channel.as_deref(),
                video.duration_ms,
                video.is_stream,
            )
            .await?
        };

        self.cancel_fallback(&mut state);
        if state.current_item_id.is_none() {
            self.advance_to_next_with_guard(&mut state).await?;
        } else {
            self.publish_queue_update_with_guard(&mut state).await?;
        }

        let position_in_queue = if state.current_item_id == Some(item.id) {
            0
        } else {
            let client = self.db.get().await?;
            MediaQueueItem::queued_before_count(&client, item.created).await? + 1
        };

        Ok(SubmitQueueResponse {
            id: item.id,
            title: item.title,
            duration_ms: item.duration_ms,
            position_in_queue,
        })
    }

    pub fn submit_url_task(&self, user_id: Uuid, url: String) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(err) = service.submit_url(user_id, &url).await {
                late_core::error_span!(
                    "audio_submit_url_failed",
                    error = ?err,
                    user_id = %user_id,
                    "failed to submit media queue URL"
                );
            }
        });
    }

    /// Booth submit: same as `submit_url` (YouTube Data API validation +
    /// rate limit) but emits banner events so the modal can surface
    /// success/failure to the submitter.
    pub fn booth_submit_public_task(&self, user_id: Uuid, url: String) {
        let service = self.clone();
        tokio::spawn(async move {
            if !service.booth_submit_enabled() {
                service.publish_event(AudioEvent::BoothSubmitFailed {
                    user_id,
                    message: "Submissions disabled - server YouTube key is unset".to_string(),
                });
                return;
            }
            match service.submit_url(user_id, &url).await {
                Ok(response) => {
                    service.publish_event(AudioEvent::BoothSubmitQueued {
                        user_id,
                        position: response.position_in_queue,
                    });
                }
                Err(err) => {
                    late_core::error_span!(
                        "audio_booth_submit_failed",
                        error = ?err,
                        user_id = %user_id,
                        "failed to submit booth audio URL"
                    );
                    service.publish_event(AudioEvent::BoothSubmitFailed {
                        user_id,
                        message: booth_submit_error_message(&err),
                    });
                }
            }
        });
    }

    pub fn submit_trusted_url_task(&self, user_id: Uuid, url: String) {
        let service = self.clone();
        tokio::spawn(async move {
            match service.submit_trusted_url(user_id, &url).await {
                Ok(response) => {
                    tracing::info!(
                        item_id = %response.id,
                        position = response.position_in_queue,
                        "queued trusted audio URL"
                    );
                    service.publish_event(AudioEvent::TrustedSubmitQueued {
                        user_id,
                        position: response.position_in_queue,
                    });
                }
                Err(err) => {
                    late_core::error_span!(
                        "audio_trusted_submit_failed",
                        error = ?err,
                        user_id = %user_id,
                        "failed to queue trusted audio URL"
                    );
                    service.publish_event(AudioEvent::TrustedSubmitFailed {
                        user_id,
                        message: trusted_submit_error_message(&err),
                    });
                }
            }
        });
    }

    pub fn set_trusted_youtube_fallback_task(&self, user_id: Uuid, url: String) {
        let service = self.clone();
        tokio::spawn(async move {
            match service.set_trusted_youtube_fallback(user_id, &url).await {
                Ok(()) => {
                    service.publish_event(AudioEvent::YoutubeFallbackSet { user_id });
                }
                Err(err) => {
                    late_core::error_span!(
                        "audio_youtube_fallback_set_failed",
                        error = ?err,
                        user_id = %user_id,
                        "failed to set YouTube fallback"
                    );
                    service.publish_event(AudioEvent::YoutubeFallbackFailed {
                        user_id,
                        message: trusted_submit_error_message(&err),
                    });
                }
            }
        });
    }

    fn publish_event(&self, event: AudioEvent) {
        let _ = self.event_tx.send(event);
    }

    /// Cast or change a vote (+1/-1) on a queued item. Rejects votes against
    /// the currently-playing track and against non-queued items. Returns the
    /// new aggregate score on success.
    pub async fn persist_audio_source(
        &self,
        user_id: Uuid,
        source: late_core::models::user::AudioSource,
    ) -> Result<()> {
        let client = self.db.get().await?;
        late_core::models::user::User::set_audio_source(&client, user_id, source).await
    }

    pub async fn read_audio_source(
        &self,
        user_id: Uuid,
    ) -> Result<late_core::models::user::AudioSource> {
        let client = self.db.get().await?;
        late_core::models::user::User::audio_source(&client, user_id).await
    }

    /// Spawn a background persist for the user's audio-source preference.
    /// On failure publishes `AudioSourcePersistFailed` so the session's
    /// `AudioState::tick` can surface a banner.
    pub fn persist_audio_source_task(
        &self,
        user_id: Uuid,
        source: late_core::models::user::AudioSource,
    ) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(err) = service.persist_audio_source(user_id, source).await {
                late_core::error_span!(
                    "audio_source_persist_failed",
                    error = ?err,
                    user_id = %user_id,
                    "failed to persist audio source preference"
                );
                service.publish_event(AudioEvent::AudioSourcePersistFailed {
                    user_id,
                    message: "Failed to save audio source preference".to_string(),
                });
            }
        });
    }

    pub async fn cast_vote(&self, user_id: Uuid, item_id: Uuid, value: i16) -> Result<i32> {
        if value != 1 && value != -1 {
            anyhow::bail!("invalid vote value");
        }

        let mut client = self.db.get().await?;
        let outcome = MediaQueueVote::cast_guarded(&mut client, user_id, item_id, value).await?;
        drop(client);
        let score = match outcome {
            CastVoteOutcome::Applied(score) => score,
            CastVoteOutcome::NotFound => anyhow::bail!("queue item not found"),
            CastVoteOutcome::VotingClosed => anyhow::bail!("voting closed - track started"),
            CastVoteOutcome::NotVoteable => anyhow::bail!("queue item is no longer voteable"),
        };

        let mut state = self.state.lock().await;
        self.publish_queue_update_with_guard(&mut state).await?;
        Ok(score)
    }

    /// Remove a vote (returns new score) for the user/item pair.
    pub async fn clear_vote(&self, user_id: Uuid, item_id: Uuid) -> Result<i32> {
        let client = self.db.get().await?;
        let score = MediaQueueVote::delete_vote(&client, user_id, item_id).await?;
        drop(client);

        let mut state = self.state.lock().await;
        self.publish_queue_update_with_guard(&mut state).await?;
        Ok(score)
    }

    /// Cast a skip-vote for the currently-playing track. Returns the new
    /// progress; if the threshold has been hit, advances the queue.
    ///
    /// Gated on the caller's session having at least one paired client. An
    /// SSH-only user can't influence what paired listeners hear, otherwise a
    /// coordinated unpaired group could grief the threshold (which is
    /// computed against `paired_clients.total_pairings()`).
    pub async fn cast_skip_vote(
        &self,
        user_id: Uuid,
        session_token: &str,
    ) -> Result<CastSkipResult> {
        if !self.paired_clients.is_paired(session_token) {
            anyhow::bail!("pair a client to skip-vote");
        }

        let mut state = self.state.lock().await;
        let Some(current_id) = state.current_item_id else {
            anyhow::bail!("nothing is playing");
        };

        state.skip_votes.insert(user_id);
        let votes = state.skip_votes.len() as u32;
        let threshold = skip_threshold(self.paired_clients.total_pairings());
        let fired = votes >= threshold;

        if fired {
            let client = self.db.get().await?;
            let _ =
                MediaQueueItem::update_status(&client, current_id, MediaQueueItem::STATUS_SKIPPED)
                    .await?;
            drop(client);
            state.current_item_id = None;
            state.skip_votes.clear();
            self.cancel_playback(&mut state);
            self.advance_to_next_with_guard(&mut state).await?;
        } else {
            self.publish_queue_update_with_guard(&mut state).await?;
        }

        Ok(CastSkipResult {
            progress: SkipProgress { votes, threshold },
            fired,
        })
    }

    /// Re-evaluate whether the pending skip-votes already meet the threshold.
    /// Called from the disconnect path when the paired-client total drops; if
    /// the threshold fell to or below the existing vote count, fire a skip.
    pub async fn reevaluate_skip_threshold(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        let Some(current_id) = state.current_item_id else {
            return Ok(());
        };
        if state.skip_votes.is_empty() {
            return Ok(());
        }
        let votes = state.skip_votes.len() as u32;
        let threshold = skip_threshold(self.paired_clients.total_pairings());
        if votes < threshold {
            self.publish_queue_update_with_guard(&mut state).await?;
            return Ok(());
        }
        let client = self.db.get().await?;
        let _ = MediaQueueItem::update_status(&client, current_id, MediaQueueItem::STATUS_SKIPPED)
            .await?;
        drop(client);
        state.current_item_id = None;
        state.skip_votes.clear();
        self.cancel_playback(&mut state);
        self.advance_to_next_with_guard(&mut state).await
    }

    pub fn cast_vote_task(&self, user_id: Uuid, item_id: Uuid, value: i16) {
        let service = self.clone();
        tokio::spawn(async move {
            match service.cast_vote(user_id, item_id, value).await {
                Ok(score) => {
                    service.publish_event(AudioEvent::BoothVoteApplied {
                        user_id,
                        item_id,
                        score,
                    });
                }
                Err(err) => {
                    service.publish_event(AudioEvent::BoothVoteFailed {
                        user_id,
                        message: booth_vote_error_message(&err),
                    });
                }
            }
        });
    }

    pub fn clear_vote_task(&self, user_id: Uuid, item_id: Uuid) {
        let service = self.clone();
        tokio::spawn(async move {
            match service.clear_vote(user_id, item_id).await {
                Ok(score) => {
                    service.publish_event(AudioEvent::BoothVoteApplied {
                        user_id,
                        item_id,
                        score,
                    });
                }
                Err(err) => {
                    service.publish_event(AudioEvent::BoothVoteFailed {
                        user_id,
                        message: booth_vote_error_message(&err),
                    });
                }
            }
        });
    }

    pub fn cast_skip_vote_task(&self, user_id: Uuid, session_token: String) {
        let service = self.clone();
        tokio::spawn(async move {
            match service.cast_skip_vote(user_id, &session_token).await {
                Ok(result) => {
                    if result.fired {
                        service.publish_event(AudioEvent::BoothSkipFired { user_id });
                    } else {
                        service.publish_event(AudioEvent::BoothSkipProgress {
                            user_id,
                            votes: result.progress.votes,
                            threshold: result.progress.threshold,
                        });
                    }
                }
                Err(err) => {
                    service.publish_event(AudioEvent::BoothVoteFailed {
                        user_id,
                        message: booth_vote_error_message(&err),
                    });
                }
            }
        });
    }

    pub fn reevaluate_skip_threshold_task(&self) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(err) = service.reevaluate_skip_threshold().await {
                late_core::error_span!(
                    "audio_skip_reeval_failed",
                    error = ?err,
                    "failed to re-evaluate skip threshold"
                );
            }
        });
    }

    pub async fn report_player_state(&self, report: PlayerStateReport) -> Result<()> {
        match report.state {
            PlayerPlaybackState::Ended => self.finish_item_from_player(report).await,
            PlayerPlaybackState::Error => {
                let reason = report
                    .error
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("browser reported playback error");
                self.fail_item(report.item_id, reason).await
            }
            PlayerPlaybackState::Playing
            | PlayerPlaybackState::Paused
            | PlayerPlaybackState::Buffering => {
                if report.autoplay_blocked {
                    tracing::warn!(
                        item_id = %report.item_id,
                        offset_ms = ?report.offset_ms,
                        "browser reported autoplay blocked"
                    );
                }
                self.record_browser_duration(report.item_id, report.duration_ms)
                    .await?;
                Ok(())
            }
        }
    }

    pub fn report_player_state_task(&self, report: PlayerStateReport) {
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(err) = service.report_player_state(report).await {
                late_core::error_span!(
                    "audio_player_state_failed",
                    error = ?err,
                    "failed to handle media player state"
                );
            }
        });
    }

    pub async fn snapshot(&self) -> Result<QueueSnapshot> {
        let mode = self.state.lock().await.mode;
        self.load_snapshot(mode).await
    }

    pub async fn initial_ws_messages(&self) -> Result<Vec<AudioWsMessage>> {
        let state = self.state.lock().await;
        let snapshot = self.load_snapshot(state.mode).await?;
        let skip_progress = self.compute_skip_progress(&state, snapshot.current.as_ref());
        let mut events = vec![
            AudioWsMessage::SourceChanged {
                audio_mode: snapshot.audio_mode,
            },
            AudioWsMessage::QueueUpdate {
                current: snapshot.current.clone(),
                queue: snapshot.queue.clone(),
                sequence: state.sequence,
                skip_progress,
            },
        ];
        if let Some(current) = &snapshot.current
            && let Some(started_at_ms) = current.started_at_ms
        {
            events.push(AudioWsMessage::LoadVideo {
                item_id: current.id,
                video_id: current.video_id.clone(),
                started_at_ms,
                offset_ms: offset_from_started_at_ms(started_at_ms),
                is_stream: current.is_stream,
            });
        } else if snapshot.audio_mode == AudioMode::Youtube {
            let client = self.db.get().await?;
            if let Some(source) = MediaSource::youtube_fallback(&client).await? {
                events.push(fallback_load_event(&source));
            }
        }
        Ok(events)
    }

    async fn sweep_orphan_playing(&self) -> Result<()> {
        let client = self.db.get().await?;
        let cutoff = Utc::now()
            - chrono::Duration::from_std(STREAM_CAP).unwrap_or_else(|_| chrono::Duration::hours(1));
        let swept = MediaQueueItem::sweep_orphan_playing(&client, cutoff).await?;
        if swept > 0 {
            tracing::warn!(
                swept,
                cutoff = %cutoff,
                "swept orphan playing media_queue_items at startup"
            );
        }
        Ok(())
    }

    async fn resume_from_db(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        let client = self.db.get().await?;
        let now = Utc::now();

        if let Some(item) = MediaQueueItem::current_playing(&client).await? {
            if item_is_still_playable(&item, now) {
                state.current_item_id = Some(item.id);
                state.skip_votes.clear();
                state.mode = AudioMode::Youtube;
                self.schedule_playback_timer(&mut state, &item);
                self.publish_source_change(AudioMode::Youtube);
                self.publish_load_video(&item);
                self.publish_queue_update_with_guard(&mut state).await?;
                return Ok(());
            }

            let _ = MediaQueueItem::mark_played(&client, item.id, now).await?;
        }

        self.advance_to_next_with_guard(&mut state).await
    }

    async fn finish_item_due_to_timer(&self, item_id: Uuid) -> Result<()> {
        tracing::info!(%item_id, "media queue item reached playback limit");
        self.finish_item(item_id).await
    }

    async fn finish_item_from_player(&self, report: PlayerStateReport) -> Result<()> {
        self.record_browser_duration(report.item_id, report.duration_ms)
            .await?;

        let state = self.state.lock().await;
        if state.current_item_id != Some(report.item_id) {
            return Ok(());
        }

        let client = self.db.get().await?;
        let Some(item) = MediaQueueItem::find_by_id(&client, report.item_id).await? else {
            return Ok(());
        };
        let Some(started_at) = item.started_at else {
            return Ok(());
        };

        let elapsed = Utc::now()
            .signed_duration_since(started_at)
            .to_std()
            .unwrap_or_default();
        let Some(duration) = playback_known_duration(&item) else {
            tracing::debug!(
                item_id = %report.item_id,
                elapsed_ms = elapsed.as_millis() as u64,
                offset_ms = ?report.offset_ms,
                "ignoring browser ended report; server-known duration missing - server timer is authoritative"
            );
            self.publish_seek_for_started_at(started_at);
            return Ok(());
        };

        if elapsed.saturating_add(PLAYBACK_END_GRACE) < duration {
            tracing::debug!(
                item_id = %report.item_id,
                elapsed_ms = elapsed.as_millis() as u64,
                duration_ms = duration.as_millis() as u64,
                offset_ms = ?report.offset_ms,
                "ignoring early browser ended report"
            );
            self.publish_seek_for_started_at(started_at);
            return Ok(());
        }

        drop(state);
        self.finish_item(report.item_id).await
    }

    async fn finish_item(&self, item_id: Uuid) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.current_item_id != Some(item_id) {
            return Ok(());
        }

        let client = self.db.get().await?;
        let changed = MediaQueueItem::mark_played(&client, item_id, Utc::now()).await?;
        if changed == 0 {
            return Ok(());
        }
        state.current_item_id = None;
        state.skip_votes.clear();
        self.cancel_playback(&mut state);
        self.advance_to_next_with_guard(&mut state).await
    }

    async fn fail_item(&self, item_id: Uuid, reason: &str) -> Result<()> {
        let mut state = self.state.lock().await;
        if state.current_item_id != Some(item_id) {
            return Ok(());
        }

        let client = self.db.get().await?;
        let changed = MediaQueueItem::mark_failed(&client, item_id, Utc::now(), reason).await?;
        if changed == 0 {
            return Ok(());
        }
        state.current_item_id = None;
        state.skip_votes.clear();
        self.cancel_playback(&mut state);
        self.advance_to_next_with_guard(&mut state).await
    }

    async fn advance_to_next_with_guard(&self, state: &mut QueueState) -> Result<()> {
        let client = self.db.get().await?;
        if let Some((next, _score)) = MediaQueueItem::first_queued(&client).await? {
            self.cancel_fallback(state);
            let Some(item) = MediaQueueItem::mark_playing(&client, next.id, Utc::now()).await?
            else {
                tracing::warn!(
                    item_id = %next.id,
                    "mark_playing returned no row; another playing row likely holds the slot - skipping advance"
                );
                self.schedule_fallback(state);
                self.publish_queue_update_with_guard(state).await?;
                return Ok(());
            };
            state.current_item_id = Some(item.id);
            state.skip_votes.clear();
            state.mode = AudioMode::Youtube;
            self.schedule_playback_timer(state, &item);
            self.publish_source_change(AudioMode::Youtube);
            self.publish_load_video(&item);
            self.publish_queue_update_with_guard(state).await?;
            return Ok(());
        }

        state.current_item_id = None;
        state.skip_votes.clear();
        self.cancel_playback(state);
        if !self.publish_youtube_fallback_with_guard(state).await? {
            self.schedule_fallback(state);
            self.publish_queue_update_with_guard(state).await?;
        }
        Ok(())
    }

    async fn publish_queue_update_with_guard(&self, state: &mut QueueState) -> Result<()> {
        state.sequence = state.sequence.saturating_add(1);
        let mut snapshot = self.load_snapshot(state.mode).await?;
        snapshot.skip_progress = self.compute_skip_progress(state, snapshot.current.as_ref());
        let _ = self.snapshot_tx.send(snapshot.clone());
        let _ = self.ws_tx.send(AudioWsMessage::QueueUpdate {
            current: snapshot.current,
            queue: snapshot.queue,
            sequence: state.sequence,
            skip_progress: snapshot.skip_progress,
        });
        Ok(())
    }

    /// Compute the skip-vote progress for the currently playing item. Returns
    /// None when nothing is playing (skip vote only applies to a live track).
    fn compute_skip_progress(
        &self,
        state: &QueueState,
        current: Option<&QueueItemView>,
    ) -> Option<SkipProgress> {
        if current.is_none() || state.current_item_id.is_none() {
            return None;
        }
        let votes = state.skip_votes.len() as u32;
        let threshold = skip_threshold(self.paired_clients.total_pairings());
        Some(SkipProgress { votes, threshold })
    }

    async fn load_snapshot(&self, mode: AudioMode) -> Result<QueueSnapshot> {
        let client = self.db.get().await?;
        let items = MediaQueueItem::list_snapshot(&client, QUEUE_SNAPSHOT_LIMIT).await?;
        let user_ids = items
            .iter()
            .map(|(item, _)| item.submitter_id)
            .collect::<Vec<_>>();
        let usernames = User::list_usernames_by_ids(&client, &user_ids).await?;

        let mut current = None;
        let mut queue = Vec::new();
        for (item, score) in items {
            let view = queue_item_view(item, score, &usernames);
            if view.started_at_ms.is_some() {
                current = Some(view);
            } else {
                queue.push(view);
            }
        }

        Ok(QueueSnapshot {
            audio_mode: mode,
            current,
            queue,
            skip_progress: None,
        })
    }

    fn publish_source_change(&self, mode: AudioMode) {
        let _ = self
            .ws_tx
            .send(AudioWsMessage::SourceChanged { audio_mode: mode });
    }

    fn publish_load_video(&self, item: &MediaQueueItem) {
        let Some(started_at) = item.started_at else {
            return;
        };
        let _ = self.ws_tx.send(AudioWsMessage::LoadVideo {
            item_id: item.id,
            video_id: item.external_id.clone(),
            started_at_ms: started_at.timestamp_millis(),
            offset_ms: offset_for_started_at(started_at),
            is_stream: item.is_stream,
        });
    }

    fn publish_load_fallback(&self, source: &MediaSource) {
        let _ = self.ws_tx.send(fallback_load_event(source));
    }

    fn publish_seek_for_started_at(&self, started_at: DateTime<Utc>) {
        let _ = self.ws_tx.send(AudioWsMessage::Seek {
            offset_ms: offset_for_started_at(started_at),
        });
    }

    async fn publish_youtube_fallback_with_guard(&self, state: &mut QueueState) -> Result<bool> {
        let client = self.db.get().await?;
        let Some(source) = MediaSource::youtube_fallback(&client).await? else {
            return Ok(false);
        };

        self.cancel_playback(state);
        self.cancel_fallback(state);
        state.mode = AudioMode::Youtube;
        self.publish_source_change(AudioMode::Youtube);
        self.publish_load_fallback(&source);
        self.publish_queue_update_with_guard(state).await?;
        Ok(true)
    }

    fn schedule_playback_timer(&self, state: &mut QueueState, item: &MediaQueueItem) {
        self.cancel_playback(state);
        let Some(started_at) = item.started_at else {
            return;
        };

        let duration = playback_duration(item);
        if duration.is_zero() {
            return;
        }

        let elapsed = Utc::now()
            .signed_duration_since(started_at)
            .to_std()
            .unwrap_or_default();
        let sleep_for = duration.saturating_sub(elapsed);
        let item_id = item.id;
        let service = self.clone();
        let (tx, rx) = oneshot::channel();
        state.playback_cancel = Some(tx);
        tokio::spawn(async move {
            let mut sync = tokio::time::interval(PLAYBACK_SYNC_INTERVAL);
            sync.tick().await;
            tokio::select! {
                _ = tokio::time::sleep(sleep_for) => {
                    if let Err(err) = service.finish_item_due_to_timer(item_id).await {
                        late_core::error_span!(
                            "audio_playback_timer_failed",
                            error = ?err,
                            item_id = %item_id,
                            "failed to finish media queue item after timer"
                        );
                    }
                }
                _ = async {
                    loop {
                        sync.tick().await;
                        service.publish_seek_for_started_at(started_at);
                    }
                } => {}
                _ = rx => {}
            }
        });
    }

    async fn record_browser_duration(&self, item_id: Uuid, duration_ms: Option<u64>) -> Result<()> {
        let Some(duration_ms) = duration_ms.and_then(|value| i32::try_from(value).ok()) else {
            return Ok(());
        };
        if duration_ms <= 0 {
            return Ok(());
        }

        let client = self.db.get().await?;
        if let Some(item) = MediaQueueItem::find_by_id(&client, item_id).await?
            && item.duration_ms.is_none()
            && item.status == MediaQueueItem::STATUS_PLAYING
            && let Some(updated) =
                MediaQueueItem::set_duration_if_missing(&client, item_id, duration_ms).await?
        {
            let mut state = self.state.lock().await;
            if state.current_item_id == Some(item_id) {
                self.schedule_playback_timer(&mut state, &updated);
            }
        }
        Ok(())
    }

    fn schedule_fallback(&self, state: &mut QueueState) {
        if state.mode == AudioMode::Icecast || state.fallback_cancel.is_some() {
            return;
        }

        let service = self.clone();
        let (tx, rx) = oneshot::channel();
        state.fallback_cancel = Some(tx);
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(FALLBACK_DEBOUNCE) => {
                    service.finish_fallback_debounce().await;
                }
                _ = rx => {}
            }
        });
    }

    async fn finish_fallback_debounce(&self) {
        let mut state = self.state.lock().await;
        state.fallback_cancel = None;
        if state.current_item_id.is_some() {
            return;
        }
        match self.publish_youtube_fallback_with_guard(&mut state).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(err) => {
                late_core::error_span!(
                    "audio_youtube_fallback_failed",
                    error = ?err,
                    "failed to publish YouTube fallback"
                );
            }
        }
        state.mode = AudioMode::Icecast;
        self.publish_source_change(AudioMode::Icecast);
        if let Err(err) = self.publish_queue_update_with_guard(&mut state).await {
            late_core::error_span!(
                "audio_fallback_queue_update_failed",
                error = ?err,
                "failed to publish queue update after fallback"
            );
        }
    }

    async fn cancel_timers(&self) {
        let mut state = self.state.lock().await;
        self.cancel_playback(&mut state);
        self.cancel_fallback(&mut state);
    }

    fn cancel_playback(&self, state: &mut QueueState) {
        if let Some(cancel) = state.playback_cancel.take() {
            let _ = cancel.send(());
        }
    }

    fn cancel_fallback(&self, state: &mut QueueState) {
        if let Some(cancel) = state.fallback_cancel.take() {
            let _ = cancel.send(());
        }
    }
}

fn item_is_still_playable(item: &MediaQueueItem, now: DateTime<Utc>) -> bool {
    let Some(started_at) = item.started_at else {
        return false;
    };
    let allowed = chrono::Duration::from_std(playback_duration(item))
        .unwrap_or_else(|_| chrono::Duration::seconds(STREAM_CAP.as_secs() as i64));
    now.signed_duration_since(started_at) < allowed
}

fn playback_duration(item: &MediaQueueItem) -> Duration {
    if item.is_stream {
        return STREAM_CAP;
    }

    playback_known_duration(item).unwrap_or(STREAM_CAP)
}

fn playback_known_duration(item: &MediaQueueItem) -> Option<Duration> {
    item.duration_ms
        .and_then(|duration_ms| u64::try_from(duration_ms).ok())
        .map(Duration::from_millis)
        .filter(|duration| !duration.is_zero())
}

fn offset_for_started_at(started_at: DateTime<Utc>) -> u64 {
    Utc::now()
        .signed_duration_since(started_at)
        .to_std()
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn offset_from_started_at_ms(started_at_ms: i64) -> u64 {
    Utc::now()
        .timestamp_millis()
        .saturating_sub(started_at_ms)
        .try_into()
        .unwrap_or_default()
}

fn skip_threshold(paired_total: usize) -> u32 {
    let total = paired_total as f32;
    let value = (total * 0.1).ceil() as u32;
    value.max(1)
}

fn booth_submit_error_message(err: &anyhow::Error) -> String {
    let text = format!("{err:#}").to_ascii_lowercase();
    if text.contains("invalid url") || text.contains("youtube") && text.contains("not found") {
        "Invalid YouTube URL".to_string()
    } else if text.contains("rate limit") || text.contains("submission rate limit") {
        "Slow down - too many submissions".to_string()
    } else if text.contains("not public") {
        "Video is not public".to_string()
    } else if text.contains("not embeddable") {
        "Video is not embeddable".to_string()
    } else if text.contains("api key") || text.contains("youtube data api") {
        "YouTube validation failed - try again".to_string()
    } else {
        "Failed to submit".to_string()
    }
}

fn booth_vote_error_message(err: &anyhow::Error) -> String {
    let text = format!("{err:#}").to_ascii_lowercase();
    if text.contains("voting closed") {
        "Voting closed - track started".to_string()
    } else if text.contains("pair a client") {
        "Pair a client to skip-vote".to_string()
    } else if text.contains("nothing is playing") {
        "Nothing is playing".to_string()
    } else if text.contains("queue item not found")
        || text.contains("queue item is no longer voteable")
    {
        "Item is no longer in the queue".to_string()
    } else {
        "Vote failed".to_string()
    }
}

fn trusted_submit_error_message(err: &anyhow::Error) -> String {
    let text = format!("{err:#}").to_ascii_lowercase();
    if text.contains("invalid url")
        || text.contains("unsupported youtube url")
        || text.contains("invalid youtube video id")
    {
        "Invalid YouTube URL".to_string()
    } else if text.contains("rate limit") {
        "Slow down — too many submissions".to_string()
    } else {
        "Failed to queue audio".to_string()
    }
}

fn fallback_load_event(source: &MediaSource) -> AudioWsMessage {
    AudioWsMessage::LoadVideo {
        item_id: source.id,
        video_id: source.external_id.clone(),
        started_at_ms: Utc::now().timestamp_millis(),
        offset_ms: 0,
        is_stream: source.is_stream,
    }
}

fn queue_item_view(
    item: MediaQueueItem,
    vote_score: i32,
    usernames: &HashMap<Uuid, String>,
) -> QueueItemView {
    QueueItemView {
        id: item.id,
        video_id: item.external_id,
        title: item.title,
        channel: item.channel,
        duration_ms: item.duration_ms,
        started_at_ms: item.started_at.map(|at| at.timestamp_millis()),
        is_stream: item.is_stream,
        submitter: usernames
            .get(&item.submitter_id)
            .cloned()
            .unwrap_or_default(),
        submitter_id: item.submitter_id,
        vote_score,
    }
}

#[cfg(test)]
mod tests {
    use super::skip_threshold;

    #[test]
    fn skip_threshold_floors_at_one_and_uses_ten_percent_ceil() {
        // No pairings → still need one vote to skip (avoids divide-by-zero
        // making the threshold trivially satisfiable).
        assert_eq!(skip_threshold(0), 1);
        // Small rooms collapse to threshold 1: any paired listener can skip.
        assert_eq!(skip_threshold(1), 1);
        assert_eq!(skip_threshold(5), 1);
        assert_eq!(skip_threshold(9), 1);
        assert_eq!(skip_threshold(10), 1);
        // 10% ceil kicks in above 10 paired clients.
        assert_eq!(skip_threshold(11), 2);
        assert_eq!(skip_threshold(20), 2);
        assert_eq!(skip_threshold(21), 3);
        assert_eq!(skip_threshold(25), 3);
        assert_eq!(skip_threshold(91), 10);
        assert_eq!(skip_threshold(100), 10);
        assert_eq!(skip_threshold(101), 11);
    }
}
