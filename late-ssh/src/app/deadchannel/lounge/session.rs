//! A session's side of the lounge: its copy of the spawn, the word its
//! own `/hit` came back with, and the one hit that may be out. Decides
//! nothing: the composer's line is read off the copy, and the service's
//! answer sets the word.

use late_core::models::deadchannel_spawn::{Spawn, SpawnOutcome};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::data;
use super::svc::{HitOutcome, LiveSpawn, LoungeService};

/// How long the session's own word stands in for the spawn's line.
const WORD_MS: i64 = 8_000;

/// What one tick of the session found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LoungeTick {
    /// The copy or the word moved: a frame is owed where it is drawn.
    pub changed: bool,
    /// A spawn this session hit went down: the sheet mirror should re-read
    /// so the strip shows the leavings.
    pub reload_sheet: bool,
}

pub(crate) struct LoungeSession {
    /// The tick copy of the process-shared spawn.
    pub spawn: LiveSpawn,
    /// The session's own last word and when it expires, unix ms.
    pub word: Option<(String, i64)>,
    hit_in_flight: bool,
    /// The slain spawn the mirror was last re-read for, so one kill
    /// reloads once.
    reloaded_for: Option<Uuid>,
    user_id: Uuid,
    username: String,
    svc: LoungeService,
    spawn_rx: watch::Receiver<LiveSpawn>,
    outcome_tx: mpsc::UnboundedSender<HitOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<HitOutcome>,
}

impl LoungeSession {
    pub(crate) fn new(user_id: Uuid, username: String, svc: LoungeService) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        let spawn_rx = svc.subscribe();
        let spawn = spawn_rx.borrow().clone();
        Self {
            spawn,
            word: None,
            hit_in_flight: false,
            reloaded_for: None,
            user_id,
            username: username.clone(),
            svc,
            spawn_rx,
            outcome_tx,
            outcome_rx,
        }
    }

    /// `/hit`: one at a time. A second press while one is out is the
    /// word `the static is deciding`.
    pub(crate) fn hit(&mut self, now_ms: i64) {
        if self.hit_in_flight {
            self.word = Some((data::DECIDING_LINE.to_string(), now_ms + WORD_MS));
            return;
        }
        self.hit_in_flight = true;
        self.svc
            .hit_task(self.user_id, self.username.clone(), self.outcome_tx.clone());
    }

    /// Copy the shared spawn if it moved, drain the hit's answer, let the
    /// word expire.
    pub(crate) fn tick(&mut self, now_ms: i64) -> LoungeTick {
        let mut tick = LoungeTick::default();
        if self.spawn_rx.has_changed().unwrap_or(false) {
            self.spawn = self.spawn_rx.borrow_and_update().clone();
            tick.changed = true;
        }
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            self.hit_in_flight = false;
            tick.changed = true;
            let word = match outcome {
                HitOutcome::Landed { kind, left, weapon } => data::hit_line(weapon, kind, left),
                HitOutcome::AlreadyHit => data::ALREADY_HIT_LINE.to_string(),
                HitOutcome::Nothing | HitOutcome::NoRunner => data::NOTHING_TO_HIT_LINE.to_string(),
                HitOutcome::Down => data::SIGNAL_DOWN_LINE.to_string(),
                HitOutcome::Failed => data::NOT_ANSWERING_LINE.to_string(),
            };
            self.word = Some((word, now_ms + WORD_MS));
        }
        if self
            .word
            .as_ref()
            .is_some_and(|(_, until_ms)| now_ms >= *until_ms)
        {
            self.word = None;
            tick.changed = true;
        }
        if let Some(spawn) = self.spawn.as_ref()
            && spawn.outcome == Some(SpawnOutcome::Slain)
            && spawn.hit_by(self.user_id)
            && self.reloaded_for != Some(spawn.id)
        {
            self.reloaded_for = Some(spawn.id);
            tick.reload_sheet = true;
        }
        tick
    }

    /// The spawn the composer should show at `now_ms`, if any.
    pub(crate) fn showing(&self, now_ms: i64) -> Option<&Spawn> {
        self.spawn
            .as_ref()
            .filter(|spawn| super::state::shows(spawn, now_ms))
    }

    /// The session's own word while it stands.
    pub(crate) fn word(&self) -> Option<&str> {
        self.word.as_ref().map(|(word, _)| word.as_str())
    }
}
