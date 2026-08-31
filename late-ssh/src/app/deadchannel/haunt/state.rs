//! First contact state machines (GAME.md, First contact): stage 1, the
//! clock glitch ([`ClockGlitch`]), and stage 3, the held door
//! ([`WhisperState`]).
//!
//! The whisper is delivered on the splash screen, once per person ever:
//! this one time the splash does not skip. Input is always acknowledged
//! (static surges, the skip hint dissolves) but control is withheld until
//! the voiced line has landed; a hard time cap then opens the door no
//! matter what. Pure state machines: no I/O, no clock reads. `App` owns
//! arming, the kill switch, and persisting the once-ever mark.
//!
//! Currently admin-scoped scaffolding: only admin sessions ever arm any
//! of it, so the nonrenewable first contact is never burned on real users
//! while the breach is far away.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use chrono::NaiveDate;
use uuid::Uuid;

/// The haunting's one slot on `App`: every stage's machine and the flags
/// that gate them. The root owns the field; everything that reads or
/// writes it goes through `svc.rs` (orchestration) or `ui.rs` (draw).
pub(crate) struct HauntState {
    /// Stage 3: the armed splash whisper. `Some` only while the splash is
    /// holding the door for it.
    pub(crate) whisper: Option<WhisperState>,
    /// Stage 1: the clock-glitch scheduler. Session-local dice, no
    /// persistence.
    pub(crate) clock_glitch: Option<ClockGlitch>,
    /// Whether this user has already had the once-ever whisper, mirrored
    /// from `users.settings` at session start and kept honest in-session.
    pub(crate) whisper_done: bool,
    /// Process-global kill switch, flipped by `/haunt on|off`.
    pub(crate) enabled: Arc<AtomicBool>,
}

impl HauntState {
    /// Whether the splash may not self-expire this tick: an armed whisper
    /// owns the release.
    pub(crate) fn holds_splash_door(&self) -> bool {
        self.whisper.is_some()
    }

    /// Drop an armed-but-unplayed whisper (test bootstrap).
    pub(crate) fn clear_whisper(&mut self) {
        self.whisper = None;
    }
}

/// The `/haunt` admin controls, recorded by the composer and drained by
/// `svc::tick`. Deliberately absent from help and autocomplete, and only
/// ever parsed for admins: for everyone else the line posts as plain
/// text, exactly as if the command did not exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HauntCommand {
    /// `/haunt`: kill switch, once-ever mark, door and glitch state.
    Status,
    /// `/haunt on`: re-enable the haunting process-wide.
    On,
    /// `/haunt off`: the kill switch; a live whisper drops mid-scene and
    /// the glitch scheduler stops firing.
    Off,
    /// `/haunt glitch`: fire a clock-glitch burst right now.
    Glitch,
    /// `/haunt replay`: re-run the splash whisper now, ignoring the mark.
    Replay,
    /// `/haunt reset`: clear this user's once-ever mark.
    Reset,
}

/// `Some(Some(command))` on a well-formed `/haunt` line, `Some(None)` on
/// anything else after `/haunt` (usage banner), `None` when the line is
/// not a haunt command at all.
pub(crate) fn parse_haunt_command(body: &str) -> Option<Option<HauntCommand>> {
    let rest = body.trim().strip_prefix("/haunt")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(match rest.trim() {
        "" => Some(HauntCommand::Status),
        "on" => Some(HauntCommand::On),
        "off" => Some(HauntCommand::Off),
        "glitch" => Some(HauntCommand::Glitch),
        "replay" => Some(HauntCommand::Replay),
        "reset" => Some(HauntCommand::Reset),
        _ => None,
    })
}

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

