//! First contact state machines (GAME.md, First contact): stage 1, the
//! clock glitch ([`ClockGlitch`]), stage 2, the own-name flicker
//! ([`NameFlicker`]), stage 3, the held door ([`WhisperState`]), and the
//! eligibility gate ([`FirstContactGate`]) that decides who goes past
//! stage 1.
//!
//! The whisper is delivered on the splash screen, once per person ever:
//! this one time the splash does not skip. Input is always acknowledged
//! (static surges, the skip hint dissolves) but control is withheld until
//! the voiced line has landed; a hard time cap then opens the door no
//! matter what. Pure state machines: no I/O, no clock reads. `App` owns
//! arming, the switches, and the persistence.
//!
//! Replica rule (root CONTEXT.md): nothing here is a source of truth. The
//! lifetime and daily caps are enforced by conditional claims on the user
//! row; the machines only decide *when to ask* and hold their schedule
//! while the row answers. The switches are `app_flags` rows read through a
//! process-shared `watch`. Stage 1 fires for staff always and for
//! everyone once the `haunt_live` fuse is lit; stages 2-4 need the gate.

use chrono::{DateTime, Utc};
use late_core::models::app_flag::{AppFlag, AppFlags};
use late_core::models::user::{FirstContactBioVerdict, FirstContactHitCaps, FirstContactHitClaim};
use tokio::sync::{oneshot, watch};
use uuid::Uuid;

/// The game's first voice (stage 4): the character whose plea invites the
/// chosen into `#deadchannel`. Rides the proven ghost-user plumbing
/// (dedicated DB user, fixed fingerprint, @bartender's shape). The name
/// comes from GAME.md's own note that "afterglow" was reserved for naming
/// something inside the world; copy and name still face design review
/// before the fuse is lit for real users.
pub(crate) const VOICE_USERNAME: &str = "afterglow";
pub(crate) const VOICE_FINGERPRINT: &str = "afterglow-fp-000";

/// Days between the last delivered whisper and the invitation DM ("some
/// days after the held door"). `/haunt invite` skips the wait for testing.
pub(crate) const INVITE_DELAY_DAYS: i64 = 2;

/// How many times the held door plays per person: the first whisper
/// notices you, the second says something is trying to get through, and
/// the invitation follows the second. Enforced by the row
/// (`User::claim_first_contact_whisper`).
pub(crate) const WHISPER_TOTAL_CAP: u32 = 2;
/// The least time between two whispers, so the second never lands the
/// same evening as the first: it waits for a fresh connect on a later
/// day. Enforced by the row alongside the cap.
pub(crate) const WHISPER_GAP_HOURS: i64 = 24;

/// The invitation: a plea, not a pitch, ending with the only instruction
/// the entire haunting ever gives. Placeholder pool of one until design
/// review (GAME.md, stage 4); it persists as a real DM on purpose, so an
/// invitation can be followed three days later.
pub(crate) const INVITATION_PLEA: &str = "i don't have long on this channel. \
there is a city under your clubhouse, behind the screen, and something old \
is broadcasting at the bottom of it. the static has been trying your name \
for weeks. we need runners. if you're willing: /join #deadchannel";

/// The eligibility gate (GAME.md, "the static chooses the invested"):
/// stages 2-4 target people who have put in the hours, deliberately
/// touched settings, and written a bio that reads as a person. Evaluated
/// once at session bootstrap (the user row that already loads plus one
/// primary-key read of `user_online_time`), and never stored: filling
/// your bio tonight means the static can find you tomorrow. The
/// thresholds are placeholders pending design review (GAME.md, Open
/// questions, "First-contact tuning").
///
/// Tenure is connected time, not account age: an account that signed up
/// a year ago and left is not invested, one that lived here for a week
/// is. Read from the online-time leaderboard's table.
pub(crate) const ACTIVE_MIN_HOURS: i64 = 7 * 24;
pub(crate) const BIO_MIN_CHARS: usize = 100;
pub(crate) const TOUCHED_SETTINGS_MIN: usize = 2;
/// A failed or stranded (pending) bio screen is claimed again after this
/// long; a pass is final for that text.
pub(crate) const BIO_RESCREEN_AFTER_HOURS: i64 = 24;

