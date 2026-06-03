use std::cell::{Cell, RefCell};

use late_core::models::bonsai::Tree;
use late_core::models::profile::Profile;
use ratatui::layout::Rect;
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::bonsai::svc::BonsaiService;
use crate::app::bonsai_v2::state::BonsaiV2State;
use crate::app::chat::showcase::svc::{ShowcaseFeedItem, ShowcaseService, ShowcaseSnapshot};
use crate::app::hub::aquarium::state::AquariumState;
use crate::app::profile::svc::{ProfileService, ProfileSnapshot};

use super::badges::{Badge, badges_for};

/// Tabs for the compact fallback layout (small terminals). The dashboard shows
/// everything at once and ignores this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProfileTab {
    Overview,
    Bonsai,
    Aquarium,
    Badges,
}

impl ProfileTab {
    pub(crate) const ALL: [ProfileTab; 4] = [
        ProfileTab::Overview,
        ProfileTab::Bonsai,
        ProfileTab::Aquarium,
        ProfileTab::Badges,
    ];

    pub(crate) fn title(self) -> &'static str {
        match self {
            ProfileTab::Overview => "Overview",
            ProfileTab::Bonsai => "Bonsai",
            ProfileTab::Aquarium => "Aquarium",
            ProfileTab::Badges => "Badges",
        }
    }

    fn index(self) -> usize {
        ProfileTab::ALL
            .iter()
            .position(|tab| *tab == self)
            .unwrap_or(0)
    }
}

pub struct ProfileModalState {
    profile_service: ProfileService,
    showcase_service: ShowcaseService,
    bonsai_service: BonsaiService,
    showcase_snapshot_rx: watch::Receiver<ShowcaseSnapshot>,
    showcases: Vec<ShowcaseFeedItem>,
    viewed_user_id: Option<Uuid>,
    fallback_name: String,
    profile: Option<Profile>,
    chip_balance: Option<i64>,
    bonsai: Option<Tree>,
    /// Read-only Dynamic Bonsai for the viewed user. Built non-persisting, so
    /// viewing never mutates the owner's tree. Standard 2D render.
    bonsai_v2: Option<BonsaiV2State>,
    dynamic_bonsai_selected: bool,
    aquarium_fish: Vec<(String, usize)>,
    /// Lazily built/ticked for the aquarium panel. Interior mutability so the
    /// immutable `draw` path can animate and rebuild on resize.
    aquarium: RefCell<Option<AquariumState>>,
    aquarium_area: Cell<Rect>,
    badges: Vec<Badge>,
    tab: ProfileTab,
    snapshot_rx: Option<watch::Receiver<ProfileSnapshot>>,
    scroll_offset: u16,
}

impl Drop for ProfileModalState {
    fn drop(&mut self) {
        self.prune_current_channel();
    }
}

impl ProfileModalState {
    pub fn new(
        profile_service: ProfileService,
        showcase_service: ShowcaseService,
        bonsai_service: BonsaiService,
    ) -> Self {
        let showcase_snapshot_rx = showcase_service.subscribe_snapshot();
        let showcases = showcase_snapshot_rx.borrow().items.clone();
        Self {
            profile_service,
            showcase_service,
            bonsai_service,
            showcase_snapshot_rx,
            showcases,
            viewed_user_id: None,
            fallback_name: String::new(),
            profile: None,
            chip_balance: None,
            bonsai: None,
            bonsai_v2: None,
            dynamic_bonsai_selected: false,
            aquarium_fish: Vec::new(),
            aquarium: RefCell::new(None),
            aquarium_area: Cell::new(Rect::default()),
            badges: Vec::new(),
            tab: ProfileTab::Overview,
            snapshot_rx: None,
            scroll_offset: 0,
        }
    }

    pub fn open(&mut self, user_id: Uuid, fallback_name: impl Into<String>) {
        self.prune_current_channel();
        self.viewed_user_id = Some(user_id);
        self.fallback_name = fallback_name.into();
        self.scroll_offset = 0;
        self.tab = ProfileTab::Overview;
        self.badges = badges_for(user_id);
        self.aquarium_fish.clear();
        *self.aquarium.get_mut() = None;
        let mut snapshot_rx = self.profile_service.subscribe_snapshot(user_id);
        let snapshot = snapshot_rx.borrow().clone();
        self.apply_snapshot(snapshot);
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
        self.chip_balance = None;
        self.bonsai = None;
        self.bonsai_v2 = None;
        self.dynamic_bonsai_selected = false;
        self.aquarium_fish.clear();
        *self.aquarium.get_mut() = None;
        self.badges.clear();
        self.tab = ProfileTab::Overview;
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
                let snapshot = rx.borrow_and_update().clone();
                self.apply_snapshot(snapshot);
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!(%e, "failed to receive profile modal snapshot");
            }
        }
    }

    fn apply_snapshot(&mut self, snapshot: ProfileSnapshot) {
        let matches = self.viewed_user_id.is_some() && snapshot.user_id == self.viewed_user_id;
        if !matches {
            self.profile = None;
            self.chip_balance = None;
            self.bonsai = None;
            self.bonsai_v2 = None;
            self.dynamic_bonsai_selected = false;
            if !self.aquarium_fish.is_empty() {
                self.aquarium_fish.clear();
                *self.aquarium.get_mut() = None;
            }
            return;
        }

        self.profile = snapshot.profile;
        self.chip_balance = snapshot.chip_balance;
        self.bonsai = snapshot.bonsai;
        self.dynamic_bonsai_selected = snapshot.dynamic_bonsai_selected;

        if snapshot.aquarium_fish != self.aquarium_fish {
            self.aquarium_fish = snapshot.aquarium_fish;
            *self.aquarium.get_mut() = None;
        }

        self.bonsai_v2 = match (
            self.dynamic_bonsai_selected,
            self.viewed_user_id,
            snapshot.bonsai_v2,
        ) {
            (true, Some(user_id), Some(tree)) => Some(BonsaiV2State::view_only(
                user_id,
                self.bonsai_service.clone(),
                tree,
            )),
            _ => None,
        };
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

    pub(crate) fn tab(&self) -> ProfileTab {
        self.tab
    }

    pub(crate) fn set_tab(&mut self, tab: ProfileTab) {
        if self.tab != tab {
            self.tab = tab;
            self.scroll_offset = 0;
        }
    }

    pub(crate) fn cycle_tab(&mut self, delta: isize) {
        let len = ProfileTab::ALL.len() as isize;
        let next = (self.tab.index() as isize + delta).rem_euclid(len) as usize;
        self.set_tab(ProfileTab::ALL[next]);
    }

    pub fn bonsai(&self) -> Option<&Tree> {
        self.bonsai.as_ref()
    }

    pub(crate) fn bonsai_v2(&self) -> Option<&BonsaiV2State> {
        self.bonsai_v2.as_ref()
    }

    pub(crate) fn dynamic_bonsai_selected(&self) -> bool {
        self.dynamic_bonsai_selected
    }

    pub(crate) fn aquarium_fish(&self) -> &[(String, usize)] {
        &self.aquarium_fish
    }

    pub(crate) fn aquarium_cell(&self) -> &RefCell<Option<AquariumState>> {
        &self.aquarium
    }

    pub(crate) fn aquarium_area(&self) -> &Cell<Rect> {
        &self.aquarium_area
    }

    pub(crate) fn badges(&self) -> &[Badge] {
        &self.badges
    }

    pub fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    pub fn chip_balance(&self) -> Option<i64> {
        self.chip_balance
    }

    pub(crate) fn fallback_name(&self) -> &str {
        &self.fallback_name
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
