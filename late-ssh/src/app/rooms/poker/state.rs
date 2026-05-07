use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{BIG_BLIND, PokerPhase, PokerPrivateSnapshot, PokerPublicSnapshot, PokerService};

const RAISE_STEP: i64 = BIG_BLIND;

pub struct State {
    user_id: Uuid,
    public_snapshot: PokerPublicSnapshot,
    private_snapshot: PokerPrivateSnapshot,
    svc: PokerService,
    public_rx: watch::Receiver<PokerPublicSnapshot>,
    private_rx: watch::Receiver<PokerPrivateSnapshot>,
    balance: i64,
    selected_raise: i64,
}

impl State {
    pub fn new(svc: PokerService, user_id: Uuid, balance: i64) -> Self {
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
            balance,
            selected_raise: BIG_BLIND,
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
            if let Some(balance) = self.private_snapshot.balance {
                self.balance = balance;
            }
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
        self.svc.sit_task(self.user_id, self.balance);
    }

    pub fn leave_seat(&self) {
        self.svc.leave_seat_task(self.user_id);
    }

    pub fn start_hand(&self) {
        self.svc.start_hand_task(self.user_id);
    }

    pub fn call_or_check(&self) {
        self.svc.call_or_check_task(self.user_id);
    }

    pub fn bet_or_raise(&self) {
        self.svc
            .bet_or_raise_task(self.user_id, self.selected_raise);
    }

    pub fn all_in(&self) {
        self.svc.all_in_task(self.user_id);
    }

    pub fn fold(&self) {
        self.svc.fold_task(self.user_id);
    }

    pub fn touch_activity(&self) {
        if self.is_seated() {
            self.svc.touch_activity_task(self.user_id);
        }
    }

    pub fn balance(&self) -> i64 {
        self.balance
    }

    pub fn selected_raise(&self) -> i64 {
        self.selected_raise
    }

    pub fn increase_raise(&mut self) {
        self.selected_raise = self.selected_raise.saturating_add(RAISE_STEP);
    }

    pub fn decrease_raise(&mut self) {
        self.selected_raise = (self.selected_raise - RAISE_STEP).max(BIG_BLIND);
    }

    pub fn to_call(&self) -> i64 {
        self.private_snapshot.to_call
    }

    pub fn min_raise(&self) -> i64 {
        self.private_snapshot.min_raise.max(BIG_BLIND)
    }

    pub fn can_raise(&self) -> bool {
        self.private_snapshot.can_raise
    }

    pub fn can_all_in(&self) -> bool {
        self.can_raise() || (self.to_call() > 0 && self.balance <= self.to_call())
    }

    pub fn can_sync_external_chip_balance(&self) -> bool {
        matches!(
            self.public_snapshot.phase,
            PokerPhase::Waiting | PokerPhase::Showdown
        ) && !self.public_snapshot.settlement_pending
    }

    pub fn sync_external_chip_balance(&mut self, balance: i64) {
        self.balance = balance;
        self.svc.sync_balance_task(self.user_id, balance);
    }
}
