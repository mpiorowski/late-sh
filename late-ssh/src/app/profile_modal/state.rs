use late_core::models::profile::Profile;
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::chat::showcase::svc::{ShowcaseFeedItem, ShowcaseService, ShowcaseSnapshot};
use crate::app::profile::svc::{ProfileService, ProfileSnapshot};

pub struct ProfileModalState {
    profile_service: ProfileService,
    showcase_service: ShowcaseService,
    showcase_snapshot_rx: watch::Receiver<ShowcaseSnapshot>,
    showcases: Vec<ShowcaseFeedItem>,
    viewed_user_id: Option<Uuid>,
    fallback_name: String,
    profile: Option<Profile>,
    snapshot_rx: Option<watch::Receiver<ProfileSnapshot>>,
    scroll_offset: u16,
}

impl Drop for ProfileModalState {
    fn drop(&mut self) {
        self.prune_current_channel();
    }
}

impl ProfileModalState {
    pub fn new(profile_service: ProfileService, showcase_service: ShowcaseService) -> Self {
        let showcase_snapshot_rx = showcase_service.subscribe_snapshot();
        let showcases = showcase_snapshot_rx.borrow().items.clone();
        Self {
            profile_service,
            showcase_service,
            showcase_snapshot_rx,
            showcases,
            viewed_user_id: None,
            fallback_name: String::new(),
            profile: None,
            snapshot_rx: None,
            scroll_offset: 0,
        }
    }

    pub fn open(&mut self, user_id: Uuid, fallback_name: impl Into<String>) {
        self.prune_current_channel();
        self.viewed_user_id = Some(user_id);
        self.fallback_name = fallback_name.into();
        self.scroll_offset = 0;
        let mut snapshot_rx = self.profile_service.subscribe_snapshot(user_id);
        self.profile = profile_from_snapshot(snapshot_rx.borrow().clone(), Some(user_id));
        snapshot_rx.mark_changed();
        self.snapshot_rx = Some(snapshot_rx);
        self.profile_service.find_profile(user_id);
        self.showcase_service.list_task();
    }

    pub fn close(&mut self) {
        self.prune_current_channel();
        self.viewed_user_id = None;
        self.fallback_name.clear();
        self.profile = None;
        self.scroll_offset = 0;
        self.snapshot_rx = None;
    }

    pub fn tick(&mut self) {
        if let Ok(true) = self.showcase_snapshot_rx.has_changed() {
            self.showcases = self.showcase_snapshot_rx.borrow_and_update().items.clone();
        }

        let Some(rx) = &mut self.snapshot_rx else {
            return;
        };

        match rx.has_changed() {
            Ok(true) => {
                let snapshot = rx.borrow_and_update();
                self.profile = profile_from_snapshot(snapshot.clone(), self.viewed_user_id);
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!(%e, "failed to receive profile modal snapshot");
            }
        }
    }

    pub fn showcases_for_viewed(&self) -> Vec<&ShowcaseFeedItem> {
        let Some(user_id) = self.viewed_user_id else {
            return Vec::new();
        };
        self.showcases
            .iter()
            .filter(|item| item.showcase.user_id == user_id)
            .collect()
    }

    pub fn title(&self) -> String {
        if let Some(profile) = &self.profile
            && !profile.username.trim().is_empty()
        {
            return format!("Profile · {}", profile.username.trim());
        }
        if self.fallback_name.trim().is_empty() {
            "Profile".to_string()
        } else {
            format!("Profile · {}", self.fallback_name.trim())
        }
    }

    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    pub fn loading(&self) -> bool {
        self.profile.is_none()
    }

    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    pub fn scroll_by(&mut self, delta: i16) {
        let next = self.scroll_offset as i32 + delta as i32;
        self.scroll_offset = next.clamp(0, u16::MAX as i32) as u16;
    }

    fn prune_current_channel(&self) {
        if let Some(user_id) = self.viewed_user_id {
            self.profile_service.prune_user_snapshot_channel(user_id);
        }
    }
}

fn profile_from_snapshot(
    snapshot: ProfileSnapshot,
    viewed_user_id: Option<Uuid>,
) -> Option<Profile> {
    if snapshot.user_id == viewed_user_id {
        snapshot.profile
    } else {
        None
    }
}
