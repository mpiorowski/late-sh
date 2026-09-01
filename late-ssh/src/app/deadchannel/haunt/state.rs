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

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

/// The game's first voice (stage 4): the character whose plea invites the
/// chosen into `#deadchannel`. Rides the proven ghost-user plumbing
/// (dedicated DB user, fixed fingerprint, @bartender's shape). The name
/// comes from GAME.md's own note that "afterglow" was reserved for naming
/// something inside the world; copy and name still face design review
/// before the fuse is lit for real users.
pub(crate) const VOICE_USERNAME: &str = "afterglow";
pub(crate) const VOICE_FINGERPRINT: &str = "afterglow-fp-000";

/// Days between the delivered whisper and the invitation DM ("some days
/// after the held door"). `/haunt invite` skips the wait for testing.
pub(crate) const INVITE_DELAY_DAYS: i64 = 2;

/// The invitation: a plea, not a pitch, ending with the only instruction
/// the entire haunting ever gives. Placeholder pool of one until design
/// review (GAME.md, stage 4); it persists as a real DM on purpose, so an
/// invitation can be followed three days later.
pub(crate) const INVITATION_PLEA: &str = "i don't have long on this channel. \
there is a city under your clubhouse, behind the screen, and something old \
is broadcasting at the bottom of it. the static has been trying your name \
for weeks. we need runners. if you're willing: /join #deadchannel";

/// This user's persisted first-contact marks, read from `users.settings`
/// at session bootstrap. One bundle so the root config carries one field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FirstContactMarks {
    /// Stage-1 clock-glitch bursts seen so far (opens stage 2 at
    /// [`GLITCH_TOTAL_CAP`]; quiets the clock after).
    pub(crate) glitch_hits: u32,
    /// Stage-2 name-flicker hits so far (arms stage 3 at
    /// [`NAME_TOTAL_CAP`]; caps stage 2).
    pub(crate) name_hits: u32,
    /// When the stage-3 whisper was delivered (schedules stage 4).
    pub(crate) whisper_at: Option<DateTime<Utc>>,
    /// When the stage-4 invitation DM was sent.
    pub(crate) invited_at: Option<DateTime<Utc>>,
}

impl FirstContactMarks {
    pub(crate) fn from_settings(settings: &serde_json::Value) -> Self {
        Self {
            glitch_hits: late_core::models::user::extract_first_contact_glitch_hits(settings),
            name_hits: late_core::models::user::extract_first_contact_name_hits(settings),
            whisper_at: late_core::models::user::extract_first_contact_whisper_at(settings),
            invited_at: late_core::models::user::extract_first_contact_invited_at(settings),
        }
    }

    /// Everything already spent: what test apps use so no stage can fire
    /// under an admin-permission test unless a test arms one on purpose.
    /// (`test_helpers` compiles unconditionally, so no `#[cfg(test)]`.)
    pub(crate) fn spent_for_tests() -> Self {
        Self {
            glitch_hits: u32::MAX,
            name_hits: u32::MAX,
            whisper_at: Some(Utc::now()),
            invited_at: Some(Utc::now()),
        }
    }
}

