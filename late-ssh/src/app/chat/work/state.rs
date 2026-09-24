use chrono::{DateTime, Utc};
use late_core::models::work_profile::WorkProfileParams;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;

use super::svc::{WorkEvent, WorkFeedItem, WorkService, WorkSnapshot};

/// Outcome of one tab tick: the banner to surface plus whether a drained
/// snapshot or event may have changed the rendered tab (badge counts, work profile list).
pub struct WorkTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

/// The work card feed: every card, the viewer's read marker, and the calls
/// that write a card. Editing happens in the profile editor
/// (`app/directory/editor`); this state only carries what it saves.
pub struct State {
    service: WorkService,
    user_id: Uuid,
    is_admin: bool,
    source_items: Vec<WorkFeedItem>,
    items: Vec<WorkFeedItem>,
    mine_only: bool,
    selected: usize,
    snapshot_rx: watch::Receiver<WorkSnapshot>,
    event_rx: broadcast::Receiver<WorkEvent>,
    submitted: bool,
    unread_count: i64,
    last_read_at: Option<DateTime<Utc>>,
    marker_read_at: Option<DateTime<Utc>>,
    preserve_marker_read_at: bool,
}

impl State {
    pub fn new(service: WorkService, user_id: Uuid, is_admin: bool) -> Self {
        let state = Self::new_without_initial_load(service, user_id, is_admin);
        state.list();
        state.refresh_unread_count();
        state
    }

    pub fn new_without_initial_load(service: WorkService, user_id: Uuid, is_admin: bool) -> Self {
        let snapshot_rx = service.subscribe_snapshot();
        let event_rx = service.subscribe_events();
        Self {
            service,
            user_id,
            is_admin,
            source_items: Vec::new(),
            items: Vec::new(),
            mine_only: false,
            selected: 0,
            snapshot_rx,
            event_rx,
            submitted: false,
            unread_count: 0,
            last_read_at: None,
            marker_read_at: None,
            preserve_marker_read_at: false,
        }
    }

    pub fn is_admin(&self) -> bool {
        self.is_admin
    }

    pub fn set_is_admin(&mut self, is_admin: bool) {
        self.is_admin = is_admin;
    }

    pub fn list(&self) {
        self.service.list_task();
    }

    pub fn mine_only(&self) -> bool {
        self.mine_only
    }

    pub fn toggle_mine_only(&mut self) {
        self.mine_only = !self.mine_only;
        self.rebuild_display();
    }

    fn rebuild_display(&mut self) {
        let prev_selected_id = self
            .items
            .get(self.selected.min(self.items.len().saturating_sub(1)))
            .map(|item| item.profile.id);

        let mut next = self.source_items.clone();

        if self.mine_only {
            next.retain(|item| item.profile.user_id == self.user_id);
        }

        self.items = next;
        if let Some(prev_id) = prev_selected_id
            && let Some(idx) = self
                .items
                .iter()
                .position(|item| item.profile.id == prev_id)
        {
            self.selected = idx;
        } else {
            self.selected = clamp_index(self.selected, self.items.len());
        }
    }

    pub fn refresh_unread_count(&self) {
        self.service.refresh_unread_count_task(self.user_id);
    }

    pub fn mark_read(&mut self) {
        self.marker_read_at = self.last_read_at;
        self.preserve_marker_read_at = true;
        self.unread_count = 0;
        self.service.mark_read_task(self.user_id);
    }

    pub fn all_items(&self) -> &[WorkFeedItem] {
        &self.items
    }

    pub fn unread_count(&self) -> i64 {
        self.unread_count
    }

    pub fn marker_read_at(&self) -> Option<DateTime<Utc>> {
        self.marker_read_at
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.items.len())
    }

    pub fn selected_item(&self) -> Option<&WorkFeedItem> {
        self.items.get(self.selected_index())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.items.len());
    }

    pub fn select_index(&mut self, index: usize) {
        self.selected = clamp_index(index, self.items.len());
    }

    pub fn selected_can_edit(&self) -> bool {
        self.selected_item()
            .map(|item| self.is_admin || item.profile.user_id == self.user_id)
            .unwrap_or(false)
    }

    /// A card by id, from the full feed (the mine-only filter does not hide it).
    pub fn card(&self, id: Uuid) -> Option<&WorkFeedItem> {
        self.source_items.iter().find(|item| item.profile.id == id)
    }

    /// `user`'s card, if they posted one.
    pub fn card_of(&self, user: Uuid) -> Option<&WorkFeedItem> {
        self.source_items
            .iter()
            .find(|item| item.profile.user_id == user)
    }

    pub fn delete_selected(&mut self) -> Option<Banner> {
        let id = self.selected_item()?.profile.id;
        self.delete_card(id)
    }

    /// Delete a card: yours, or anyone's for a moderator.
    pub fn delete_card(&mut self, id: Uuid) -> Option<Banner> {
        let item = self.card(id)?;
        if !(self.is_admin || item.profile.user_id == self.user_id) {
            return Some(Banner::error("not your work card"));
        }
        self.service.delete_task(self.user_id, id, self.is_admin);
        None
    }

    /// Write a card the editor validated: a new one, or `editing` by id.
    /// The service's event raises the banner.
    pub fn save(&mut self, params: WorkProfileParams, editing: Option<Uuid>) {
        match editing {
            Some(id) => self
                .service
                .update_task(self.user_id, id, params, self.is_admin),
            None => self.service.create_task(self.user_id, params),
        }
        self.submitted = true;
    }

    pub fn copy_selected_profile_url(&self, base_url: &str) -> Option<String> {
        let item = self.selected_item()?;
        Some(profile_url(base_url, &item.profile.slug))
    }

    pub fn tick(&mut self) -> WorkTick {
        // Peek before draining: anything queued may change the rendered tab
        // (badge counts, work profile list), so it counts as changed.
        let changed = self.snapshot_rx.has_changed().unwrap_or(false) || !self.event_rx.is_empty();
        self.drain_snapshot();
        WorkTick {
            banner: self.drain_events(),
            changed,
        }
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            let snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.source_items = snapshot.items;
            self.rebuild_display();
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    WorkEvent::Created { user_id } if self.user_id == user_id && self.submitted => {
                        self.submitted = false;
                        banner = Some(Banner::success("Work card posted."));
                    }
                    WorkEvent::Updated { user_id } if self.user_id == user_id && self.submitted => {
                        self.submitted = false;
                        banner = Some(Banner::success("Work card saved."));
                    }
                    WorkEvent::Deleted { user_id } if self.user_id == user_id => {
                        banner = Some(Banner::success("Work card deleted."));
                    }
                    WorkEvent::Failed { user_id, error } if self.user_id == user_id => {
                        self.submitted = false;
                        banner = Some(Banner::error(&format!("Failed: {error}")));
                    }
                    WorkEvent::UnreadCountUpdated {
                        user_id,
                        unread_count,
                        last_read_at,
                    } if self.user_id == user_id => {
                        self.unread_count = unread_count;
                        self.last_read_at = last_read_at;
                        if unread_count == 0 && !self.preserve_marker_read_at {
                            self.marker_read_at = last_read_at;
                        }
                    }
                    _ => {}
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                // Skipped events are gone; the receiver resumes at the oldest
                // one still buffered, so keep draining.
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "work event receiver lagged");
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        banner
    }
}

pub(crate) fn profile_url(base_url: &str, slug: &str) -> String {
    format!("{}/profiles/{slug}", base_url.trim_end_matches('/'))
}

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn move_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, len as isize - 1) as usize
}
