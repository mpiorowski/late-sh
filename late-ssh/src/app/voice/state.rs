use super::svc::{VoiceService, VoiceSnapshot};
use tokio::sync::watch;
use uuid::Uuid;

pub struct VoiceState {
    rx: watch::Receiver<VoiceSnapshot>,
    snapshot: VoiceSnapshot,
}

impl VoiceState {
    pub fn new(service: VoiceService) -> Self {
        let rx = service.subscribe();
        let snapshot = rx.borrow().clone();
        Self { rx, snapshot }
    }

    pub fn tick(&mut self) {
        while self.rx.has_changed().unwrap_or(false) {
            self.snapshot = self.rx.borrow_and_update().clone();
        }
    }

    pub fn snapshot(&self) -> &VoiceSnapshot {
        &self.snapshot
    }

    pub fn is_joined(&self, user_id: Uuid) -> bool {
        self.snapshot.participant(user_id).is_some()
    }

    pub fn muted(&self, user_id: Uuid) -> bool {
        self.snapshot
            .participant(user_id)
            .is_some_and(|participant| participant.muted)
    }

    pub fn deafened(&self, user_id: Uuid) -> bool {
        self.snapshot
            .participant(user_id)
            .is_some_and(|participant| participant.deafened)
    }
}
