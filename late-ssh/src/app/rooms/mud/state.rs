// Per-session client wrapper for a Lateania world.
//
// One State per session. Holds a cached snapshot drained from the service's
// watch channel each tick, plus local-only UI state (log scroll). All real
// actions delegate to the service's *_task methods; this struct never blocks
// and never mutates world truth.

use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{MudService, MudSnapshot, PlayerView, empty_player_view};
use super::world::Dir;

pub struct State {
    user_id: Uuid,
    snapshot: MudSnapshot,
    svc: MudService,
    snapshot_rx: watch::Receiver<MudSnapshot>,
}

impl State {
    pub fn new(svc: MudService, user_id: Uuid) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let state = Self {
            user_id,
            snapshot,
            svc,
            snapshot_rx,
        };
        // Auto-join the world on entry; the slice has no separate "sit" step.
        state.svc.join_task(user_id);
        state
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn is_self(&self, user_id: Uuid) -> bool {
        self.user_id == user_id
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
    }

    pub fn touch_activity(&self) {
        self.svc.touch_activity_task(self.user_id);
    }

    /// This player's view, or an empty placeholder until the join lands.
    pub fn view(&self) -> PlayerView {
        self.snapshot
            .players
            .get(&self.user_id)
            .cloned()
            .unwrap_or_else(|| empty_player_view(self.snapshot.room_id))
    }

    pub fn player_count(&self) -> usize {
        self.snapshot.players.values().filter(|p| p.joined).count()
    }

    // ---- Actions (delegate to the service) ------------------------------

    pub fn go(&self, dir: Dir) {
        self.svc.move_task(self.user_id, dir);
    }

    pub fn look(&self) {
        self.svc.look_task(self.user_id);
    }

    pub fn attack(&self) {
        self.svc.attack_task(self.user_id);
    }

    pub fn flee(&self) {
        self.svc.flee_task(self.user_id);
    }

    pub fn leave_world(&self) {
        self.svc.leave_task(self.user_id);
    }
}
