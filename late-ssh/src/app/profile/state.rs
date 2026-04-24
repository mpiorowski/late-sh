use late_core::models::profile::Profile;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use super::svc::{ProfileEvent, ProfileService, ProfileSnapshot};
use crate::app::common::{primitives::Banner, theme};

pub struct ProfileState {
    profile_service: ProfileService,
    user_id: Uuid,
    pub(crate) profile: Profile,
    snapshot_rx: watch::Receiver<ProfileSnapshot>,
    event_rx: broadcast::Receiver<ProfileEvent>,
}

impl Drop for ProfileState {
    fn drop(&mut self) {
        self.profile_service
            .prune_user_snapshot_channel(self.user_id);
    }
}

impl ProfileState {
    pub fn new(profile_service: ProfileService, user_id: Uuid, initial_theme_id: String) -> Self {
        let snapshot_rx = profile_service.subscribe_snapshot(user_id);
        let event_rx = profile_service.subscribe_events();
        profile_service.find_profile(user_id);
        let profile = Profile {
            theme_id: Some(theme::normalize_id(&initial_theme_id).to_string()),
            ..Profile::default()
        };
        Self {
            profile_service,
            user_id,
            profile,
            snapshot_rx,
            event_rx,
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn theme_id(&self) -> &str {
        self.profile
            .theme_id
            .as_deref()
            .unwrap_or_else(|| theme::normalize_id(""))
    }

    // Tick
    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        self.drain_events()
    }

    fn drain_snapshot(&mut self) {
        match self.snapshot_rx.has_changed() {
            Ok(true) => {
                let snapshot = self.snapshot_rx.borrow_and_update();
                if snapshot.user_id != Some(self.user_id) {
                    return;
                }
                let profile = snapshot.profile.clone();
                drop(snapshot);
                if let Some(mut profile) = profile {
                    let normalized = theme::normalize_id(profile.theme_id.as_deref().unwrap_or(""));
                    profile.theme_id = Some(normalized.to_string());
                    self.profile = profile;
                }
            }
            Ok(false) => (),
            Err(e) => {
                tracing::error!(%e, "failed to receive profile snapshot");
            }
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    ProfileEvent::Saved { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Profile saved!"));
                    }
                    ProfileEvent::Error { user_id, message } if self.user_id == user_id => {
                        banner = Some(Banner::error(&message));
                    }
                    _ => (),
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive profile event");
                    break;
                }
            }
        }
        banner
    }
}
