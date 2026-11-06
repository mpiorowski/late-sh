use std::collections::HashSet;

use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;
use late_core::models::notification::NotificationView;

use super::svc::{NotificationEvent, NotificationService, NotificationSnapshot};

/// Outcome of one tab tick: the banner to surface plus whether a drained
/// snapshot or event may have changed the rendered tab (badge counts, mention list).
pub struct NotificationsTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

pub struct State {
    service: NotificationService,
    user_id: Uuid,
    items: Vec<NotificationView>,
    selected: usize,
    snapshot_rx: watch::Receiver<NotificationSnapshot>,
    event_rx: broadcast::Receiver<NotificationEvent>,
    unread_count: i64,
    /// What the last visit to the Mentions entry stamped read: their dots stay
    /// up for that visit, and the next visit replaces the set.
    read_on_open: HashSet<Uuid>,
}

impl State {
    pub fn new(service: NotificationService, user_id: Uuid) -> Self {
        let snapshot_rx = service.subscribe_snapshot(user_id);
        let event_rx = service.subscribe_events();
        service.refresh_unread_count_task(user_id);
        Self {
            service,
            user_id,
            items: Vec::new(),
            selected: 0,
            snapshot_rx,
            event_rx,
            unread_count: 0,
            read_on_open: HashSet::new(),
        }
    }

    pub fn all_items(&self) -> &[NotificationView] {
        &self.items
    }

    pub fn list(&self) {
        self.service.list_task(self.user_id);
    }

    pub fn refresh_unread_count(&self) {
        self.service.refresh_unread_count_task(self.user_id);
    }

    /// The given messages, which mention this user, were rendered in their own
    /// room; clear their mentions and let the service republish the badge.
    pub fn mark_read_for_messages(&self, message_ids: Vec<Uuid>) {
        self.service
            .mark_read_for_messages_task(self.user_id, message_ids);
    }

    pub fn selected_index(&self) -> usize {
        clamp_index(self.selected, self.items.len())
    }

    pub fn move_selection(&mut self, delta: isize) {
        self.selected = move_index(self.selected_index(), delta, self.items.len());
    }

    pub fn selected_item(&self) -> Option<&NotificationView> {
        self.items.get(self.selected_index())
    }

    pub fn unread_count(&self) -> i64 {
        self.unread_count
    }

    pub fn read_on_open(&self) -> &HashSet<Uuid> {
        &self.read_on_open
    }

    /// The Mentions entry was opened: stamps its unread mentions and loads the
    /// list. The badge moves when the stamp has committed, not before: a count
    /// moved here would have the attention guest list the rows still unread.
    pub fn open_feed(&self) {
        self.service.open_feed_task(self.user_id);
    }

    pub fn tick(&mut self) -> NotificationsTick {
        // Peek before draining: anything queued may change the rendered tab
        // (badge counts, mention list), so it counts as changed.
        let changed = self.snapshot_rx.has_changed().unwrap_or(false) || !self.event_rx.is_empty();
        self.drain_snapshot();
        NotificationsTick {
            banner: self.drain_events(),
            changed,
        }
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            self.items = self.snapshot_rx.borrow_and_update().items.clone();
            self.selected = clamp_index(self.selected, self.items.len());
        }
    }

    fn drain_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => match event {
                    NotificationEvent::UnreadCountUpdated {
                        user_id,
                        unread_count,
                    } if user_id == self.user_id => {
                        self.unread_count = unread_count;
                    }
                    NotificationEvent::FeedOpened { user_id, read } if user_id == self.user_id => {
                        self.read_on_open = read.into_iter().collect();
                    }
                    NotificationEvent::NewMention {
                        user_id,
                        unread_count,
                    } if user_id == self.user_id => {
                        let increased = unread_count > self.unread_count;
                        self.unread_count = unread_count;
                        if increased {
                            let noun = if unread_count == 1 {
                                "mention"
                            } else {
                                "mentions"
                            };
                            banner =
                                Some(Banner::success(&format!("{unread_count} unread {noun}")));
                        }
                    }
                    _ => (),
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive notification event");
                    break;
                }
            }
        }
        banner
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.service.prune_snapshot_channel(self.user_id);
    }
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
