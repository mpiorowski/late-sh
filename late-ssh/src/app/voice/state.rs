use super::svc::{VoiceParticipant, VoiceService, VoiceSnapshot};
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

    /// Returns true when the snapshot was replaced this tick.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        while self.rx.has_changed().unwrap_or(false) {
            self.snapshot = self.rx.borrow_and_update().clone();
            changed = true;
        }
        changed
    }

    pub fn snapshot(&self) -> &VoiceSnapshot {
        &self.snapshot
    }

    /// The room whose voice channel the user is currently in, if any.
    pub fn current_room(&self, user_id: Uuid) -> Option<Uuid> {
        self.snapshot.current_room(user_id)
    }

    pub fn is_joined(&self, user_id: Uuid) -> bool {
        self.snapshot.is_joined(user_id)
    }

    fn participant(&self, user_id: Uuid) -> Option<&VoiceParticipant> {
        let room_id = self.snapshot.current_room(user_id)?;
        self.snapshot.participant(room_id, user_id)
    }

    pub fn muted(&self, user_id: Uuid) -> bool {
        self.participant(user_id)
            .is_some_and(|participant| participant.muted)
    }

    pub fn deafened(&self, user_id: Uuid) -> bool {
        self.participant(user_id)
            .is_some_and(|participant| participant.deafened)
    }
}