/// Where the bio leg of the gate stands for the bio text on the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BioStanding {
    /// Under [`BIO_MIN_CHARS`]: no screen is spent on it.
    TooShort,
    /// AI is switched off in this install and no pass is on record, so the
    /// leg cannot pass. Failing closed on a nonrenewable resource.
    AiOff,
    /// No usable verdict for the current text (never screened, the text
    /// changed, or the last verdict is stale): this session claims one.
    Unscreened,
    /// A screen is in flight, claimed by some session on some replica.
    Pending,
    Passed,
    Failed,
}

/// What the gate said at connect: passed, or the first leg that failed in
/// the order bootstrap checks them (the free legs before the paid bio
/// one). The connect log carries it per person and the
/// `late_ssh_first_contact_gate_total` counter tallies it, so a threshold
/// can be tuned against how many it actually turns away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateVerdict {
    Passed,
    TooFewHours,
    TooFewSettings,
    BioTooShort,
    BioAiOff,
    BioUnscreened,
    BioPending,
    BioFailed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FirstContactGate {
    /// Lifetime connected hours, as last flushed.
    pub(crate) active_hours: i64,
    pub(crate) touched_settings: usize,
    pub(crate) bio_chars: usize,
    pub(crate) bio: BioStanding,
}

impl FirstContactGate {
    pub(crate) fn evaluate(
        now: DateTime<Utc>,
        online_milliseconds: i64,
        settings: &serde_json::Value,
        ai_enabled: bool,
    ) -> Self {
        let bio = late_core::models::user::extract_bio(settings);
        let bio_chars = bio.chars().count();
        let screen = late_core::models::user::extract_first_contact_bio_screen(settings);
        let stale_after = chrono::Duration::hours(BIO_RESCREEN_AFTER_HOURS);
        let standing = if bio_chars < BIO_MIN_CHARS {
            BioStanding::TooShort
        } else {
            match screen {
                Some(screen) if screen.hash == bio_hash(&bio) => match screen.verdict {
                    FirstContactBioVerdict::Passed => BioStanding::Passed,
                    FirstContactBioVerdict::Pending | FirstContactBioVerdict::Failed
                        if now - screen.at >= stale_after =>
                    {
                        if ai_enabled {
                            BioStanding::Unscreened
                        } else {
                            BioStanding::AiOff
                        }
                    }
                    FirstContactBioVerdict::Pending => BioStanding::Pending,
                    FirstContactBioVerdict::Failed => BioStanding::Failed,
                },
                // A verdict for other text, or none at all.
                Some(_) | None => {
                    if ai_enabled {
                        BioStanding::Unscreened
                    } else {
                        BioStanding::AiOff
                    }
                }
            }
        };
        Self {
            active_hours: online_milliseconds / (60 * 60 * 1000),
            touched_settings: late_core::models::user::count_touched_settings(settings),
            bio_chars,
            bio: standing,
        }
    }

    /// All three legs hold: the static may choose this person.
    pub(crate) fn passes(&self) -> bool {
        self.verdict() == GateVerdict::Passed
    }

    pub(crate) fn verdict(&self) -> GateVerdict {
        if self.active_hours < ACTIVE_MIN_HOURS {
            return GateVerdict::TooFewHours;
        }
        if self.touched_settings < TOUCHED_SETTINGS_MIN {
            return GateVerdict::TooFewSettings;
        }
        match self.bio {
            BioStanding::Passed => GateVerdict::Passed,
            BioStanding::TooShort => GateVerdict::BioTooShort,
            BioStanding::AiOff => GateVerdict::BioAiOff,
            BioStanding::Unscreened => GateVerdict::BioUnscreened,
            BioStanding::Pending => GateVerdict::BioPending,
            BioStanding::Failed => GateVerdict::BioFailed,
        }
    }

    /// Whether bootstrap should claim a bio screen for this session. The
    /// two free legs come first: a screen is a paid AI call, and a person
    /// without the hours or the touched settings cannot pass the gate
    /// whatever the verdict says, so their bio is not read until they can.
    pub(crate) fn needs_bio_screen(&self) -> bool {
        if !self.tenure_and_settings_pass() {
            return false;
        }
        match self.bio {
            BioStanding::Unscreened => true,
            BioStanding::TooShort
            | BioStanding::AiOff
            | BioStanding::Pending
            | BioStanding::Passed
            | BioStanding::Failed => false,
        }
    }

