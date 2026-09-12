use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use late_core::models::pet::{
    LifeStage, PetCompanion, PetMood, PetSpecies, pet_age_anchor, pet_age_label,
};
use ratatui::layout::Rect;
use uuid::Uuid;

use super::svc::PetService;

/// How long each event mood lasts. A win outlasts a loss on purpose: the
/// pet is an optimist.
pub const PURR_FOR: Duration = Duration::from_secs(2 * 60);
pub const PROUD_FOR: Duration = Duration::from_secs(30 * 60);
pub const SULK_FOR: Duration = Duration::from_secs(10 * 60);
pub const CHATTY_FOR: Duration = Duration::from_secs(10 * 60);
/// No key from the owner for this long and the pet dozes off: well before
/// the account reads as idle to anyone else.
pub const ASLEEP_AFTER: Duration = Duration::from_secs(10 * 60);

/// Body width including the tail column, used to keep the pet inside its box.
pub const PET_WIDTH: usize = 8;
pub const PET_HEIGHT: usize = 3;

/// What the session has told the pet, as the moment each thing last
/// happened. Nothing here is persisted: a fresh session starts blank, so
/// the first tick reads idle (or asleep, for a session nobody touches).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoodSignals {
    /// A click on the pet.
    pub petted: Option<Instant>,
    /// Any win: a game, a daily match, a boss.
    pub won: Option<Instant>,
    /// Any loss: a death in a door game, a lost daily match.
    pub lost: Option<Instant>,
    /// A chat message of the owner's landed, in any room or DM.
    pub spoke: Option<Instant>,
}

/// The session facts the tick hands over every time: they are read, not
/// pushed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ambient {
    /// The owner's last keystroke or click.
    pub last_input: Instant,
    /// A paired client is playing, unmuted.
    pub music_playing: bool,
}

/// The one reading: precedence top to bottom (`PetMood::ALL`). Event moods
/// hold for their window; sleep beats the music, since a pet dozing beside
/// a radio is the honest picture of an owner who walked off.
pub fn mood_at(signals: MoodSignals, ambient: Ambient, now: Instant) -> PetMood {
    let within = |at: Option<Instant>, window: Duration| {
        at.is_some_and(|at| now.saturating_duration_since(at) < window)
    };
    if within(signals.petted, PURR_FOR) {
        PetMood::Purring
    } else if within(signals.won, PROUD_FOR) {
        PetMood::Proud
    } else if within(signals.lost, SULK_FOR) {
        PetMood::Sulking
    } else if within(signals.spoke, CHATTY_FOR) {
        PetMood::Chatty
    } else if now.saturating_duration_since(ambient.last_input) >= ASLEEP_AFTER {
        PetMood::Asleep
    } else if ambient.music_playing {
        PetMood::Vibing
    } else {
        PetMood::Idle
    }
}

/// Where the pet's eyes point while it walks after the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Left,
    Ahead,
    Right,
}

/// The pet off the stroll formula: walking after the cursor, or sitting
/// under it. Cell offsets inside the roam zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Perch {
    pub x: usize,
    pub y: usize,
    pub look: Look,
}

/// What the last draw of the box used, recorded for the tick: how far the
/// pet can travel, the zone it travels in (so the cursor can be placed in
/// it), what it has to watch beside it, and where it stood.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetFrameInputs {
    pub travel: PetTravel,
    pub zone: Rect,
    pub neighbours: super::ui::Neighbours,
    pub position: (usize, usize),
}

/// How far the pet can travel inside its box, in cells, on each axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PetTravel {
    pub x: usize,
    pub y: usize,
}

/// Everything one tick reads.
#[derive(Clone, Copy, Debug)]
pub struct PetTick {
    /// The app's shared 66ms wall clock (marquee_tick): the adaptive loop
    /// ticks sparsely, so a per-call counter would slow the animation with
    /// the cadence; syncing to the wall clock keeps every speed true.
    pub wall_tick: usize,
    pub now: Instant,
    pub ambient: Ambient,
    /// The box drawn last frame, `None` when no box was drawn.
    pub frame: Option<PetFrameInputs>,
    /// The terminal cursor, 0-based cells, when the terminal reported it.
    pub cursor: Option<(u16, u16)>,
    /// Whether a mood change is written to the row (pet owners only; the
    /// state machine runs for everyone, cheaply).
    pub persist: bool,
}

pub struct PetState {
    pub user_id: Uuid,
    pub svc: PetService,

    /// User-set pet name. `None` until set via the `/petname` chat command.
    pub name: Option<String>,
    pub species: PetSpecies,
    /// When the row was first created. Used as a fallback age anchor.
    pub created: DateTime<Utc>,
    /// When the user unlocked the companion. Drives the life-stage buckets
    /// for purchased pets.
    pub adopted_at: Option<DateTime<Utc>>,

    pub signals: MoodSignals,
    mood: PetMood,
    perch: Option<Perch>,
    animation_ticks: usize,
}

