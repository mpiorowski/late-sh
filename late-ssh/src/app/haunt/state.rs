//! First contact, stage 3: the held door (GAME.md, First contact).
//!
//! The whisper is delivered on the splash screen, once per person ever:
//! this one time the splash does not skip. Input is always acknowledged
//! (static surges, the skip hint dissolves) but control is withheld until
//! the voiced line has landed; a hard time cap then opens the door no
//! matter what. Pure state machine: no I/O, no clock reads. `App` owns
//! arming, the kill switch, and persisting the once-ever mark.
//!
//! Currently admin-scoped scaffolding: only admin sessions ever arm it,
//! so the nonrenewable first contact is never burned on real users while
//! the breach is far away.

use uuid::Uuid;

/// Ticks are `App::splash_ticks`: one per world tick while the splash is
/// up, 66ms each at the splash's hot cadence.
///
/// The base splash line finishes typing around tick 27; if nobody has
/// pressed anything by here, the whisper starts on its own.
const ANSWER_TICK: usize = 48;
/// How long the fully typed line holds before the door opens.
const LINGER_TICKS: usize = 24;
/// The hard cap, from splash start: the door opens whatever the phase.
/// A normal splash runs 90 ticks; the longest natural whisper releases
/// around tick 122, so the cap is a backstop, not a beat.
const HARD_CAP_TICKS: usize = 150;
/// How long one static surge decays after a keypress.
const SURGE_TICKS: usize = 8;
/// How long the skip hint takes to dissolve after the first keypress.
const DISSOLVE_TICKS: usize = 12;

/// The voiced lines. Screenshot-test vocabulary only (static, signal,
/// city, channel, door); a repeated whisper is a bug report, not a
/// haunting, so this pool grows under the same variety discipline as feed
/// templates (GAME.md, Open questions) before it ever leaves admin scope.
const WHISPER_LINES: [&str; 6] = [
    "you were not supposed to notice this yet",
    "the sky down here is the color of a dead channel",
    "the rain in this city falls as static",
    "something behind the screen just learned your name",
    "the door is held. not yet.",
    "there is a city under this room. it noticed you",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WhisperPhase {
    /// The door is held; the line has not started.
    Held,
    /// The line is typing itself, one char per tick since `from_tick`.
    Typing { from_tick: usize },
    /// The line is fully typed; it holds until `until_tick` so it lands.
    Linger { until_tick: usize },
    /// The door opened. `delivered` is false when the kill switch or the
    /// hard cap cut the scene before the line finished: the once-ever
    /// mark is only spent on a whisper that was actually read.
    Released { delivered: bool },
}

/// What one `tick` decided, for the owner to act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WhisperTick {
    Holding,
    Released { delivered: bool },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WhisperState {
    line: &'static str,
    phase: WhisperPhase,
    /// Last tick input landed; every keypress re-surges the static.
    last_input_tick: Option<usize>,
    /// First tick input landed; starts the skip-hint dissolve.
    first_input_tick: Option<usize>,
    /// Per-user seed for the deterministic corruption patterns.
    seed: u64,
}

impl WhisperState {
    pub(crate) fn for_user(user_id: Uuid) -> Self {
        Self::with_seed(user_id.as_u128() as u64)
    }

    fn with_seed(seed: u64) -> Self {
        Self {
            line: WHISPER_LINES[(seed % WHISPER_LINES.len() as u64) as usize],
            phase: WhisperPhase::Held,
            last_input_tick: None,
            first_input_tick: None,
            seed,
        }
    }

    /// Input landed while the splash is up. Acknowledged, never obeyed:
    /// the static surges, the hint starts dissolving, and if the line has
    /// not started yet it starts now, in answer.
    pub(crate) fn note_input(&mut self, tick: usize) {
        if matches!(self.phase, WhisperPhase::Released { .. }) {
            return;
        }
        self.last_input_tick = Some(tick);
        if self.first_input_tick.is_none() {
            self.first_input_tick = Some(tick);
        }
        if self.phase == WhisperPhase::Held {
            self.phase = WhisperPhase::Typing { from_tick: tick };
        }
    }

    /// Advance one splash tick. `enabled` is the live kill switch: turning
    /// it off drops the theater mid-scene without spending the mark.
    pub(crate) fn tick(&mut self, tick: usize, enabled: bool) -> WhisperTick {
        if let WhisperPhase::Released { delivered } = self.phase {
            return WhisperTick::Released { delivered };
        }
        if !enabled {
            self.phase = WhisperPhase::Released { delivered: false };
            return WhisperTick::Released { delivered: false };
        }
        if tick >= HARD_CAP_TICKS {
            let delivered = matches!(self.phase, WhisperPhase::Linger { .. });
            self.phase = WhisperPhase::Released { delivered };
            return WhisperTick::Released { delivered };
        }
        match self.phase {
            WhisperPhase::Held => {
                if tick >= ANSWER_TICK {
                    self.phase = WhisperPhase::Typing { from_tick: tick };
                }
            }
            WhisperPhase::Typing { from_tick } => {
                if tick.saturating_sub(from_tick) >= self.line.chars().count() {
                    self.phase = WhisperPhase::Linger {
                        until_tick: tick + LINGER_TICKS,
                    };
                }
            }
            WhisperPhase::Linger { until_tick } => {
                if tick >= until_tick {
                    self.phase = WhisperPhase::Released { delivered: true };
                    return WhisperTick::Released { delivered: true };
                }
            }
            WhisperPhase::Released { .. } => unreachable!("handled above"),
        }
        WhisperTick::Holding
    }

    pub(crate) fn seed(&self) -> u64 {
        self.seed
    }

    pub(crate) fn line(&self) -> &'static str {
        self.line
    }

    /// How many chars of the line are visible at `tick`, and whether it is
    /// still typing (drives the cursor).
    pub(crate) fn typed_chars(&self, tick: usize) -> (usize, bool) {
        let len = self.line.chars().count();
        match self.phase {
            WhisperPhase::Held => (0, false),
            WhisperPhase::Typing { from_tick } => {
                let typed = tick.saturating_sub(from_tick).min(len);
                (typed, typed < len)
            }
            WhisperPhase::Linger { .. } | WhisperPhase::Released { .. } => (len, false),
        }
    }

    /// 0.0 (fresh burst) to 1.0 (faded) while a static surge is live.
    pub(crate) fn surge_progress(&self, tick: usize) -> Option<f32> {
        let since = tick.saturating_sub(self.last_input_tick?);
        (since < SURGE_TICKS).then(|| since as f32 / SURGE_TICKS as f32)
    }

    /// 0.0 to 1.0 skip-hint dissolution once input has landed; `None`
    /// while the hint is still intact.
    pub(crate) fn dissolve_progress(&self, tick: usize) -> Option<f32> {
        let since = tick.saturating_sub(self.first_input_tick?);
        Some((since as f32 / DISSOLVE_TICKS as f32).min(1.0))
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
