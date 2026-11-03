//! One session's hold on presence: its session id, the record it last
//! told the replica's presence task, its tick copy of everyone's, and its
//! tick copy of the day's headcount for Home's pulse tile.
//!
//! The session builds its whole record on every tick from the rooms that
//! own the parts (`App::presence_record`) and hands it to `publish`, which
//! sends only a change: a send on an unbounded queue, never an await, so
//! it is cheap to call from every tick. `Drop` is the logout.

use std::sync::Arc;

use late_core::models::presence::PresenceRecord;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::svc::{History, PresenceEvent, PresenceService, Records};

pub struct PresenceSession {
    session_id: Uuid,
    events_tx: mpsc::UnboundedSender<PresenceEvent>,
    records_rx: watch::Receiver<Records>,
    history_rx: watch::Receiver<History>,
    /// The tick copy the rooms derive from.
    pub records: Records,
    /// The tick copy the pulse tile draws: the last day of headcount
    /// buckets, oldest first.
    pub history: History,
    sent: Option<PresenceRecord>,
}

impl PresenceSession {
    pub fn new(service: &PresenceService, session_id: Uuid) -> Self {
        let records_rx = service.records_rx();
        let records = records_rx.borrow().clone();
        let history_rx = service.history_rx();
        let history = history_rx.borrow().clone();
        Self {
            session_id,
            events_tx: service.events_tx(),
            records_rx,
            history_rx,
            records,
            history,
            sent: None,
        }
    }

    pub fn session_id(&self) -> Uuid {
        self.session_id
    }

    /// Tell presence where this session is, if that changed.
    pub fn publish(&mut self, record: PresenceRecord) {
        if self.sent.as_ref() == Some(&record) {
            return;
        }
        self.sent = Some(record.clone());
        self.send(PresenceEvent::Put(Box::new(record)));
    }

    /// Copy the shared records if they moved. Returns whether they did.
    pub fn refresh(&mut self) -> bool {
        if !self.records_rx.has_changed().unwrap_or(false) {
            return false;
        }
        let records = self.records_rx.borrow_and_update().clone();
        let moved = !Arc::ptr_eq(&records, &self.records);
        self.records = records;
        moved
    }

    /// Copy the day's headcount if a new bucket landed (once in five
    /// minutes). Returns whether it did.
    pub fn refresh_history(&mut self) -> bool {
        if !self.history_rx.has_changed().unwrap_or(false) {
            return false;
        }
        self.history = self.history_rx.borrow_and_update().clone();
        true
    }

    fn send(&self, event: PresenceEvent) {
        // A closed queue is detached presence (tests, headless paths); in
        // production the task lives as long as the process.
        let _ = self.events_tx.send(event);
    }
}

impl Drop for PresenceSession {
    fn drop(&mut self) {
        if self.sent.is_some() {
            self.send(PresenceEvent::Leave {
                session_id: self.session_id,
            });
        }
    }
}
