use image::Rgba;
use ratatui::text::Line;
use sshattrick_core::GameCommand;
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::rooms::image_render::img_to_lines;

use super::svc::{SshattrickPrivateSnapshot, SshattrickPublicSnapshot, SshattrickService};

// sshattrick pitches use alpha-0 as the only transparency signal. Pick a
// magenta sentinel rgb that the asset palette never produces so the shared
// `is_transparent` rgb check is a no-op for us.
const BACKGROUND: Rgba<u8> = Rgba([255, 0, 255, 0]);

pub struct State {
    user_id: Uuid,
    session_id: Uuid,
    public: SshattrickPublicSnapshot,
    private: SshattrickPrivateSnapshot,
    cached_lines: Vec<Line<'static>>,
    svc: SshattrickService,
    public_rx: watch::Receiver<SshattrickPublicSnapshot>,
    private_rx: watch::Receiver<SshattrickPrivateSnapshot>,
}

impl State {
    pub fn new(svc: SshattrickService, user_id: Uuid, session_id: Uuid) -> Self {
        let public_rx = svc.subscribe_public();
        let private_rx = svc.subscribe_private(user_id);
        let public = public_rx.borrow().clone();
        let private = private_rx.borrow().clone();
        svc.join_task(user_id, session_id);
        Self {
            user_id,
            session_id,
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
                Some(view) => img_to_lines(&view.image, None, BACKGROUND),
                None => Vec::new(),
            };
        }
    }

    pub fn lines(&self) -> &[Line<'static>] {
        &self.cached_lines
    }

    pub fn public(&self) -> &SshattrickPublicSnapshot {
        &self.public
    }

    pub fn private(&self) -> &SshattrickPrivateSnapshot {
        &self.private
    }

    pub fn send_command(&self, command: GameCommand) {
        self.svc.command_task(self.user_id, command);
    }

    pub fn sit(&self) {
        self.svc.sit_task(self.user_id);
    }

    pub fn reset(&self) {
        self.svc.reset_task();
    }

    pub fn touch_activity(&self) {
        self.svc.touch_activity_task(self.user_id);
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.svc.leave_task(self.user_id, self.session_id);
    }
}
