use late_core::models::profile::{Profile, ProfileParams};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LayoutVisibility {
    pub(crate) show_room_list_sidebar: bool,
    pub(crate) show_right_sidebar: bool,
    pub(crate) show_dashboard_header: bool,
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

    pub fn toggle_favorite_room(&mut self, room_id: Uuid) -> bool {
        let added = if let Some(index) = self
            .profile
            .favorite_room_ids
            .iter()
            .position(|id| *id == room_id)
        {
            self.profile.favorite_room_ids.remove(index);
            false
        } else {
            self.profile.favorite_room_ids.push(room_id);
            true
        };
        self.save_profile();
        added
    }

    pub(crate) fn cycle_layout_visibility(&mut self) -> LayoutVisibility {
        const PRESETS: [LayoutVisibility; 8] = [
            LayoutVisibility {
                show_room_list_sidebar: true,
                show_right_sidebar: true,
                show_dashboard_header: true,
            },
            LayoutVisibility {
                show_room_list_sidebar: false,
                show_right_sidebar: true,
                show_dashboard_header: true,
            },
            LayoutVisibility {
                show_room_list_sidebar: true,
                show_right_sidebar: false,
                show_dashboard_header: true,
            },
            LayoutVisibility {
                show_room_list_sidebar: true,
                show_right_sidebar: true,
                show_dashboard_header: false,
            },
            LayoutVisibility {
                show_room_list_sidebar: false,
                show_right_sidebar: false,
                show_dashboard_header: true,
            },
            LayoutVisibility {
                show_room_list_sidebar: false,
                show_right_sidebar: true,
                show_dashboard_header: false,
            },
            LayoutVisibility {
                show_room_list_sidebar: true,
                show_right_sidebar: false,
                show_dashboard_header: false,
            },
            LayoutVisibility {
                show_room_list_sidebar: false,
                show_right_sidebar: false,
                show_dashboard_header: false,
            },
        ];

        let current = LayoutVisibility {
            show_room_list_sidebar: self.profile.show_room_list_sidebar,
            show_right_sidebar: self.profile.show_right_sidebar,
            show_dashboard_header: self.profile.show_dashboard_header,
        };
        let index = PRESETS
            .iter()
            .position(|preset| *preset == current)
            .unwrap_or(0);
        let next = PRESETS[(index + 1) % PRESETS.len()];

        self.profile.show_room_list_sidebar = next.show_room_list_sidebar;
        self.profile.show_right_sidebar = next.show_right_sidebar;
        self.profile.show_dashboard_header = next.show_dashboard_header;
        self.save_profile();
        next
    }

    fn save_profile(&self) {
        self.profile_service
            .edit_profile(self.user_id, profile_params_from_profile(&self.profile));
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
                    let fallback = self.theme_id().to_string();
                    let normalized =
                        theme::normalize_id(profile.theme_id.as_deref().unwrap_or(&fallback));
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

fn profile_params_from_profile(profile: &Profile) -> ProfileParams {
    ProfileParams {
        username: profile.username.clone(),
        bio: profile.bio.clone(),
        country: profile.country.clone(),
        timezone: profile.timezone.clone(),
        ide: profile.ide.clone(),
        terminal: profile.terminal.clone(),
        os: profile.os.clone(),
        langs: profile.langs.clone(),
        notify_kinds: profile.notify_kinds.clone(),
        notify_bell: profile.notify_bell,
        notify_cooldown_mins: profile.notify_cooldown_mins,
        notify_format: profile.notify_format.clone(),
        theme_id: Some(
            profile
                .theme_id
                .clone()
                .unwrap_or_else(|| theme::DEFAULT_ID.to_string()),
        ),
        enable_background_color: profile.enable_background_color,
        show_dashboard_header: profile.show_dashboard_header,
        show_right_sidebar: profile.show_right_sidebar,
        show_room_list_sidebar: profile.show_room_list_sidebar,
        show_settings_on_connect: profile.show_settings_on_connect,
        favorite_room_ids: profile.favorite_room_ids.clone(),
    }
}