impl PetState {
    pub fn new(user_id: Uuid, svc: PetService, companion: PetCompanion) -> Self {
        let species = companion.species();
        let mood = companion.mood();
        Self {
            user_id,
            svc,
            name: companion.name,
            species,
            created: companion.created,
            adopted_at: companion.adopted_at,
            signals: MoodSignals::default(),
            // The mood the row holds (`asleep` after the last session left):
            // the first tick reads the real thing and, since it differs,
            // writes it, so the profile stops saying asleep the moment the
            // owner is back.
            mood,
            perch: None,
            animation_ticks: 0,
        }
    }

    /// Current life stage based on how long the pet has existed.
    pub fn life_stage(&self) -> LifeStage {
        LifeStage::from_age_days(
            (Utc::now() - pet_age_anchor(self.created, self.adopted_at))
                .num_days()
                .max(0),
        )
    }

    /// Human-readable age string for display, e.g. "3 days" or "1 year".
    pub fn age_label(&self) -> String {
        pet_age_label(pet_age_anchor(self.created, self.adopted_at), Utc::now())
    }

    /// Set (or clear with `None`) the user-set pet name and persist it.
    pub fn set_name(&mut self, name: Option<String>) {
        self.name = name.clone();
        self.svc.set_name_task(self.user_id, name);
    }

    /// Set the pet species and persist it.
    pub fn set_species(&mut self, species: PetSpecies) {
        self.species = species;
        self.svc.set_species_task(self.user_id, species);
    }

    pub fn mood(&self) -> PetMood {
        self.mood
    }

    pub fn perch(&self) -> Option<Perch> {
        self.perch
    }

    pub fn animation_ticks(&self) -> usize {
        self.animation_ticks
    }

    pub fn note_petted(&mut self, now: Instant) {
        self.signals.petted = Some(now);
    }

    pub fn note_win(&mut self, now: Instant) {
        self.signals.won = Some(now);
    }

    pub fn note_loss(&mut self, now: Instant) {
        self.signals.lost = Some(now);
    }

    pub fn note_spoke(&mut self, now: Instant) {
        self.signals.spoke = Some(now);
    }

    /// Advance the pet: read the mood, walk after the cursor. Returns true
    /// on state edges that need a frame even when the animation predicate
    /// is quiet: a mood change, a step of the walk. Pure animation cadence
    /// is the box's business (`ui::frame_changed`).
    pub fn tick(&mut self, input: PetTick) -> bool {
        let mut changed = false;
        let elapsed = input.wall_tick.saturating_sub(self.animation_ticks);
        self.animation_ticks = input.wall_tick;

        let mood = mood_at(self.signals, input.ambient, input.now);
        if mood != self.mood {
            self.mood = mood;
            changed = true;
            if input.persist {
                self.svc.set_mood_task(self.user_id, mood);
            }
        }

        let target = match (mood_follows(mood), input.frame, input.cursor) {
            (true, Some(frame), Some(cursor)) => cursor_target(frame, cursor),
            (true, _, _) | (false, _, _) => None,
        };
        // Walking after the cursor: one cell per animation edge on each
        // axis, from wherever the pet stood. The moment the cursor leaves
        // the box the stroll takes over again, purring or not: a pet that
        // holds still where it was petted reads as stuck.
        let perch = match (target, self.perch, input.frame) {
            (Some(target), from, Some(frame)) => {
                let (x, y) = from.map_or(frame.position, |perch| (perch.x, perch.y));
                // The stroll paints on every second wall tick; the walk
                // keeps that pace whatever the loop's cadence.
                let cells = elapsed.div_ceil(2);
                Some(Perch {
                    x: step_toward(x, target.x, cells),
                    y: step_toward(y, target.y, cells),
                    look: target.look,
                })
            }
            (Some(_), _, None) | (None, _, _) => None,
        };
        if perch != self.perch {
            self.perch = perch;
            changed = true;
        }
        changed
    }
}

/// A sulking or sleeping pet does not come when called.
fn mood_follows(mood: PetMood) -> bool {
    match mood {
        PetMood::Purring | PetMood::Proud | PetMood::Chatty | PetMood::Vibing | PetMood::Idle => {
            true
        }
        PetMood::Sulking | PetMood::Asleep => false,
    }
}

/// Where the pet should stand to sit under the cursor, when the cursor is
/// inside the box the pet was last drawn in; the face lands on the cursor
/// column and the eyes point at it on the way.
fn cursor_target(frame: PetFrameInputs, cursor: (u16, u16)) -> Option<Perch> {
    let zone = frame.zone;
    let (cx, cy) = cursor;
    let inside = cx >= zone.x && cx < zone.right() && cy >= zone.y && cy < zone.bottom();
    if !inside {
        return None;
    }
    // The face is three cells in from the pet's left edge.
    let x = (usize::from(cx - zone.x))
        .saturating_sub(3)
        .min(frame.travel.x);
    let y = (usize::from(cy - zone.y))
        .saturating_sub(1)
        .min(frame.travel.y);
    let face = frame.position.0 + 3;
    let look = if usize::from(cx - zone.x) < face {
        Look::Left
    } else if usize::from(cx - zone.x) > face + 1 {
        Look::Right
    } else {
        Look::Ahead
    };
    Some(Perch { x, y, look })
}

fn step_toward(from: usize, to: usize, cells: usize) -> usize {
    if from < to {
        (from + cells).min(to)
    } else {
        from.saturating_sub(cells).max(to)
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
