use asterion_core::GameCommand;
use tokio::sync::watch;
use uuid::Uuid;

use super::svc::{AsterionService, AsterionSnapshot};

pub struct State {
    user_id: Uuid,
    snapshot: AsterionSnapshot,
    svc: AsterionService,
    snapshot_rx: watch::Receiver<AsterionSnapshot>,
}

impl State {
    pub fn new(svc: AsterionService, user_id: Uuid, name: String) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        svc.join_task(user_id, name);
        Self {
            user_id,
            snapshot,
            svc,
            snapshot_rx,
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
    }

    pub fn snapshot(&self) -> &AsterionSnapshot {
        &self.snapshot
    }

    pub fn send_command(&self, command: GameCommand) {
        self.svc.command_task(self.user_id, command);
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.svc.leave_task(self.user_id);
    }
}
