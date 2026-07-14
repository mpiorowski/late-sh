use std::time::{Duration, Instant};

use asterion_core::GameCommand;
use ratatui::text::Line;
use tokio::sync::watch;
use uuid::Uuid;

use crate::app::lobby::house::image_render::img_to_lines;

use super::svc::{AsterionPrivateSnapshot, AsterionPublicSnapshot, AsterionService};

const FLASH_TTL: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerUpFlash {
    Speed,
    Vision,
    Memory,
}

impl PowerUpFlash {
    pub fn label(self) -> &'static str {
        match self {
            Self::Speed => "SPEED UP!",
            Self::Vision => "VISION UP!",
            Self::Memory => "MEMORY UP!",
        }
    }
}

pub struct State {
    user_id: Uuid,
    session_id: Uuid,
    public: AsterionPublicSnapshot,
    private: AsterionPrivateSnapshot,
    cached_lines: Vec<Line<'static>>,
    flash: Option<(PowerUpFlash, Instant)>,
    svc: AsterionService,
    public_rx: watch::Receiver<AsterionPublicSnapshot>,
    private_rx: watch::Receiver<AsterionPrivateSnapshot>,
}

impl State {
    pub fn new(svc: AsterionService, user_id: Uuid, session_id: Uuid) -> Self {
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
            flash: None,
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
            let next = self.private_rx.borrow_and_update().clone();
            if let Some(flash) = detect_power_up(&self.private, &next) {
                self.flash = Some((flash, Instant::now()));
            }
            self.private = next;
            self.cached_lines = match &self.private.view {
                Some(view) => img_to_lines(&view.image, Some(&view.overrides), view.background),
                None => Vec::new(),
            };
        }
        if let Some((_, at)) = self.flash
            && at.elapsed() >= FLASH_TTL
        {
            self.flash = None;
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

    pub fn power_up_flash(&self) -> Option<PowerUpFlash> {
        self.flash.map(|(flash, _)| flash)
    }

    pub fn send_command(&self, command: GameCommand) {
        self.svc.command_task(self.user_id, command);
    }

    pub fn touch_activity(&self) {
        // Asterion has no inactivity kick: heroes die to the maze, and an
        // empty maze stops itself. The rooms-era DB touch died with rooms.
    }
}

fn detect_power_up(
    prev: &AsterionPrivateSnapshot,
    next: &AsterionPrivateSnapshot,
) -> Option<PowerUpFlash> {
    if next.speed > prev.speed {
        Some(PowerUpFlash::Speed)
    } else if next.vision > prev.vision {
        Some(PowerUpFlash::Vision)
    } else if next.memory > prev.memory {
        Some(PowerUpFlash::Memory)
    } else {
        None
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.svc.leave_task(self.user_id, self.session_id);
    }
}
