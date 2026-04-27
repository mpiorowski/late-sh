use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::primitives::Banner;
use late_core::models::notification::NotificationView;

use super::svc::{NotificationEvent, NotificationService, NotificationSnapshot};

pub struct State {
    service: NotificationService,
    user_id: Uuid,
    items: Vec<NotificationView>,
    selected: usize,
    snapshot_rx: watch::Receiver<NotificationSnapshot>,
    event_rx: broadcast::Receiver<NotificationEvent>,
    unread_count: i64,
    last_read_at: Option<DateTime<Utc>>,
    marker_read_at: Option<DateTime<Utc>>,
}

impl State {
    pub fn new(service: NotificationService, user_id: Uuid) -> Self {
        let snapshot_rx = service.subscribe_snapshot();
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
            last_read_at: None,
            marker_read_at: None,
        }
    }

    pub fn all_items(&self) -> &[NotificationView] {
        &self.items
    }

    pub fn list(&self) {
        self.service.list_task(self.user_id);
        self.service.refresh_unread_count_task(self.user_id);
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

    pub fn marker_read_at(&self) -> Option<DateTime<Utc>> {
        self.marker_read_at
    }

    pub fn mark_read(&mut self) {
        self.marker_read_at = self.last_read_at;
        self.unread_count = 0;
        self.service.mark_all_read_task(self.user_id);
    }

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_snapshot();
        self.drain_events()
    }

    fn drain_snapshot(&mut self) {
        if let Ok(true) = self.snapshot_rx.has_changed() {
            let snapshot = self.snapshot_rx.borrow_and_update().clone();
            if snapshot.user_id == Some(self.user_id) {
                self.items = snapshot.items;
                self.selected = clamp_index(self.selected, self.items.len());
            }
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
                        last_read_at,
                    } if user_id == self.user_id => {
                        self.unread_count = unread_count;
                        self.last_read_at = last_read_at;
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

fn clamp_index(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { index.min(len - 1) }
}

fn move_index(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    (current as isize + delta).clamp(0, len as isize - 1) as usize
}