    fn tenure_and_settings_pass(&self) -> bool {
        self.active_hours >= ACTIVE_MIN_HOURS && self.touched_settings >= TOUCHED_SETTINGS_MIN
    }

    /// Nothing passes, and nothing is worth screening: what bootstrap
    /// returns when the fuse is unlit, and what test apps use so no stage
    /// past 1 can arm unless a test arms one on purpose.
    pub(crate) fn closed() -> Self {
        Self {
            active_hours: 0,
            touched_settings: 0,
            bio_chars: 0,
            bio: BioStanding::TooShort,
        }
    }
}

/// The key a bio screen verdict is filed under: a rewritten bio never
/// inherits a verdict. Sixteen hex characters of blake3 is plenty for one
/// user's own history of bios.
pub(crate) fn bio_hash(bio: &str) -> String {
    blake3::hash(bio.trim().as_bytes()).to_hex()[..16].to_string()
}

pub(crate) fn glitch_caps() -> FirstContactHitCaps {
    FirstContactHitCaps {
        daily: GLITCH_DAILY_CAP,
        total: GLITCH_TOTAL_CAP,
    }
}

pub(crate) fn name_caps() -> FirstContactHitCaps {
    FirstContactHitCaps {
        daily: NAME_DAILY_CAP,
        total: NAME_TOTAL_CAP,
    }
}

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
    /// Stage-3 whispers delivered so far (the door plays
    /// [`WHISPER_TOTAL_CAP`] times; the last one schedules stage 4).
    pub(crate) whisper_hits: u32,
    /// When the last stage-3 whisper was delivered (spaces the whispers
    /// apart; at the cap, schedules stage 4).
    pub(crate) whisper_at: Option<DateTime<Utc>>,
    /// When the stage-4 invitation DM was sent.
    pub(crate) invited_at: Option<DateTime<Utc>>,
}

impl FirstContactMarks {
    pub(crate) fn from_settings(settings: &serde_json::Value) -> Self {
        Self {
            glitch_hits: late_core::models::user::extract_first_contact_glitch_hits(settings),
            name_hits: late_core::models::user::extract_first_contact_name_hits(settings),
            whisper_hits: late_core::models::user::extract_first_contact_whisper_hits(settings),
            whisper_at: late_core::models::user::extract_first_contact_whisper_at(settings),
            invited_at: late_core::models::user::extract_first_contact_invited_at(settings),
        }
    }

    /// Whether the held door is owed at `now`: under the cap, and either
    /// never played or played at least [`WHISPER_GAP_HOURS`] ago. The row
    /// re-judges the same rule on delivery.
    pub(crate) fn whisper_due(&self, now: DateTime<Utc>) -> bool {
        if self.whisper_hits >= WHISPER_TOTAL_CAP {
            return false;
        }
        match self.whisper_at {
            None => true,
            Some(at) => now - at >= chrono::Duration::hours(WHISPER_GAP_HOURS),
        }
    }

    /// Whether every whisper has played: the stage-4 clock runs from
    /// `whisper_at` only once this holds.
    pub(crate) fn whispers_spent(&self) -> bool {
        self.whisper_hits >= WHISPER_TOTAL_CAP
    }

