use asterion_core::GameCommand;
use ratatui::text::Line;
use tokio::sync::watch;
use uuid::Uuid;

use super::render::img_to_lines;
use super::svc::{AsterionPrivateSnapshot, AsterionPublicSnapshot, AsterionService};

pub struct State {
    user_id: Uuid,
    public: AsterionPublicSnapshot,
    private: AsterionPrivateSnapshot,
    cached_lines: Vec<Line<'static>>,
    svc: AsterionService,
    public_rx: watch::Receiver<AsterionPublicSnapshot>,
    private_rx: watch::Receiver<AsterionPrivateSnapshot>,
}

impl State {
    pub fn new(svc: AsterionService, user_id: Uuid) -> Self {
        let public_rx = svc.subscribe_public();
        let private_rx = svc.subscribe_private(user_id);
        let public = public_rx.borrow().clone();
        let private = private_rx.borrow().clone();
        svc.join_task(user_id);
        Self {
            user_id,
            public,
            private,
            cached_lines: Vec::new(),
            svc,
            public_rx,
            private_rx,
        }
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn tick(&mut self) {
        if self.public_rx.has_changed().unwrap_or(false) {
            self.public = self.public_rx.borrow_and_update().clone();
        }
        if self.private_rx.has_changed().unwrap_or(false) {
            self.private = self.private_rx.borrow_and_update().clone();
            self.cached_lines = match &self.private.view {
                Some(view) => img_to_lines(&view.image, &view.overrides, view.background),
                None => Vec::new(),
            };
        }
    }

    pub fn lines(&self) -> &[Line<'static>] {
        &self.cached_lines
    }

    pub fn public(&self) -> &AsterionPublicSnapshot {
        &self.public
    }

    pub fn private(&self) -> &AsterionPrivateSnapshot {
        &self.private
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
