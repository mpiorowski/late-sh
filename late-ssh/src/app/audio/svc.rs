use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    db::Db,
    models::{media_queue_item::MediaQueueItem, user::User},
};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, broadcast, oneshot};
use uuid::Uuid;

use super::youtube::YoutubeClient;

const QUEUE_SNAPSHOT_LIMIT: i64 = 50;
const MAX_SUBMISSIONS_PER_WINDOW: i64 = 3;
const SUBMISSION_WINDOW: chrono::Duration = chrono::Duration::minutes(5);
const FALLBACK_DEBOUNCE: Duration = Duration::from_secs(10);
const PLAYBACK_SYNC_INTERVAL: Duration = Duration::from_secs(10);
const PLAYBACK_END_GRACE: Duration = Duration::from_secs(5);
const UNKNOWN_DURATION_ENDED_FLOOR: Duration = Duration::from_secs(30);
const STREAM_CAP: Duration = Duration::from_secs(60 * 60);

#[derive(Clone)]
pub struct AudioService {
    db: Db,
    youtube: YoutubeClient,
    event_tx: broadcast::Sender<AudioEvent>,
    state: Arc<Mutex<QueueState>>,
}

#[derive(Default)]
struct QueueState {
    mode: AudioMode,
    current_item_id: Option<Uuid>,
    sequence: u64,
    playback_cancel: Option<oneshot::Sender<()>>,
    fallback_cancel: Option<oneshot::Sender<()>>,
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
pub enum AudioEvent {
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
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct QueueSnapshot {
    pub audio_mode: AudioMode,
    pub current: Option<QueueItemView>,
    pub queue: Vec<QueueItemView>,
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
    pub fn new(db: Db, youtube_api_key: Option<String>) -> Self {
        let (event_tx, _) = broadcast::channel(512);
        Self {
            db,
            youtube: YoutubeClient::new(youtube_api_key),
            event_tx,
            state: Arc::new(Mutex::new(QueueState::default())),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AudioEvent> {
        self.event_tx.subscribe()
    }

    pub async fn start_background_task(self, shutdown: late_core::shutdown::CancellationToken) {
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

    async fn submit_video(
        &self,
        user_id: Uuid,
        video: super::youtube::YoutubeVideo,
        enforce_rate_limit: bool,
    ) -> Result<SubmitQueueResponse> {
        let mut state = self.state.lock().await;
        let client = self.db.get().await?;

        if enforce_rate_limit {
            let since = Utc::now() - SUBMISSION_WINDOW;
            let recent = MediaQueueItem::recent_submission_count(&client, user_id, since).await?;
            if recent >= MAX_SUBMISSIONS_PER_WINDOW {
                anyhow::bail!("submission rate limit exceeded");
            }
        }

        let item = MediaQueueItem::insert_youtube(
            &client,
            user_id,
            &video.video_id,
            video.title.as_deref(),
            video.channel.as_deref(),
            video.duration_ms,
            video.is_stream,
        )
        .await?;

        self.cancel_fallback(&mut state);
        if state.current_item_id.is_none() {
            self.advance_to_next_with_guard(&mut state).await?;
        } else {
            self.publish_queue_update_with_guard(&mut state).await?;
        }

        let position_in_queue = if state.current_item_id == Some(item.id) {
            0
        } else {
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

    pub async fn initial_events(&self) -> Result<Vec<AudioEvent>> {
        let state = self.state.lock().await;
        let snapshot = self.load_snapshot(state.mode).await?;
        let mut events = vec![
            AudioEvent::SourceChanged {
                audio_mode: snapshot.audio_mode,
            },
            AudioEvent::QueueUpdate {
                current: snapshot.current.clone(),
                queue: snapshot.queue.clone(),
                sequence: state.sequence,
            },
        ];
        if let Some(current) = &snapshot.current
            && let Some(started_at_ms) = current.started_at_ms
        {
            events.push(AudioEvent::LoadVideo {
                item_id: current.id,
                video_id: current.video_id.clone(),
                started_at_ms,
                offset_ms: offset_from_started_at_ms(started_at_ms),
                is_stream: current.is_stream,
            });
        }
        Ok(events)
    }

    async fn resume_from_db(&self) -> Result<()> {
        let mut state = self.state.lock().await;
        let client = self.db.get().await?;
        let now = Utc::now();

        if let Some(item) = MediaQueueItem::current_playing(&client).await? {
            if item_is_still_playable(&item, now) {
                state.current_item_id = Some(item.id);
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
        let duration = playback_known_duration(&item)
            .or_else(|| report.duration_ms.and_then(duration_from_report_ms));

        let Some(duration) = duration else {
            if elapsed >= UNKNOWN_DURATION_ENDED_FLOOR {
                tracing::info!(
                    item_id = %report.item_id,
                    elapsed_ms = elapsed.as_millis() as u64,
                    offset_ms = ?report.offset_ms,
                    "accepting browser ended report after unknown-duration floor"
                );
                drop(state);
                return self.finish_item(report.item_id).await;
            }

            tracing::debug!(
                item_id = %report.item_id,
                elapsed_ms = elapsed.as_millis() as u64,
                offset_ms = ?report.offset_ms,
                "ignoring early browser ended report without known duration"
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
        self.cancel_playback(&mut state);
        self.advance_to_next_with_guard(&mut state).await
    }

    async fn advance_to_next_with_guard(&self, state: &mut QueueState) -> Result<()> {
        let client = self.db.get().await?;
        if let Some(next) = MediaQueueItem::first_queued(&client).await? {
            self.cancel_fallback(state);
            let item = MediaQueueItem::mark_playing(&client, next.id, Utc::now()).await?;
            state.current_item_id = Some(item.id);
            state.mode = AudioMode::Youtube;
            self.schedule_playback_timer(state, &item);
            self.publish_source_change(AudioMode::Youtube);
            self.publish_load_video(&item);
            self.publish_queue_update_with_guard(state).await?;
            return Ok(());
        }

        state.current_item_id = None;
        self.cancel_playback(state);
        self.schedule_fallback(state);
        self.publish_queue_update_with_guard(state).await?;
        Ok(())
    }

    async fn publish_queue_update_with_guard(&self, state: &mut QueueState) -> Result<()> {
        state.sequence = state.sequence.saturating_add(1);
        let snapshot = self.load_snapshot(state.mode).await?;
        let _ = self.event_tx.send(AudioEvent::QueueUpdate {
            current: snapshot.current,
            queue: snapshot.queue,
            sequence: state.sequence,
        });
        Ok(())
    }

    async fn load_snapshot(&self, mode: AudioMode) -> Result<QueueSnapshot> {
        let client = self.db.get().await?;
        let items = MediaQueueItem::list_snapshot(&client, QUEUE_SNAPSHOT_LIMIT).await?;
        let user_ids = items
            .iter()
            .map(|item| item.submitter_id)
            .collect::<Vec<_>>();
        let usernames = User::list_usernames_by_ids(&client, &user_ids).await?;

        let mut current = None;
        let mut queue = Vec::new();
        for item in items {
            let view = queue_item_view(item, &usernames);
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
        })
    }

    fn publish_source_change(&self, mode: AudioMode) {
        let _ = self
            .event_tx
            .send(AudioEvent::SourceChanged { audio_mode: mode });
    }

    fn publish_load_video(&self, item: &MediaQueueItem) {
        let Some(started_at) = item.started_at else {
            return;
        };
        let _ = self.event_tx.send(AudioEvent::LoadVideo {
            item_id: item.id,
            video_id: item.external_id.clone(),
            started_at_ms: started_at.timestamp_millis(),
            offset_ms: offset_for_started_at(started_at),
            is_stream: item.is_stream,
        });
    }

    fn publish_seek_for_started_at(&self, started_at: DateTime<Utc>) {
        let _ = self.event_tx.send(AudioEvent::Seek {
            offset_ms: offset_for_started_at(started_at),
        });
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
        {
            if let Some(updated) =
                MediaQueueItem::set_duration_if_missing(&client, item_id, duration_ms).await?
            {
                let mut state = self.state.lock().await;
                if state.current_item_id == Some(item_id) {
                    self.schedule_playback_timer(&mut state, &updated);
                }
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

fn duration_from_report_ms(duration_ms: u64) -> Option<Duration> {
    if duration_ms == 0 || duration_ms > STREAM_CAP.as_millis() as u64 {
        return None;
    }
    Some(Duration::from_millis(duration_ms))
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

fn queue_item_view(item: MediaQueueItem, usernames: &HashMap<Uuid, String>) -> QueueItemView {
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
    }
}