    /// Everything already spent: what test apps use so no stage can fire
    /// under an admin-permission test unless a test arms one on purpose.
    #[cfg(test)]
    pub(crate) fn spent_for_tests() -> Self {
        Self {
            glitch_hits: u32::MAX,
            name_hits: u32::MAX,
            whisper_hits: u32::MAX,
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
    /// Stage 1: the clock-glitch scheduler. Session-local dice; the caps
    /// live on the row.
    pub(crate) clock_glitch: Option<ClockGlitch>,
    /// Stage 2: the own-name flicker roller.
    pub(crate) name_flicker: Option<NameFlicker>,
    /// Persisted marks, mirrored at session start and kept honest
    /// in-session from every won claim.
    pub(crate) marks: FirstContactMarks,
    /// The eligibility gate as evaluated at bootstrap (for `/haunt` status
    /// and the arming decision).
    pub(crate) gate: FirstContactGate,
    /// Stage 1 armed for this session: staff always, everyone once the
    /// `haunt_live` fuse is lit. Evaluated once at arming.
    pub(crate) stage1: bool,
    /// Stages 2-4 armed: stage 1 plus the gate, or a funnel already entered
    /// (eligibility gates entering, never continuing). `/haunt on` forces
    /// it for the session.
    pub(crate) chosen: bool,
    /// Process-wide switches (`app/flags`), one `watch` shared by every
    /// session on this replica and kept in step across replicas by the
    /// `app_flag_changed` notify. `None` until the first load: off.
    pub(crate) flags: watch::Receiver<Option<AppFlags>>,
    /// Capped hit claims in flight (at most one per machine). The machine
    /// that asked holds its schedule until the row answers; `svc::tick`
    /// drains these.
    pub(crate) pending_claims: Vec<PendingClaim>,
    /// `/haunt on|off|live` writes in flight. The banner waits for the
    /// row's answer, so an admin flipping the kill switch is told what
    /// actually happened; `svc::tick` drains these.
    pub(crate) pending_flag_writes: Vec<PendingFlagWrite>,
}

impl HauntState {
    /// The kill switch. `None` (flags never loaded) reads as off.
    pub(crate) fn enabled(&self) -> bool {
        (*self.flags.borrow()).is_some_and(|flags| flags.haunt_enabled)
    }

    /// The fuse: stage 1 for everyone, not only staff.
    pub(crate) fn live(&self) -> bool {
        (*self.flags.borrow()).is_some_and(|flags| flags.haunt_live)
    }

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

/// Which machine asked the row for a hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HitStage {
    Glitch,
    Name { message_id: Uuid },
}

/// One claim out on the row. An `Err` (or a dropped sender) means the row
/// could not be asked; the machine defers.
pub(crate) struct PendingClaim {
    pub(crate) stage: HitStage,
    pub(crate) rx: oneshot::Receiver<anyhow::Result<FirstContactHitClaim>>,
}

/// One `app_flags` write out on the row, with the banner to show once it
/// lands.
pub(crate) struct PendingFlagWrite {
    pub(crate) flag: AppFlag,
    pub(crate) enabled: bool,
    pub(crate) done: &'static str,
    pub(crate) rx: oneshot::Receiver<anyhow::Result<()>>,
}

/// The `/haunt` admin controls, recorded by the composer and drained by
/// `svc::tick`. Deliberately absent from help and autocomplete, and only
/// ever parsed for admins: for everyone else the line posts as plain
/// text, exactly as if the command did not exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HauntCommand {
    /// `/haunt`: switches, gate, marks, door and glitch state.
    Status,
    /// `/haunt on`: re-enable the haunting everywhere (the kill switch row).
    On,
    /// `/haunt off`: the kill switch; a live whisper drops mid-scene and
    /// the schedulers stop firing, on every replica.
    Off,
    /// `/haunt live on`: light the fuse; stage 1 fires for everyone.
    LiveOn,
    /// `/haunt live off`: back to staff only.
    LiveOff,
    /// `/haunt glitch`: fire a clock-glitch burst right now.
    Glitch,
    /// `/haunt name`: force the next own send to flicker.
    Name,
    /// `/haunt replay`: re-run the splash whisper now, ignoring the mark.
    Replay,
    /// `/haunt invite`: send the invitation DM now, skipping the delay.
    Invite,
    /// `/haunt reset`: clear every first-contact chain mark for this user.
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
    let words: Vec<&str> = rest.split_whitespace().collect();
    Some(match words.as_slice() {
        [] => Some(HauntCommand::Status),
        ["on"] => Some(HauntCommand::On),
        ["off"] => Some(HauntCommand::Off),
        ["live", "on"] => Some(HauntCommand::LiveOn),
        ["live", "off"] => Some(HauntCommand::LiveOff),
        ["glitch"] => Some(HauntCommand::Glitch),
        ["name"] => Some(HauntCommand::Name),
        ["replay"] => Some(HauntCommand::Replay),
        ["invite"] => Some(HauntCommand::Invite),
        ["reset"] => Some(HauntCommand::Reset),
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

/// The voiced lines for the first held door: the static has noticed
/// you. Screenshot-test vocabulary only (static, signal, city, channel,
/// door); a repeated whisper is a bug report, not a haunting, so this
/// pool grows under the same variety discipline as feed templates
/// (GAME.md, Open questions) before it ever leaves staff scope.
const WHISPER_LINES: [&str; 6] = [
    "you were not supposed to notice this yet",
    "the sky down here is the color of a dead channel",
    "the rain in this city falls as static",
    "something behind the screen just learned your name",
    "the door is held. not yet.",
    "there is a city under this room. it noticed you",
];

/// The voiced lines for the second held door, a later day: whatever
/// noticed you is now trying to get through. The escalation is in the
/// verb, the vocabulary stays the same.
const WHISPER_LINES_SECOND: [&str; 4] = [
    "something is trying to break in. do you see it?",
    "it found the door. it is pushing from the other side",
    "the static is not weather anymore. it is knocking",
    "listen. that is not the signal dropping. that is something coming up",
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
    /// The door for this user's next whisper: `delivered_before` is how
    /// many have already played, which picks the pool (the first door
    /// notices you, every later one is something trying to get through).
    pub(crate) fn for_user(user_id: Uuid, delivered_before: u32) -> Self {
        Self::with_seed(user_id.as_u128() as u64, delivered_before)
    }

    fn with_seed(seed: u64, delivered_before: u32) -> Self {
        let line = match delivered_before {
            0 => WHISPER_LINES[(seed % WHISPER_LINES.len() as u64) as usize],
            _ => WHISPER_LINES_SECOND[(seed % WHISPER_LINES_SECOND.len() as u64) as usize],
        };
        Self {
            line,
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
/// people almost never see it together), rare, render-layer only. The
/// one DB touch per burst is the capped claim on the user row, which is
/// what makes the daily and lifetime caps exact across devices and
/// replicas: the machine says `Due`, holds its schedule, and starts only
/// on a won claim.
///
/// How long one burst holds: 3 ticks (~200ms). One frame at 15fps is too
/// fast to trust; this survives the sidebar's ~132ms wake cadence.
const GLITCH_HOLD_TICKS: usize = 3;
/// Gap between bursts: order of once per hours-long session.
const GLITCH_GAP_MIN_TICKS: usize = 40 * 60 * 1000 / 66; // ~40 min
const GLITCH_GAP_MAX_TICKS: usize = 3 * 60 * 60 * 1000 / 66; // ~3 h
/// When the burst comes due while the clock is off screen, or the row
/// could not be asked, defer a little instead of spending it invisibly.
const GLITCH_DEFER_MIN_TICKS: usize = 3 * 60 * 1000 / 66; // ~3 min
const GLITCH_DEFER_MAX_TICKS: usize = 10 * 60 * 1000 / 66; // ~10 min
/// At most this many bursts per UTC day per person (enforced by the row).
pub(crate) const GLITCH_DAILY_CAP: u32 = 2;
/// The ladder's share of clock bursts (the persisted counter): once this
/// many have been seen, the clock goes quiet and stage 2 opens. The quiet
/// is part of the escalation; whether unchosen users keep an unbounded
/// ambient clock is a fuse-time question (GAME.md).
pub(crate) const GLITCH_TOTAL_CAP: u32 = 3;
/// Fuse on a forced burst: the `/haunt glitch` banner covers the sidebar
/// clock for ~5s, so the burst waits it out.
const GLITCH_FORCE_DELAY_TICKS: usize = 7 * 1000 / 66; // ~7 s

/// What one `tick` decided. `Started` and `Ended` are the two frames the
/// owner must actually paint; `Due` asks the owner to claim a burst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlitchTick {
    Idle,
    /// The schedule came due with the clock on screen: claim one burst on
    /// the row, then call `start` or `claim_capped`/`claim_failed`.
    Due,
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
    /// A claim is out on the row; the schedule holds until it answers.
    claiming: bool,
    /// Lifetime bursts, seeded from the persisted counter and refreshed by
    /// every claim answer; at [`GLITCH_TOTAL_CAP`] the schedule goes quiet
    /// and stage 2 opens.
    total_hits: u32,
}

impl ClockGlitch {
    pub(crate) fn new(seed: u64, now_tick: usize, total_hits: u32) -> Self {
        let mut glitch = Self {
            rng: seed | 1,
            next_at: 0,
            active_since: None,
            forced_at: None,
            claiming: false,
            total_hits,
        };
        glitch.next_at = now_tick + glitch.roll(GLITCH_GAP_MIN_TICKS, GLITCH_GAP_MAX_TICKS);
        glitch
    }

    /// Advance one world tick. `clock_visible` is whether the sidebar
    /// clock is actually on screen; a burst never spends itself unseen.
    pub(crate) fn tick(&mut self, tick: usize, enabled: bool, clock_visible: bool) -> GlitchTick {
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
            self.claiming = false;
            self.active_since = Some(tick);
            self.total_hits = self.total_hits.saturating_add(1);
            return GlitchTick::Started;
        }
        if self.claiming {
            return GlitchTick::Idle;
        }
        if self.total_hits >= GLITCH_TOTAL_CAP {
            // The ladder's share of bursts has been seen: the clock goes
            // quiet for good, and the quiet is part of the escalation.
            return GlitchTick::Idle;
        }
        if tick < self.next_at {
            return GlitchTick::Idle;
        }
        if !enabled {
            // The kill switch re-dices the full gap.
            self.next_at = tick + self.roll(GLITCH_GAP_MIN_TICKS, GLITCH_GAP_MAX_TICKS);
            return GlitchTick::Idle;
        }
        if !clock_visible {
            self.next_at = tick + self.roll(GLITCH_DEFER_MIN_TICKS, GLITCH_DEFER_MAX_TICKS);
            return GlitchTick::Idle;
        }
        self.claiming = true;
        GlitchTick::Due
    }

    /// The row granted the burst: show it now. `hits` is the lifetime
    /// counter after the claim.
    pub(crate) fn start(&mut self, tick: usize, hits: u32) {
        self.claiming = false;
        self.active_since = Some(tick);
        self.total_hits = hits;
    }

    /// The row refused (today's or the lifetime cap): a full re-dice, and
    /// the mirror takes the row's count so a spent share goes quiet here
    /// too.
    pub(crate) fn claim_capped(&mut self, tick: usize, hits: u32) {
        self.claiming = false;
        self.total_hits = hits;
        self.next_at = tick + self.roll(GLITCH_GAP_MIN_TICKS, GLITCH_GAP_MAX_TICKS);
    }

    /// The row could not be asked: defer a little and try again.
    pub(crate) fn claim_failed(&mut self, tick: usize) {
        self.claiming = false;
        self.next_at = tick + self.roll(GLITCH_DEFER_MIN_TICKS, GLITCH_DEFER_MAX_TICKS);
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
            .then_some(self.rng ^ ((since as u64) << 1 | 1))
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
/// glyph alphabet in two waves back to back, each with its own glyphs
/// (~800ms, then a different corruption for ~800ms more), then heals. The
/// body is never touched: the escalation over stage 1 is targeting, not
/// content. It only rolls once
/// stage 1 has run its course (`stage_open`), and only on a message that
/// renders its own author header (the landing hook in `chat/state.rs`
/// skips grouped continuations, whose label never draws). A natural hit
/// is a claim on the row first (the daily and lifetime caps live there)
/// and shows on the tick the claim comes back won.
///
/// How long one wave of a hit holds, in `App::marquee_tick` units
/// (~800ms): well past the clock's ~200ms, because stage 2 is meant to be
/// hard to miss.
const NAME_WAVE_TICKS: usize = 12;
/// Waves per hit (decided 2026-09-03): the label corrupts, then corrupts
/// differently, then heals. Twice the hold of one wave, and the second
/// wave rolls its own glyphs, so an eye that wrote the first off as a
/// render hiccup is caught by the change.
const NAME_WAVES: usize = 2;
/// How long the whole hit holds.
const NAME_HOLD_TICKS: usize = NAME_WAVE_TICKS * NAME_WAVES;
/// Order of one in dozens of sends.
const NAME_CHANCE_ONE_IN: u64 = 24;
/// At most one hit per UTC day per person (enforced by the row).
pub(crate) const NAME_DAILY_CAP: u32 = 1;
/// The ladder's share of name hits (the persisted counter): the third
/// hit arms the stage-3 whisper for the next fresh connect. With the
/// one-per-day cap, stages 1 and 2 each spread over two or three days:
/// the full ladder is roughly a week of slow burn.
pub(crate) const NAME_TOTAL_CAP: u32 = 3;

/// What a landed own message rolled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameRoll {
    Miss,
    /// The dice landed: claim one hit on the row, then `start` or
    /// `claim_capped`/`claim_failed`.
    Claim,
    /// `/haunt name`: the hit is already showing, uncapped.
    Forced,
}

/// The live hit: which message's author label is corrupted, since when,
/// and which wave the rows were last rebuilt for.
#[derive(Debug, PartialEq, Eq)]
struct ActiveHit {
    message_id: Uuid,
    since: usize,
    painted_wave: usize,
}

impl ActiveHit {
    fn new(message_id: Uuid, since: usize) -> Self {
        Self {
            message_id,
            since,
            painted_wave: 0,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct NameFlicker {
    /// xorshift64 state: the per-send dice.
    rng: u64,
    active: Option<ActiveHit>,
    /// A claim is out on the row for this message's label.
    claiming: Option<Uuid>,
    /// Lifetime hits, seeded from the persisted counter and refreshed by
    /// every claim answer; this is also the stage-3 arming counter.
    total_hits: u32,
    /// `/haunt name`: the next own send hits regardless of dice and caps.
    force_next: bool,
}

impl NameFlicker {
    pub(crate) fn new(seed: u64, total_hits: u32) -> Self {
        Self {
            rng: seed | 1,
            active: None,
            claiming: None,
            total_hits,
            force_next: false,
        }
    }

    /// One of this session's own messages just landed. Rolls the dice.
    /// `stage_open` is whether stage 1 has run its course (glitch hits at
    /// the cap): the ladder never skips a rung, but a forced (`/haunt
    /// name`) hit ignores it, and the caps, and shows at once.
    pub(crate) fn note_own_message(
        &mut self,
        message_id: Uuid,
        tick: usize,
        enabled: bool,
        stage_open: bool,
    ) -> NameRoll {
        if self.active.is_some() || self.claiming.is_some() || !enabled {
            return NameRoll::Miss;
        }
        if std::mem::take(&mut self.force_next) {
            self.active = Some(ActiveHit::new(message_id, tick));
            // Saturating: the marks of a test app sit at the ceiling.
            self.total_hits = self.total_hits.saturating_add(1);
            return NameRoll::Forced;
        }
        if !stage_open || self.total_hits >= NAME_TOTAL_CAP {
            return NameRoll::Miss;
        }
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        if !self.rng.is_multiple_of(NAME_CHANCE_ONE_IN) {
            return NameRoll::Miss;
        }
        self.claiming = Some(message_id);
        NameRoll::Claim
    }

    /// The row granted the hit: corrupt `message_id`'s label from `tick`.
    pub(crate) fn start(&mut self, message_id: Uuid, tick: usize, hits: u32) {
        self.claiming = None;
        self.active = Some(ActiveHit::new(message_id, tick));
        self.total_hits = hits;
    }

    /// The row refused (today's or the lifetime cap); the mirror takes the
    /// row's count.
    pub(crate) fn claim_capped(&mut self, hits: u32) {
        self.claiming = None;
        self.total_hits = hits;
    }

    /// The row could not be asked; the next send rolls again.
    pub(crate) fn claim_failed(&mut self) {
        self.claiming = None;
    }

    /// Advance one world tick; returns true on a repaint edge: the start
    /// of the next wave (the caller repaints the label with that wave's
    /// glyphs) and the heal (the caller repaints the healed label).
    pub(crate) fn tick(&mut self, tick: usize) -> bool {
        let Some(hit) = self.active.as_mut() else {
            return false;
        };
        let elapsed = tick.saturating_sub(hit.since);
        if elapsed >= NAME_HOLD_TICKS {
            self.active = None;
            return true;
        }
        let wave = elapsed / NAME_WAVE_TICKS;
        if wave == hit.painted_wave {
            return false;
        }
        hit.painted_wave = wave;
        true
    }

    /// The live hit for the row builder: which message's author label to
    /// corrupt, and the burst seed for the deterministic swap. Each wave
    /// of the hit hands out its own seed, so the swapped glyphs change at
    /// the wave edge; the first wave's seed is the hit's own.
    pub(crate) fn corruption(&self, tick: usize) -> Option<(Uuid, u64)> {
        let hit = self.active.as_ref()?;
        let elapsed = tick.saturating_sub(hit.since);
        if elapsed >= NAME_HOLD_TICKS {
            return None;
        }
        let wave = (elapsed / NAME_WAVE_TICKS) as u64;
        let seed = self.rng ^ ((hit.since as u64) << 1 | 1);
        Some((
            hit.message_id,
            seed ^ wave.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        ))
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
