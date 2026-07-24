use late_core::models::user::{AudioSource, IcecastStream, RadioStation};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{AudioEvent, AudioService, QueueSnapshot};
use crate::app::common::primitives::Banner;

pub struct AudioTick {
    pub banner: Option<Banner>,
    /// True when the shared queue snapshot moved this tick: the booth modal
    /// and the sidebar music stage draw straight from it.
    pub changed: bool,
}

pub struct AudioState {
    pub(crate) service: AudioService,
    user_id: Uuid,
    event_rx: broadcast::Receiver<AudioEvent>,
    snapshot_rx: watch::Receiver<QueueSnapshot>,
}

impl AudioState {
    pub fn new(service: AudioService, user_id: Uuid) -> Self {
        let event_rx = service.subscribe_events();
        let snapshot_rx = service.subscribe_snapshot();
        Self {
            service,
            user_id,
            event_rx,
            snapshot_rx,
        }
    }

    pub fn queue_snapshot(&self) -> QueueSnapshot {
        self.snapshot_rx.borrow().clone()
    }

    pub fn youtube_source_count(&self) -> usize {
        self.service.youtube_source_count()
    }

    pub fn icecast_source_count(&self) -> usize {
        self.service.icecast_source_count()
    }

    pub fn radio_source_count(&self) -> usize {
        self.service.radio_source_count()
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn service(&self) -> &AudioService {
        &self.service
    }

    pub fn submit_trusted(&self, url: String) {
        self.service.submit_trusted_url_task(self.user_id, url);
    }

    pub fn set_youtube_fallback(&self, url: String) {
        self.service
            .set_trusted_youtube_fallback_task(self.user_id, url);
    }

    pub fn skip_trusted(&self) {
        self.service.force_skip_task(self.user_id);
    }

    pub fn booth_submit_enabled(&self) -> bool {
        self.service.booth_submit_enabled()
    }

    pub fn booth_submit_public(&self, url: String) {
        self.service.booth_submit_public_task(self.user_id, url);
    }

    pub fn booth_vote(&self, item_id: Uuid, value: i16) {
        self.service.cast_vote_task(self.user_id, item_id, value);
    }

    pub fn booth_clear_vote(&self, item_id: Uuid) {
        self.service.clear_vote_task(self.user_id, item_id);
    }

    pub fn booth_skip_vote(&self) {
        self.service.cast_skip_vote_task(self.user_id);
    }

    pub fn booth_delete(&self, item_id: Uuid) {
        self.service.delete_queue_item_task(self.user_id, item_id);
    }

    pub fn booth_toggle_unskippable(&self, item_id: Uuid) {
        self.service.toggle_unskippable_task(self.user_id, item_id);
    }

    pub fn booth_history_requeue(&self, item_id: Uuid) {
        self.service
            .requeue_history_item_task(self.user_id, item_id);
    }

    pub fn booth_history_delete(&self, item_id: Uuid) {
        self.service.delete_history_item_task(self.user_id, item_id);
    }

    /// Spawn an audio-source persist task that surfaces failures as banners
    /// via `AudioEvent::AudioSourcePersistFailed`. Caller is expected to have
    /// already optimistically updated local UI state.
    pub fn persist_audio_source(&self, source: AudioSource) {
        self.service.persist_audio_source_task(self.user_id, source);
    }

    pub fn persist_icecast_stream(&self, stream: IcecastStream) {
        self.service
            .persist_icecast_stream_task(self.user_id, stream);
    }

    pub fn persist_radio_station(&self, station: RadioStation) {
        self.service
            .persist_radio_station_task(self.user_id, station);
    }

    pub fn tick(&mut self) -> AudioTick {
        // Mark the queue snapshot seen so the peek only fires once per
        // publish; renders keep reading it through `queue_snapshot()`.
        let changed = self.snapshot_rx.has_changed().unwrap_or(false);
        if changed {
            let _ = self.snapshot_rx.borrow_and_update();
        }
        let mut banner = None;
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AudioEvent::TrustedSubmitQueued { user_id, position }
                    if user_id == self.user_id =>
                {
                    banner = Some(if position == 0 {
                        Banner::success("Queued audio - up next")
                    } else {
                        Banner::success(&format!("Queued audio - #{position} in line"))
                    });
                }
                AudioEvent::TrustedSubmitFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::YoutubeFallbackSet { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Set YouTube fallback"));
                }
                AudioEvent::YoutubeFallbackFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::TrustedSkipFired { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Skipped audio"));
                }
                AudioEvent::TrustedSkipFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothSubmitQueued { user_id, position } if user_id == self.user_id => {
                    banner = Some(if position == 0 {
                        Banner::success("Submitted - up next")
                    } else {
                        Banner::success(&format!("Submitted - #{position} in line"))
                    });
                }
                AudioEvent::BoothSubmitFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothVoteApplied { user_id, score, .. } if user_id == self.user_id => {
                    banner = Some(Banner::success(&format!("Vote registered (score {score})")));
                }
                AudioEvent::BoothVoteFailed { user_id, message } if user_id == self.user_id => {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothSkipFired { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Skip threshold reached"));
                }
                AudioEvent::BoothItemDeleted { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Deleted track"));
                }
                AudioEvent::BoothItemDeleteFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothItemUnskippableToggled {
                    user_id,
                    unskippable,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(if unskippable {
                        "Locked - skip-vote disabled"
                    } else {
                        "Unlocked - skip-vote enabled"
                    }));
                }
                AudioEvent::BoothItemUnskippableFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothSkipProgress {
                    user_id,
                    votes,
                    threshold,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(&format!(
                        "Skip vote registered ({votes}/{threshold})"
                    )));
                }
                AudioEvent::BoothHistoryRequeued { user_id, position }
                    if user_id == self.user_id =>
                {
                    banner = Some(if position == 0 {
                        Banner::success("Queued from history - up next")
                    } else {
                        Banner::success(&format!("Queued from history - #{position} in line"))
                    });
                }
                AudioEvent::BoothHistoryRequeueFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::BoothHistoryItemDeleted { user_id } if user_id == self.user_id => {
                    banner = Some(Banner::success("Deleted history track"));
                }
                AudioEvent::BoothHistoryItemDeleteFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                AudioEvent::AudioSourcePersistFailed { user_id, message }
                    if user_id == self.user_id =>
                {
                    banner = Some(Banner::error(&message));
                }
                _ => {}
            }
        }
        AudioTick { banner, changed }
    }
}