/// Stage 1: the deniable clock glitch. Ticks are `App::marquee_tick`
/// (wall-derived 66ms units), so the schedule survives however sparsely
/// the adaptive loop runs.
///
/// A glitch is only legible against stability, so the target is the
/// sidebar clock (pinned core block, Home and Arcade). One burst swaps a
/// character or two of the rendered time for glyph-alphabet characters,
/// holds ~200ms, restores. Rolled per session with independent dice (two
/// people almost never see it together), rare, render-layer only, no DB.
///
/// How long one burst holds: 3 ticks (~200ms). One frame at 15fps is too
/// fast to trust; this survives the sidebar's ~132ms wake cadence.
const GLITCH_HOLD_TICKS: usize = 3;
/// Gap between bursts: order of once per hours-long session.
const GLITCH_GAP_MIN_TICKS: usize = 40 * 60 * 1000 / 66; // ~40 min
const GLITCH_GAP_MAX_TICKS: usize = 3 * 60 * 60 * 1000 / 66; // ~3 h
/// When the burst comes due while the clock is off screen, defer a little
/// instead of spending it invisibly.
const GLITCH_DEFER_MIN_TICKS: usize = 3 * 60 * 1000 / 66; // ~3 min
const GLITCH_DEFER_MAX_TICKS: usize = 10 * 60 * 1000 / 66; // ~10 min
/// At most this many bursts per UTC day per session.
const GLITCH_DAILY_CAP: u8 = 2;

/// What one `tick` decided. `Started` and `Ended` are the two frames the
/// owner must actually paint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlitchTick {
    Idle,
    Started,
    Ended,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ClockGlitch {
    /// xorshift64 state: this session's independent dice.
    rng: u64,
    /// Tick the next burst comes due.
    next_at: usize,
    /// Tick the live burst started, while one is showing.
    active_since: Option<usize>,
    /// UTC day the counter below counts within.
    today: Option<NaiveDate>,
    /// Bursts fired today (session-local; the daily cap needs no DB).
    fired_today: u8,
}

impl ClockGlitch {
    pub(crate) fn new(seed: u64, now_tick: usize) -> Self {
        let mut glitch = Self {
            rng: seed | 1,
            next_at: 0,
            active_since: None,
            today: None,
            fired_today: 0,
        };
        glitch.next_at = now_tick + glitch.roll(GLITCH_GAP_MIN_TICKS, GLITCH_GAP_MAX_TICKS);
        glitch
    }

    /// Advance one world tick. `clock_visible` is whether the sidebar
    /// clock is actually on screen; a burst never spends itself unseen.
    pub(crate) fn tick(
        &mut self,
        tick: usize,
        today: NaiveDate,
        enabled: bool,
        clock_visible: bool,
    ) -> GlitchTick {
        if let Some(since) = self.active_since {
            if tick.saturating_sub(since) >= GLITCH_HOLD_TICKS {
                self.active_since = None;
                self.next_at = tick + self.roll(GLITCH_GAP_MIN_TICKS, GLITCH_GAP_MAX_TICKS);
                return GlitchTick::Ended;
            }
            return GlitchTick::Idle;
        }
        if tick < self.next_at {
            return GlitchTick::Idle;
        }
        if self.today != Some(today) {
            self.today = Some(today);
            self.fired_today = 0;
        }
        if !enabled || self.fired_today >= GLITCH_DAILY_CAP {
            // The kill switch and the daily cap both re-dice the full gap;
            // the day rollover resets the counter when it next comes due.
            self.next_at = tick + self.roll(GLITCH_GAP_MIN_TICKS, GLITCH_GAP_MAX_TICKS);
            return GlitchTick::Idle;
        }
        if !clock_visible {
            self.next_at = tick + self.roll(GLITCH_DEFER_MIN_TICKS, GLITCH_DEFER_MAX_TICKS);
            return GlitchTick::Idle;
        }
        self.active_since = Some(tick);
        self.fired_today += 1;
        GlitchTick::Started
    }

    /// The live burst's corruption seed while one is showing, for the
    /// deterministic character swap at draw time.
    pub(crate) fn corruption(&self, tick: usize) -> Option<u64> {
        let since = self.active_since?;
        (tick.saturating_sub(since) < GLITCH_HOLD_TICKS)
            .then(|| self.rng ^ ((since as u64) << 1 | 1))
    }

    /// `/haunt glitch`: start a burst right now, bypassing schedule and
    /// caps. Admin test hook only.
    pub(crate) fn fire_now(&mut self, tick: usize) {
        self.active_since = Some(tick);
    }

    /// Ticks until the next scheduled burst, for `/haunt` status.
    pub(crate) fn next_in_ticks(&self, tick: usize) -> usize {
        self.next_at.saturating_sub(tick)
    }

    /// xorshift64, then scaled into `min..max`.
    fn roll(&mut self, min: usize, max: usize) -> usize {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        min + (self.rng as usize) % (max - min)
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