/// The haunting's one slot on `App`: every stage's machine and the flags
/// that gate them. The root owns the field; everything that reads or
/// writes it goes through `svc.rs` (orchestration) or `ui.rs` (draw).
pub(crate) struct HauntState {
    /// Stage 3: the armed splash whisper. `Some` only while the splash is
    /// holding the door for it.
    pub(crate) whisper: Option<WhisperState>,
    /// Stage 1: the clock-glitch scheduler. Session-local dice; only the
    /// lifetime burst counter persists.
    pub(crate) clock_glitch: Option<ClockGlitch>,
    /// Stage 2: the own-name flicker roller.
    pub(crate) name_flicker: Option<NameFlicker>,
    /// Persisted marks, mirrored at session start and kept honest
    /// in-session.
    pub(crate) marks: FirstContactMarks,
    /// Whether this session may haunt at all (the admin-scoped gate,
    /// evaluated once at arming).
    pub(crate) eligible: bool,
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
    /// `/haunt name`: force the next own send to flicker.
    Name,
    /// `/haunt replay`: re-run the splash whisper now, ignoring the mark.
    Replay,
    /// `/haunt invite`: send the invitation DM now, skipping the delay.
    Invite,
    /// `/haunt reset`: clear every first-contact mark for this user.
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
        "name" => Some(HauntCommand::Name),
        "replay" => Some(HauntCommand::Replay),
        "invite" => Some(HauntCommand::Invite),
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
/// The ladder's share of clock bursts (the persisted counter): once this
/// many have been seen, the clock goes quiet and stage 2 opens. The quiet
/// is part of the escalation; whether unchosen users keep an unbounded
/// ambient clock is a fuse-time question (GAME.md).
pub(crate) const GLITCH_TOTAL_CAP: u32 = 3;
/// Fuse on a forced burst: the `/haunt glitch` banner covers the sidebar
/// clock for ~5s, so the burst waits it out.
const GLITCH_FORCE_DELAY_TICKS: usize = 7 * 1000 / 66; // ~7 s

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
    /// Tick a forced (`/haunt glitch`) burst fires, while its fuse burns.
    forced_at: Option<usize>,
    /// UTC day the counter below counts within.
    today: Option<NaiveDate>,
    /// Bursts fired today (session-local; the daily cap needs no DB).
    fired_today: u8,
    /// Lifetime bursts, seeded from the persisted counter; at
    /// [`GLITCH_TOTAL_CAP`] the schedule goes quiet and stage 2 opens.
    total_hits: u32,
}

