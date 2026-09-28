//! One session's hold on the street: whether its runner is on it, what it
//! last told the replica's street task, and its tick copy of everyone
//! else.
//!
//! The first descent puts the runner on the street and it stays there,
//! lit while the session looks at the page and dim while it is on another,
//! until the session ends (`Drop`) or the runner stops being one
//! (`leave`, from the runner-directory edge in `tick.rs`). Telling the
//! task is a send on an unbounded queue, never an await, and only when
//! something changed, so `sync` is cheap to call from every tick.

use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::svc::{SharedStreetView, StreetEvent, StreetService};

pub struct StreetSession {
    session_id: Uuid,
    user_id: Uuid,
    events_tx: mpsc::UnboundedSender<StreetEvent>,
    view_rx: watch::Receiver<SharedStreetView>,
    /// The tick copy the renderer reads.
    pub view: SharedStreetView,
    on_street: bool,
    /// The last `(x, y, present)` sent, so `sync` sends only a change.
    sent: Option<(u16, u16, bool)>,
}

impl StreetSession {
    pub fn new(service: &StreetService, user_id: Uuid) -> Self {
        let view_rx = service.view_rx();
        let view = view_rx.borrow().clone();
        Self {
            session_id: Uuid::now_v7(),
            user_id,
            events_tx: service.events_tx(),
            view_rx,
            view,
            on_street: false,
            sent: None,
        }
    }

    /// The runner goes down: on the street from here until the session
    /// ends. The next `sync` puts it there.
    pub fn descend(&mut self) {
        self.on_street = true;
    }

    /// Tell the street where the runner stands and whether the session is
    /// looking, if that changed. Nothing before the first descent.
    pub fn sync(&mut self, x: u16, y: u16, present: bool) {
        if !self.on_street || self.sent == Some((x, y, present)) {
            return;
        }
        self.sent = Some((x, y, present));
        self.send(StreetEvent::Stand {
            session_id: self.session_id,
            user_id: self.user_id,
            x,
            y,
            present,
        });
    }

    /// Off the street: the runner stopped being one. A later descent (a
    /// runner again) puts it back.
    pub fn leave(&mut self) {
        if !self.on_street {
            return;
        }
        self.on_street = false;
        if self.sent.take().is_some() {
            self.send(StreetEvent::Leave {
                session_id: self.session_id,
            });
        }
    }

    /// Copy the shared view if it moved. Returns whether it did.
    pub fn refresh(&mut self) -> bool {
        if !self.view_rx.has_changed().unwrap_or(false) {
            return false;
        }
        let view = self.view_rx.borrow_and_update().clone();
        let moved = !Arc::ptr_eq(&view, &self.view);
        self.view = view;
        moved
    }

    fn send(&self, event: StreetEvent) {
        // A closed queue is a detached street (tests, headless paths); in
        // production the task lives as long as the process.
        let _ = self.events_tx.send(event);
    }
}

impl Drop for StreetSession {
    fn drop(&mut self) {
        self.leave();
    }
}
