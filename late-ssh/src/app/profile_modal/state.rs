use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use late_core::models::artboard_piece::GalleryCounts;
use late_core::models::bonsai::Tree;
use late_core::models::chat_message_gild::GildCounts;
use late_core::models::chips::ChipLedgerEntry;
use late_core::models::profile::Profile;
use late_core::models::profile_award::ProfileAward;
use ratatui::layout::Rect;
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::bonsai::svc::BonsaiService;
use crate::app::bonsai_v2::state::BonsaiV2State;
use crate::app::chat::showcase::svc::{ShowcaseFeedItem, ShowcaseService, ShowcaseSnapshot};
use crate::app::hub::aquarium::state::AquariumState;
use crate::app::profile::svc::{ProfileService, ProfileSnapshot};

/// The vertical extent the last draw measured: how tall the composed body
/// is, how many rows the viewport showed, and where the chips section
/// starts. `draw` takes `&self`, so these are interior-mutable, the same
/// way `popup_area` is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScrollExtent {
    pub(crate) content_height: u16,
    pub(crate) viewport_height: u16,
    pub(crate) chips_top: Option<u16>,
}

impl ScrollExtent {
    fn max_offset(self) -> u16 {
        self.content_height.saturating_sub(self.viewport_height)
    }
}

pub(crate) struct ProfileModalState {
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
    /// The modal's outer popup rect from the last render, so a click landing
    /// outside it can dismiss the modal. Interior-mutable: published by the
    /// immutable `draw` path.
    popup_area: Cell<Rect>,
    profile_awards: Vec<ProfileAward>,
    gild_counts: GildCounts,
    gallery_counts: GalleryCounts,
    chip_ledger: Vec<ChipLedgerEntry>,
    chips_earned_month: i64,
    ledger_usernames: HashMap<Uuid, String>,
    snapshot_rx: Option<watch::Receiver<ProfileSnapshot>>,
    /// First body row shown. Clamped against `extent` on every move, and
    /// again by `draw` once the body has been measured.
    scroll_offset: Cell<u16>,
    extent: Cell<ScrollExtent>,
    /// Set by `/chips`: the next draw that knows where the chips section
    /// is scrolls there and clears this.
    jump_to_chips: Cell<bool>,
}

impl Drop for ProfileModalState {
    fn drop(&mut self) {
        self.prune_current_channel();
    }
}

