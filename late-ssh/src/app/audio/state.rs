use tokio::sync::broadcast;
use uuid::Uuid;

use super::svc::{AudioEvent, AudioService};
use crate::app::common::primitives::Banner;

pub struct AudioState {
    pub(crate) service: AudioService,
    user_id: Uuid,
    event_rx: broadcast::Receiver<AudioEvent>,
}

impl AudioState {
    pub fn new(service: AudioService, user_id: Uuid) -> Self {
        let event_rx = service.subscribe_events();
        Self {
            service,
            user_id,
            event_rx,
        }
    }

    pub fn submit_trusted(&self, url: String) {
        self.service.submit_trusted_url_task(self.user_id, url);
    }

    pub fn set_youtube_fallback(&self, url: String) {
        self.service
            .set_trusted_youtube_fallback_task(self.user_id, url);
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
                _ => {}
            }
        }
        banner
    }
}
