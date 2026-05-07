use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{PokerPrivateSnapshot, PokerPublicSnapshot, PokerService};

pub struct State {
    user_id: Uuid,
    public_snapshot: PokerPublicSnapshot,
    private_snapshot: PokerPrivateSnapshot,
    svc: PokerService,
    public_rx: watch::Receiver<PokerPublicSnapshot>,
    private_rx: watch::Receiver<PokerPrivateSnapshot>,
}

impl State {
    pub fn new(svc: PokerService, user_id: Uuid) -> Self {
        let public_rx = svc.subscribe_public();
        let private_rx = svc.subscribe_private(user_id);
        let public_snapshot = public_rx.borrow().clone();
        let private_snapshot = private_rx.borrow().clone();
        Self {
            user_id,
            public_snapshot,
            private_snapshot,
            svc,
            public_rx,
            private_rx,
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn tick(&mut self) {
        if self.public_rx.has_changed().unwrap_or(false) {
            self.public_snapshot = self.public_rx.borrow_and_update().clone();
        }
        if self.private_rx.has_changed().unwrap_or(false) {
            self.private_snapshot = self.private_rx.borrow_and_update().clone();
        }
    }

    pub fn public_snapshot(&self) -> &PokerPublicSnapshot {
        &self.public_snapshot
    }

    pub fn private_snapshot(&self) -> &PokerPrivateSnapshot {
        &self.private_snapshot
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn seat_index(&self) -> Option<usize> {
        self.public_snapshot
            .seats
            .iter()
            .position(|seat| seat.user_id == Some(self.user_id))
    }

    pub fn is_seated(&self) -> bool {
        self.seat_index().is_some()
    }

    pub fn can_act(&self) -> bool {
        self.seat_index() == self.public_snapshot.active_seat
    }

    pub fn sit(&self) {
        self.svc.sit_task(self.user_id);
    }

    pub fn leave_seat(&self) {
        self.svc.leave_seat_task(self.user_id);
    }

    pub fn start_hand(&self) {
        self.svc.start_hand_task(self.user_id);
    }

    pub fn check(&self) {
        self.svc.check_task(self.user_id);
    }

    pub fn fold(&self) {
        self.svc.fold_task(self.user_id);
    }

    pub fn touch_activity(&self) {
        if self.is_seated() {
            self.svc.touch_activity_task(self.user_id);
        }
    }
}