impl ProfileModalState {
    pub(crate) fn new(
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
            popup_area: Cell::new(Rect::default()),
            profile_awards: Vec::new(),
            gild_counts: GildCounts::default(),
            gallery_counts: GalleryCounts::default(),
            chip_ledger: Vec::new(),
            chips_earned_month: 0,
            ledger_usernames: HashMap::new(),
            snapshot_rx: None,
            scroll_offset: Cell::new(0),
            extent: Cell::new(ScrollExtent::default()),
            jump_to_chips: Cell::new(false),
        }
    }

    pub(crate) fn open(&mut self, user_id: Uuid, fallback_name: impl Into<String>) {
        self.prune_current_channel();
        self.viewed_user_id = Some(user_id);
        self.fallback_name = fallback_name.into();
        self.scroll_offset.set(0);
        self.extent.set(ScrollExtent::default());
        self.jump_to_chips.set(false);
        self.profile_awards.clear();
        self.gild_counts = GildCounts::default();
        self.gallery_counts = GalleryCounts::default();
        self.chip_ledger.clear();
        self.chips_earned_month = 0;
        self.ledger_usernames.clear();
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

    /// `/chips`: land on the chips section as soon as the body has been
    /// measured (the next draw).
    pub(crate) fn jump_to_chips(&self) {
        self.jump_to_chips.set(true);
    }

    pub(crate) fn close(&mut self) {
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
        self.profile_awards.clear();
        self.gild_counts = GildCounts::default();
        self.gallery_counts = GalleryCounts::default();
        self.chip_ledger.clear();
        self.chips_earned_month = 0;
        self.ledger_usernames.clear();
        self.scroll_offset.set(0);
        self.extent.set(ScrollExtent::default());
        self.jump_to_chips.set(false);
        self.snapshot_rx = None;
    }

    /// Returns true when this tick drained a snapshot into the open modal.
    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Ok(true) = self.showcase_snapshot_rx.has_changed() {
            self.showcases = self.showcase_snapshot_rx.borrow_and_update().items.clone();
            changed = true;
        }

        let Some(rx) = &mut self.snapshot_rx else {
            return changed;
        };

        match rx.has_changed() {
            Ok(true) => {
                let snapshot = rx.borrow_and_update().clone();
                self.apply_snapshot(snapshot);
                changed = true;
            }
            Ok(false) => {}
            Err(e) => {
                tracing::error!(%e, "failed to receive profile modal snapshot");
            }
        }
        changed
    }

    /// True while the modal draws a live aquarium (the viewed profile owns
    /// fish): the reef ticks during draw, so it needs frames while visible.
    pub(crate) fn aquarium_animating(&self) -> bool {
        !self.aquarium_fish.is_empty()
    }

    fn apply_snapshot(&mut self, snapshot: ProfileSnapshot) {
        let matches = self.viewed_user_id.is_some() && snapshot.user_id == self.viewed_user_id;
        if !matches {
            self.profile = None;
            self.chip_balance = None;
            self.bonsai = None;
            self.bonsai_v2 = None;
            self.dynamic_bonsai_selected = false;
            self.profile_awards.clear();
            self.gild_counts = GildCounts::default();
            self.gallery_counts = GalleryCounts::default();
            self.chip_ledger.clear();
            self.chips_earned_month = 0;
            self.ledger_usernames.clear();
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
        self.profile_awards = snapshot.profile_awards;
        self.gild_counts = snapshot.gild_counts;
        self.gallery_counts = snapshot.gallery_counts;
        self.chip_ledger = snapshot.chip_ledger;
        self.chips_earned_month = snapshot.chips_earned_month;
        self.ledger_usernames = snapshot.ledger_usernames;

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
                snapshot.bonsai_decay_protection,
            )),
            _ => None,
        };
    }

    pub(crate) fn showcases_for_viewed(&self) -> Vec<&ShowcaseFeedItem> {
        let Some(user_id) = self.viewed_user_id else {
            return Vec::new();
        };
        self.showcases
            .iter()
            .filter(|item| item.showcase.user_id == user_id)
            .collect()
    }

    pub(crate) fn bonsai(&self) -> Option<&Tree> {
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

    /// Advance the open modal's live reef one animation step. App::tick
    /// drives this on its half-rate edge; draw only paints. False while no
    /// reef is built yet (it is built lazily by the first draw, which the
    /// modal-opening input frame forces).
    pub(crate) fn step_reef(&mut self) -> bool {
        match self.aquarium.get_mut() {
            Some(aquarium) => {
                aquarium.tick();
                true
            }
            None => false,
        }
    }

    pub(crate) fn aquarium_area(&self) -> &Cell<Rect> {
        &self.aquarium_area
    }

    /// Record the outer popup rect each render, so `input` can tell an inside
    /// click from an outside one (click-outside dismisses the modal).
    pub(crate) fn set_popup_area(&self, area: Rect) {
        self.popup_area.set(area);
    }

    pub(crate) fn popup_area(&self) -> Rect {
        self.popup_area.get()
    }

    pub(crate) fn profile_awards(&self) -> &[ProfileAward] {
        &self.profile_awards
    }

    pub(crate) fn gild_counts(&self) -> GildCounts {
        self.gild_counts
    }

    pub(crate) fn gallery_counts(&self) -> GalleryCounts {
        self.gallery_counts
    }

    pub(crate) fn chip_ledger(&self) -> &[ChipLedgerEntry] {
        &self.chip_ledger
    }

    pub(crate) fn chips_earned_month(&self) -> i64 {
        self.chips_earned_month
    }

    pub(crate) fn ledger_username(&self, user_id: Uuid) -> Option<&str> {
        self.ledger_usernames.get(&user_id).map(String::as_str)
    }

    pub(crate) fn profile(&self) -> Option<&Profile> {
        self.profile.as_ref()
    }

    pub(crate) fn chip_balance(&self) -> Option<i64> {
        self.chip_balance
    }

    pub(crate) fn fallback_name(&self) -> &str {
        &self.fallback_name
    }

    pub(crate) fn scroll_offset(&self) -> u16 {
        self.scroll_offset.get()
    }

    pub(crate) fn scroll_by(&self, delta: i16) {
        let next = self.scroll_offset.get() as i32 + delta as i32;
        self.scroll_offset
            .set(next.clamp(0, self.extent.get().max_offset() as i32) as u16);
    }

    pub(crate) fn scroll_to_top(&self) {
        self.scroll_offset.set(0);
    }

    pub(crate) fn scroll_to_bottom(&self) {
        self.scroll_offset.set(self.extent.get().max_offset());
    }

    /// Record what the last draw measured, then settle the offset: clamp it
    /// to the new extent, and honour a pending `/chips` jump once the chips
    /// section has a known row. Called by `draw`.
    pub(crate) fn set_scroll_extent(&self, extent: ScrollExtent) {
        self.extent.set(extent);
        if self.jump_to_chips.get()
            && let Some(top) = extent.chips_top
        {
            self.scroll_offset.set(top);
            self.jump_to_chips.set(false);
        }
        self.scroll_by(0);
    }

    fn prune_current_channel(&self) {
        if let Some(user_id) = self.viewed_user_id {
            self.profile_service.prune_user_snapshot_channel(user_id);
        }
    }
}