impl ClockGlitch {
    pub(crate) fn new(seed: u64, now_tick: usize, total_hits: u32) -> Self {
        let mut glitch = Self {
            rng: seed | 1,
            next_at: 0,
            active_since: None,
            forced_at: None,
            today: None,
            fired_today: 0,
            total_hits,
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
        if let Some(at) = self.forced_at {
            // A burning fuse bypasses the schedule, the caps, and the
            // visibility gate, exactly as the instant fire did: the admin
            // asked for it and is watching.
            if tick < at {
                return GlitchTick::Idle;
            }
            self.forced_at = None;
            self.active_since = Some(tick);
            self.total_hits = self.total_hits.saturating_add(1);
            return GlitchTick::Started;
        }
        if self.total_hits >= GLITCH_TOTAL_CAP {
            // The ladder's share of bursts has been seen: the clock goes
            // quiet for good, and the quiet is part of the escalation.
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
        self.total_hits = self.total_hits.saturating_add(1);
        GlitchTick::Started
    }

    /// Lifetime bursts including this session's; the caller mirrors it into
    /// the persisted marks on every `Started`.
    pub(crate) fn total_hits(&self) -> u32 {
        self.total_hits
    }

    /// The live burst's corruption seed while one is showing, for the
    /// deterministic character swap at draw time.
    pub(crate) fn corruption(&self, tick: usize) -> Option<u64> {
        let since = self.active_since?;
        (tick.saturating_sub(since) < GLITCH_HOLD_TICKS)
            .then(|| self.rng ^ ((since as u64) << 1 | 1))
    }

    /// `/haunt glitch`: light a short fuse (the command banner covers the
    /// sidebar clock for ~5s), then burst, bypassing schedule and caps.
    /// Admin test hook only.
    pub(crate) fn fire_now(&mut self, tick: usize) {
        self.forced_at = Some(tick + GLITCH_FORCE_DELAY_TICKS);
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

/// Stage 2: the corruption chooses you, and the target is your own name.
/// In the sender's session only, immediately after their message lands
/// (the one moment of guaranteed attention), the author label of that
/// just-landed message renders with two or three characters from the
/// glyph alphabet, holds ~800ms, heals. The body is never touched: the
/// escalation over stage 1 is targeting, not content. It only rolls once
/// stage 1 has run its course (`stage_open`), and only on a message that
/// renders its own author header (the landing hook in `chat/state.rs`
/// skips grouped continuations, whose label never draws).
///
/// How long one hit holds, in `App::marquee_tick` units (~800ms): well
/// past the clock's ~200ms, because stage 2 is meant to be hard to miss.
const NAME_HOLD_TICKS: usize = 12;
/// Order of one in dozens of sends.
const NAME_CHANCE_ONE_IN: u64 = 24;
/// At most one hit per UTC day.
const NAME_DAILY_CAP: u8 = 1;
/// The ladder's share of name hits (the persisted counter): the third
/// hit arms the stage-3 whisper for the next fresh connect. With the
/// one-per-day cap, stages 1 and 2 each spread over two or three days:
/// the full ladder is roughly a week of slow burn.
pub(crate) const NAME_TOTAL_CAP: u32 = 3;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NameFlicker {
    /// xorshift64 state: the per-send dice.
    rng: u64,
    /// The live hit: which message's author label is corrupted, since when.
    active: Option<(Uuid, usize)>,
    /// UTC day the daily counter counts within.
    today: Option<NaiveDate>,
    fired_today: u8,
    /// Lifetime hits, seeded from the persisted counter; this is also the
    /// stage-3 arming counter.
    total_hits: u32,
    /// `/haunt name`: the next own send hits regardless of dice and caps.
    force_next: bool,
}

impl NameFlicker {
    pub(crate) fn new(seed: u64, total_hits: u32) -> Self {
        Self {
            rng: seed | 1,
            active: None,
            today: None,
            fired_today: 0,
            total_hits,
            force_next: false,
        }
    }

    /// One of this session's own messages just landed. Rolls the dice;
    /// returns true when a hit starts (the caller repaints and persists
    /// the counter). `stage_open` is whether stage 1 has run its course
    /// (glitch hits at the cap): the ladder never skips a rung, but a
    /// forced (`/haunt name`) hit ignores it.
    pub(crate) fn note_own_message(
        &mut self,
        message_id: Uuid,
        tick: usize,
        today: NaiveDate,
        enabled: bool,
        stage_open: bool,
    ) -> bool {
        if self.active.is_some() || !enabled {
            return false;
        }
        if self.today != Some(today) {
            self.today = Some(today);
            self.fired_today = 0;
        }
        let forced = std::mem::take(&mut self.force_next);
        if !forced {
            if !stage_open || self.fired_today >= NAME_DAILY_CAP || self.total_hits >= NAME_TOTAL_CAP
            {
                return false;
            }
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            if self.rng % NAME_CHANCE_ONE_IN != 0 {
                return false;
            }
        }
        self.active = Some((message_id, tick));
        // Saturating: a forced hit bypasses the caps, and the marks of a
        // test app sit at the counter's ceiling on purpose.
        self.fired_today = self.fired_today.saturating_add(1);
        self.total_hits = self.total_hits.saturating_add(1);
        true
    }

    /// Advance one world tick; returns true on the heal edge (the caller
    /// repaints the healed label).
    pub(crate) fn tick(&mut self, tick: usize) -> bool {
        match self.active {
            Some((_, since)) if tick.saturating_sub(since) >= NAME_HOLD_TICKS => {
                self.active = None;
                true
            }
            _ => false,
        }
    }

    /// The live hit for the row builder: which message's author label to
    /// corrupt, and the burst seed for the deterministic swap.
    pub(crate) fn corruption(&self, tick: usize) -> Option<(Uuid, u64)> {
        let (message_id, since) = self.active?;
        (tick.saturating_sub(since) < NAME_HOLD_TICKS)
            .then(|| (message_id, self.rng ^ ((since as u64) << 1 | 1)))
    }

    pub(crate) fn total_hits(&self) -> u32 {
        self.total_hits
    }

    /// `/haunt name`: force the next own send to hit. Admin test hook.
    pub(crate) fn force_next(&mut self) {
        self.force_next = true;
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
