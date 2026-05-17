use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{AudioEvent, AudioService, QueueSnapshot};
use crate::app::common::primitives::Banner;

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

    pub fn tick(&mut self) -> Option<Banner> {
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
                AudioEvent::BoothSubmitQueued { user_id, position }
                    if user_id == self.user_id =>
                {
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
                AudioEvent::BoothSkipProgress {
                    user_id,
                    votes,
                    threshold,
                } if user_id == self.user_id => {
                    banner = Some(Banner::success(&format!(
                        "Skip vote registered ({votes}/{threshold})"
                    )));
                }
                _ => {}
            }
        }
        banner
    }
}
