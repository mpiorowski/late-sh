//! The shot being composed, and everything else pool keeps per-session.
//!
//! This file is the pool game's half of the split that `state.rs` documents:
//! the shared daily state owns the *seam* — the roster arms, the reload
//! machinery, the service handle — and everything that is purely pool lives
//! here. The rule of thumb that fell out of building it: if a function takes
//! `(&mut PoolDraft, &DailyPoolState)` and touches no `DailyState` field, it
//! belongs in this file, and the first version of pool that ignored that rule
//! put a thousand lines of game into the shared state.
//!
//! Nothing here talks to the service or the database. A draft becomes a
//! `Shot` (`PoolDraft::shot`) and the seam sends it; a playback is handed in
//! by the seam when a reload brings a shot worth watching.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;

use crate::app::games::pool_core::{
    aim::{self, ShotLine},
    ball::CUE,
    cue::{MAX_SPEED, MISCUE_LIMIT, PowerBand, ShotMode},
    cue_ui::PanelHit,
    rack, rules as pool_rules,
    shot::{BallFrame, Shot, Timeline},
};

use super::pool::{DailyPoolState, PoolAimShare};

/// Where the cue panel drew its parts, for the click hit test. The pixel
/// geometry inside `area` is exactly what `cue_ui::draw` hands back, so the
/// input path never reconstructs the panel's own layout.
#[derive(Clone, Copy, Debug)]
pub struct PoolCueHit {
    pub area: Rect,
    pub panel: PanelHit,
}

pub struct PoolDetail {
    pub state: DailyPoolState,
    /// The shot being composed. Only sent when the player actually strikes.
    pub draft: PoolDraft,
    /// A shot left this session and hasn't come back via reload yet. Pool
    /// cannot apply optimistically the way the other games do — the result is
    /// a simulation, not a rule — so this is what blocks a second shot.
    pub shot_in_flight: bool,
    /// The shot currently being watched. Set when a reload brings in a shot
    /// this session has not shown yet; cleared when it finishes playing.
    pub playback: Option<PoolPlayback>,
    /// What the *other* player is lining up, as their board last broadcast it.
    ///
    /// A daily game is otherwise a series of still frames a day apart, and
    /// watching an opponent pick a ball and turn the cue onto it is the only
    /// moment the correspondence version has the texture of the real one. It
    /// is presentation only: nothing here can become a move, and the shot
    /// itself still arrives as a reload like every other game's.
    pub watching: Option<PoolAimShare>,
}

impl PoolDetail {
    /// Take over what the detail being replaced was in the middle of showing.
    ///
    /// The playback because a reload must not delete a shot that is still
    /// rolling, and the watched aim because the opponent is not going to
    /// re-broadcast it just because this board reloaded.
    pub fn adopt(&mut self, previous: &mut PoolDetail) {
        self.playback = previous.playback.take();
        self.watching = previous.watching.take();
    }
}

/// A shot being animated: the re-derived timeline plus when it started.
///
/// The timeline is never stored or sent — `DailyPoolState` keeps the rack from
/// before the shot, and this is re-simulated from it. Both players therefore
/// watch the same balls take the same path without any of it crossing the
/// wire, which is the whole reason the simulation had to be deterministic.
pub struct PoolPlayback {
    pub timeline: Timeline,
    started: Instant,
    /// How long the shot is worth *watching* — see `Timeline::visible_duration`.
    /// A shot ends for the player when the picture stops changing; the tail of
    /// millimetre creep the physics still has to resolve is time spent looking
    /// at a settled table wondering why the board will not take a shot.
    visible: f64,
}

impl PoolPlayback {
    pub fn new(timeline: Timeline) -> Self {
        Self {
            visible: timeline.visible_duration(),
            timeline,
            started: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    /// A beat of stillness after the balls stop, so the final position
    /// registers as the result of the shot rather than a jump cut.
    pub fn finished(&self) -> bool {
        self.elapsed() > self.visible + PLAYBACK_HOLD
    }

    pub fn frame(&self) -> Vec<BallFrame> {
        self.timeline.sample(self.elapsed())
    }
}

/// Seconds the settled rack is held on screen after a shot finishes playing.
const PLAYBACK_HOLD: f64 = 0.6;

/// Floor on how often a board tells the other side what it is lining up.
/// Fast enough to read as somebody moving a cue, slow enough that a pointer
/// sweep is a handful of events rather than one per terminal cell.
const AIM_SHARE_INTERVAL: Duration = Duration::from_millis(120);

/// The shot being composed, and the aiming aids around it.
///
/// The aim is a **bearing** and nothing else. Which ball it is on, where the
/// cue ball will touch it, where the object ball goes, which rail a miss
/// meets: all of that is read back off the table by `pool_core::aim`, never
/// stored, so the picture and the shot cannot disagree. Turning the bearing
/// is what every aiming control does, whether it is a key, the pointer, a
/// click on a ball or a pot line; they differ only in how far.
///
/// **Nothing here is sequenced.** Target, spin, aim and stroke are all live at
/// once and a player may strike at any moment; `mode` only says which of them
/// the mouse is steering. The reason it is a mode at all rather than a held
/// key is that a terminal has no key-up event to observe (see `ShotMode`).
pub struct PoolDraft {
    pub mode: ShotMode,
    /// Where the cue ball is sent, in radians from table +x, increasing
    /// clockwise on the overview (which draws +y downward). The only aiming
    /// state there is; everything drawn is read back off the table from it.
    pub azimuth: f64,
    /// The ball last *picked* by name (a click, `[`/`]`, `'`), which is not
    /// always the ball the line is on: pick a ball hidden behind another and
    /// the line stops at the one in front. The brackets step from here, or
    /// stepping onto a hidden ball would be stepping onto the same ball for
    /// ever. Nothing else reads it; the picture follows the line.
    pub picked: Option<u8>,
    /// Tip placement on the cue ball's face, `[across, up]` in ball radii.
    pub tip: [f64; 2],
    /// How far the cue is drawn back, 0 to 1 *within the armed band*. The band
    /// is what turns it into a speed, so the same pull is a gentle roll in
    /// `light` and a break in `strong`.
    pub pull: f64,
    /// Ball-in-hand placement, when the state grants one.
    pub place: Option<[f64; 2]>,
    pub called_pocket: Option<u8>,
    /// Last pointer position seen while a mode was armed. Motion is applied as
    /// a delta from here rather than against a fixed anchor, so arming a mode
    /// never yanks the setting to wherever the pointer happened to be sitting.
    last_pointer: Option<(u16, u16)>,
    /// A button went down and the pointer has moved since: releasing strikes.
    dragging: bool,
    /// What the adjustable values were when the current mode was armed, so a
    /// right-click or an Esc can put them back. `None` while nothing is armed.
    restore: Option<DraftRestore>,
    /// Row the current stroke began on — where the cue ball is, as far as the
    /// gesture is concerned. The pull is measured from here rather than
    /// accumulated, so drawing back to the same place is always the same
    /// power, and pushing back *past* it is unambiguously a strike.
    stroke_origin: Option<u16>,
    /// Furthest the cue was drawn back during this stroke. The strike uses
    /// this rather than the pull at the moment of contact: on a real table the
    /// backswing is what decides the power, not how the follow-through is
    /// timed.
    backswing: f64,
}

/// The values a mode can change, snapshotted at arming time.
#[derive(Clone, Copy)]
struct DraftRestore {
    azimuth: f64,
    tip: [f64; 2],
    pull: f64,
    place: Option<[f64; 2]>,
}

/// How far the cue must be drawn back before pushing forward counts as a
/// stroke. Without it a twitch of the mouse in the wrong direction fires.
const MIN_BACKSWING: f64 = 0.08;

/// What a pointer report did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerOutcome {
    /// Nothing was armed, or nothing moved. The caller need not repaint.
    Ignored,
    Changed,
    /// The cue was pushed forward through the ball: play the shot.
    Strike,
}

/// How far a keypress turns the aim. A degree is a ball's width at about a
/// metre and a half, so held down it sweeps the table in a few seconds and
/// tapped it walks across a ball; the shifted step is for the last fraction.
const AIM_STEP: f64 = 1.0 * std::f64::consts::PI / 180.0;
const AIM_FINE_STEP: f64 = 0.1 * std::f64::consts::PI / 180.0;
/// Tip movement per keypress, in ball radii.
const TIP_STEP: f64 = 0.05;
/// Pull per keypress, for the keyboard-only path.
const PULL_STEP: f64 = 0.05;

// ── Pointer sensitivity ───────────────────────────────────────────────
//
// All three are per *terminal cell*, and a cell is about twice as tall as it
// is wide, so the vertical rates are roughly double their horizontal twins to
// keep the gesture feeling isotropic.

/// How far a column of pointer travel turns the aim: a ball's width at a
/// metre in about twenty columns, a comfortable sweep at the minimum width.
/// Turning rather than sliding, so the same gesture steers the eye view,
/// where the whole room turns with it.
const AIM_PER_COLUMN: f64 = 0.15 * std::f64::consts::PI / 180.0;
/// Tip travel per column and per row of pointer travel. Halved when the cue
/// panel started drawing the face magnified — the mark moves twice as far per
/// unit of tip there, so the same pointer speed now buys twice the precision
/// instead of twice the movement.
const TIP_PER_COLUMN: f64 = 0.02;
const TIP_PER_ROW: f64 = 0.04;
/// Rows of downward travel that draw the cue from nothing to a full pull.
const PULL_ROWS: f64 = 14.0;

impl PoolDraft {
    /// A draft aimed at something sensible, so a player who fires immediately
    /// plays a real shot rather than a random one.
    pub fn new(state: &DailyPoolState) -> Self {
        let target = state.legal_targets().first().copied();
        // A cue ball that has to be put back is put *somewhere* immediately,
        // so the player arrives holding a ball they can see and move rather
        // than at a table with no cue ball on it and no obvious way to fix
        // that. The spot is only a starting point; the pointer carries it.
        let place = state
            .must_place()
            .then(|| opening_placement(state))
            .flatten();
        Self {
            // A ball in hand is opened *holding*, always. A potted cue ball
            // has to be put back before anything else can happen; a ball in
            // hand from any other foul is something a player would take nine
            // times in ten, and arriving already holding it is how they find
            // out they have it. Right click puts it back down untouched.
            mode: if state.ball_in_hand.is_some() {
                ShotMode::Place
            } else {
                ShotMode::Idle
            },
            azimuth: default_aim(state, place, target),
            picked: target,
            tip: [0.0, 0.0],
            // Two thirds drawn back: a player who arms a band and fires
            // without touching the mouse plays an ordinary shot, not a tap.
            pull: 0.66,
            place,
            called_pocket: None,
            last_pointer: None,
            dragging: false,
            restore: None,
            stroke_origin: None,
            backswing: 0.0,
        }
    }

    /// Stroke speed as a fraction of `MAX_SPEED`: the pull, scaled into the
    /// armed band. Unarmed, it reads as `normal` so the panel has something
    /// honest to show before a band is picked.
    pub fn power(&self) -> f64 {
        let band = self.mode.band().unwrap_or(PowerBand::Normal);
        band.ceiling() * self.pull.clamp(0.0, 1.0)
    }

    /// Where the cue ball will be when the shot is struck: the pending
    /// placement if there is one, otherwise where it lies.
    pub fn cue_ball(&self, state: &DailyPoolState) -> Option<[f64; 2]> {
        self.place.or_else(|| {
            state
                .rack
                .get(CUE)
                .filter(|b| b.potted.is_none())
                .map(|b| b.pos)
        })
    }

    /// Ball positions as the table will be when the shot is struck: the rack,
    /// with a pending ball-in-hand placement folded in. A potted cue ball is
    /// not on the table, so without this the player would be carrying it
    /// invisibly, and the line would have nowhere to start.
    pub fn frames(&self, state: &DailyPoolState) -> Vec<BallFrame> {
        let mut frames: Vec<BallFrame> = state
            .rack
            .balls
            .iter()
            .map(|ball| BallFrame {
                id: ball.id,
                pos: ball.pos,
                potted: ball.potted.is_some(),
            })
            .collect();
        if let Some(at) = self.place
            && let Some(cue) = frames.iter_mut().find(|frame| frame.id == CUE)
        {
            cue.pos = at;
            cue.potted = false;
        }
        frames
    }

    /// The aim, read off the table: what the cue ball meets first, where the
    /// object ball goes, where a miss comes off the rail. `None` only when
    /// there is no cue ball to shoot from.
    pub fn line(&self, state: &DailyPoolState) -> Option<ShotLine> {
        let from = self.cue_ball(state)?;
        let spec = state.spec().ok()?;
        Some(aim::shot_line(
            spec,
            &spec.geometry(),
            &self.frames(state),
            from,
            self.azimuth,
        ))
    }

    /// The ball the aim is on, if the line runs near enough to one.
    pub fn target(&self, state: &DailyPoolState) -> Option<u8> {
        self.line(state).and_then(|line| line.target())
    }

    /// Where the cue ball's centre will be at the moment it touches the ball
    /// it is aimed at: the "ghost ball" every player aims with. `None` when
    /// the line reaches no ball, which is how the board says the aim is off.
    pub fn ghost(&self, state: &DailyPoolState) -> Option<[f64; 2]> {
        self.line(state).and_then(|line| line.ghost())
    }

    /// The move to send. `None` when there is nowhere to shoot from.
    pub fn shot(&self, state: &DailyPoolState) -> Option<Shot> {
        self.cue_ball(state)?;
        Some(Shot {
            place: self.place,
            azimuth: self.azimuth,
            tip: self.tip,
            speed: self.power().clamp(0.05, 1.0) * MAX_SPEED,
            called_pocket: self.called_pocket,
            play_again: false,
        })
    }

    // ── Modes ─────────────────────────────────────────────────────────

    /// Arm a mode, or commit it if it is already the one running.
    ///
    /// Arming snapshots what the mode is about to change so `cancel` can put
    /// it back, and forgets the last pointer position so the next motion event
    /// becomes the new reference — the setting never jumps to wherever the
    /// mouse happened to be left sitting.
    pub fn toggle_mode(&mut self, mode: ShotMode) {
        if self.mode == mode {
            self.commit();
            return;
        }
        // Arming a different mode commits the one running, then takes a fresh
        // snapshot: cancelling the new mode must not roll back the old one.
        self.restore = Some(DraftRestore {
            azimuth: self.azimuth,
            tip: self.tip,
            pull: self.pull,
            place: self.place,
        });
        self.mode = mode;
        self.last_pointer = None;
        self.dragging = false;
        self.stroke_origin = None;
        self.backswing = 0.0;
    }

    /// Keep the adjustment and put the cue down. Returns whether anything was
    /// armed, so the caller can tell a key that landed from one that did not.
    pub fn commit(&mut self) -> bool {
        let was_armed = self.mode != ShotMode::Idle;
        self.mode = ShotMode::Idle;
        self.restore = None;
        self.last_pointer = None;
        self.dragging = false;
        self.stroke_origin = None;
        self.backswing = 0.0;
        was_armed
    }

    /// Put the adjustment back to what it was when the mode was armed, and
    /// put the cue down.
    ///
    /// This is what makes committing mean anything: without a counterpart that
    /// discards, a right-click that merely left the mode would land in exactly
    /// the same place as a left-click that kept it. Restoring all three values
    /// rather than only the armed mode's is deliberate — one snapshot cannot
    /// fall out of step with the mode it belongs to.
    pub fn cancel(&mut self) -> bool {
        let was_armed = self.mode != ShotMode::Idle;
        if let Some(restore) = self.restore.take() {
            self.azimuth = restore.azimuth;
            self.tip = restore.tip;
            self.pull = restore.pull;
            self.place = restore.place;
        }
        self.mode = ShotMode::Idle;
        self.last_pointer = None;
        self.dragging = false;
        self.stroke_origin = None;
        self.backswing = 0.0;
        was_armed
    }

    /// What the other side needs to draw this shot as it is being composed.
    pub fn share(&self) -> PoolAimShare {
        PoolAimShare {
            azimuth: self.azimuth,
            tip: self.tip,
            pull: self.pull,
            mode: self.mode,
            place: self.place,
            called_pocket: self.called_pocket,
        }
    }

    /// The mirror image: someone else's shot, as a draft this board can draw
    /// with the same code that draws its own. Read-only by construction —
    /// `pool_draft_mut` refuses a board that is not yours to act on, so
    /// nothing can steer it and nothing it holds can become a move.
    pub fn watching(share: PoolAimShare) -> Self {
        Self {
            mode: share.mode,
            azimuth: share.azimuth,
            picked: None,
            tip: share.tip,
            pull: share.pull,
            place: share.place,
            called_pocket: share.called_pocket,
            last_pointer: None,
            dragging: false,
            restore: None,
            stroke_origin: None,
            backswing: 0.0,
        }
    }

    /// Zero the armed adjustment without leaving the mode.
    ///
    /// Neutral, not "what it was when this mode was armed": centre-ball and
    /// dead-on are positions a player asks for by name, and reaching them by
    /// walking the pointer back is fiddly on a face a few pixels across. A
    /// stroke has no meaningful neutral (half-drawn is not a thing anyone
    /// wants), so there it means put the cue down. Dead-on is the centre of
    /// the ball the line is on; with no ball on the line there is nothing to
    /// straighten onto and the aim stays.
    pub fn reset(&mut self, state: &DailyPoolState) -> bool {
        match self.mode {
            // Nothing armed: put the whole shot back to square. Reaching for
            // "centre the spin" *after* committing it is the common case:
            // you look at the panel, decide against the english, and there is
            // nothing to re-arm and undo, because you already put the cue
            // down. So an idle right-click clears both adjustments at once.
            ShotMode::Idle => {
                let straightened = self.straighten(state);
                let moved = straightened || self.tip != [0.0, 0.0];
                self.tip = [0.0, 0.0];
                moved
            }
            ShotMode::Aim => self.straighten(state),
            ShotMode::Spin => {
                let moved = self.tip != [0.0, 0.0];
                self.tip = [0.0, 0.0];
                moved
            }
            ShotMode::Place => {
                let was = self.place;
                self.place = opening_placement(state);
                self.place != was
            }
            ShotMode::Stroke(_) => self.cancel(),
        }
    }

    /// Pointer motion, with or without a button held.
    ///
    /// Aim and spin read the motion as a **delta** from the last report, so
    /// arming a mode never yanks the setting to wherever the mouse was left.
    /// The stroke reads it as an **absolute** offset from where the gesture
    /// began, because there the origin means something physical — it is where
    /// the cue ball is. Drawing back to the same place is always the same
    /// power, and pushing back past it is unambiguously a strike.
    pub fn pointer_moved(&mut self, x: u16, y: u16, button_down: bool) -> PointerOutcome {
        if self.mode == ShotMode::Idle {
            self.last_pointer = None;
            return PointerOutcome::Ignored;
        }
        let last = self.last_pointer.replace((x, y));
        let Some((last_x, last_y)) = last else {
            // First report since arming: the reference, not a movement. For a
            // stroke it is also the ball, which the rest of the gesture is
            // measured against.
            self.stroke_origin = Some(y);
            return PointerOutcome::Ignored;
        };
        if (last_x, last_y) == (x, y) {
            return PointerOutcome::Ignored;
        }
        if button_down {
            // **Re-grip.** Holding the button and moving is lifting the mouse
            // off the pad: the reference follows the pointer and the setting
            // does not move. A terminal reports motion only while the pointer
            // is inside the window, so without this a player who runs out of
            // screen mid-aim has nowhere left to go — the physical gesture
            // people already use for that has no other expression here.
            self.dragging = true;
            self.stroke_origin = Some(y);
            return PointerOutcome::Ignored;
        }
        let dx = x as f64 - last_x as f64;
        let dy = y as f64 - last_y as f64;
        match self.mode {
            ShotMode::Idle => PointerOutcome::Ignored,
            ShotMode::Aim => {
                // Sideways turns the cue: right is clockwise on the overview
                // and a turn to the right in the eye view. Up and down mean
                // nothing, so a hand that drifts while sweeping does not
                // change the shot. Running out of screen is what the re-grip
                // above is for.
                self.turn(dx * AIM_PER_COLUMN);
                PointerOutcome::Changed
            }
            ShotMode::Spin => {
                // Screen rows grow downward while the tip offset grows up the
                // face, hence the negated `dy`: get this backwards and the
                // panel shows draw while the ball is struck with follow.
                self.nudge_tip(dx * TIP_PER_COLUMN, -dy * TIP_PER_ROW);
                PointerOutcome::Changed
            }
            ShotMode::Place => {
                // Placement needs the table geometry to turn a cell into a
                // spot on the cloth, and only the caller has it — motion over
                // the table arrives through `pool_hover_table` instead. Motion
                // anywhere else genuinely means nothing here.
                PointerOutcome::Ignored
            }
            ShotMode::Stroke(_) => self.stroke_to(y),
        }
    }

    /// The stroke itself: draw down, then push back up through the ball.
    ///
    /// Modelled on the real gesture rather than on a button, which is what the
    /// press-drag-release version got wrong — a stroke is one continuous
    /// motion, and the moment of contact is when the cue passes the ball, not
    /// when a finger happens to lift.
    fn stroke_to(&mut self, y: u16) -> PointerOutcome {
        let origin = *self.stroke_origin.get_or_insert(y);
        let delta = y as f64 - origin as f64;
        if delta >= 0.0 {
            // Behind the ball: drawing back.
            self.pull = (delta / PULL_ROWS).clamp(0.0, 1.0);
            self.backswing = self.backswing.max(self.pull);
            return PointerOutcome::Changed;
        }
        // Past the ball. Only a stroke if there was a backswing behind it —
        // otherwise nudging the mouse upward on an armed cue fires it.
        if self.backswing < MIN_BACKSWING {
            self.pull = 0.0;
            return PointerOutcome::Changed;
        }
        // The backswing is the power, not wherever the pointer ended up.
        self.pull = self.backswing;
        PointerOutcome::Strike
    }

    /// A button went down. Only marks the reference so the gesture measures
    /// from here; the press itself changes nothing, because until the button
    /// comes up there is no telling a click from the start of a re-grip.
    pub fn pointer_pressed(&mut self, x: u16, y: u16) {
        self.last_pointer = Some((x, y));
        self.dragging = false;
        if self.mode.band().is_some() {
            self.stroke_origin.get_or_insert(y);
        }
    }

    /// A button came up. Reports whether this was a **click** — a press with
    /// no travel behind it — which is what carries the meaning; a press that
    /// moved was a re-grip and must not also commit whatever was armed.
    ///
    /// The stroke does not ride on the release either way: pushing the cue
    /// forward through the ball is what fires it.
    pub fn pointer_released(&mut self) -> bool {
        let clicked = !self.dragging;
        self.dragging = false;
        clicked
    }

    // ── Adjustments ───────────────────────────────────────────────────

    /// One press of an arrow (or wasd): steer whatever is armed.
    ///
    /// Arrows do whatever the armed mode does, so the whole shot is reachable
    /// without ever touching the mouse. Unarmed, they cycle the target, which
    /// is the only thing there is to walk on an idle board. The steps live
    /// here beside the pointer rates so the two input paths are tuned as one.
    pub fn key_step(&mut self, state: &DailyPoolState, dx: isize, dy: isize) {
        match self.mode {
            ShotMode::Idle => self.cycle_target(state, dx.signum()),
            // Left and right turn the cue, like `h` and `l`. Up and down do
            // nothing here: there is only one axis to an aim.
            ShotMode::Aim => self.turn(dx as f64 * AIM_STEP),
            ShotMode::Spin => self.nudge_tip(dx as f64 * TIP_STEP, dy as f64 * TIP_STEP),
            ShotMode::Place => self.nudge_placement(state, dx, dy),
            ShotMode::Stroke(_) => self.nudge_pull(-dy as f64 * PULL_STEP),
        }
    }

    /// `h`/`l` turn the cue a degree, `H`/`L` a tenth of one.
    pub fn key_aim(&mut self, delta: isize, fine: bool) {
        let step = if fine { AIM_FINE_STEP } else { AIM_STEP };
        self.turn(delta as f64 * step);
    }

    /// Turn the cue by `delta` radians, positive clockwise on the overview.
    pub fn turn(&mut self, delta: f64) {
        self.azimuth = (self.azimuth + delta).rem_euclid(std::f64::consts::TAU);
    }

    /// Point dead at the centre of the ball the line is on. Reports whether
    /// the aim moved; with no ball on the line there is nothing to do.
    fn straighten(&mut self, state: &DailyPoolState) -> bool {
        let Some(id) = self.target(state) else {
            return false;
        };
        let before = self.azimuth;
        self.aim_at_ball(state, id);
        self.azimuth != before
    }

    /// `{` / `}`: step through the pots on offer for the ball the aim is on,
    /// easiest first. Reports whether there was one to step to.
    ///
    /// The ball is the sighted one when it is legal to hit, and otherwise the
    /// first legal target, so the key always answers about a ball the shot
    /// could play. Which pot is "current" is read back off the bearing rather
    /// than remembered: an aim that has since been turned by hand is not on
    /// any of them, and the next press starts from the easiest again.
    pub fn cycle_pot(&mut self, state: &DailyPoolState, delta: isize) -> bool {
        let legal = state.legal_targets();
        let target = match self.target(state) {
            Some(id) if legal.contains(&id) => id,
            Some(_) | None => match legal.first() {
                Some(id) => *id,
                None => return false,
            },
        };
        let Some(from) = self.cue_ball(state) else {
            return false;
        };
        let Ok(spec) = state.spec() else {
            return false;
        };
        let pots = aim::pot_lines(spec, &spec.geometry(), &self.frames(state), from, target);
        if pots.is_empty() {
            return false;
        }
        let current = pots
            .iter()
            .position(|pot| (pot.azimuth - self.azimuth).abs() < 1e-9);
        let next = match current {
            Some(index) => (index as isize + delta).rem_euclid(pots.len() as isize) as usize,
            None if delta < 0 => pots.len() - 1,
            None => 0,
        };
        self.azimuth = pots[next].azimuth;
        true
    }

    /// Step through the balls this player may legally hit first.
    ///
    /// Cycling rather than free-roaming a cursor: on a table drawn three
    /// pixels to the ball, hunting one down with arrow keys is worse in every
    /// way than naming it, and the mouse still picks any point on the cloth
    /// for a cushion target.
    pub fn cycle_target(&mut self, state: &DailyPoolState, delta: isize) {
        let targets = state.legal_targets();
        if targets.is_empty() {
            return;
        }
        // From the picked ball while it is still on, otherwise from the ball
        // the line happens to be on.
        let current = self
            .picked
            .filter(|id| targets.contains(id))
            .or_else(|| self.target(state))
            .and_then(|id| targets.iter().position(|t| *t == id));
        let next = match current {
            Some(index) => (index as isize + delta).rem_euclid(targets.len() as isize) as usize,
            // No ball picked (a cushion target): step onto the ends of the list.
            None if delta < 0 => targets.len() - 1,
            None => 0,
        };
        self.aim_at_ball(state, targets[next]);
    }

    /// Jump to the ball that is most obviously "on": the lowest-numbered
    /// legal target. In nine-ball that is the only one there is; in eight-ball
    /// it is the lowest of your group, which is where most players look first.
    pub fn next_in_line(&mut self, state: &DailyPoolState) {
        if let Some(id) = state.legal_targets().first().copied() {
            self.aim_at_ball(state, id);
        }
    }

    /// Point dead at the centre of ball `id`, and remember it as the pick.
    pub fn aim_at_ball(&mut self, state: &DailyPoolState, id: u8) {
        let Some(ball) = state.rack.get(id).filter(|b| b.potted.is_none()) else {
            return;
        };
        self.picked = Some(id);
        self.aim_at_point(state, ball.pos);
    }

    /// Set the cue ball down at `at`, or as near as the rules allow.
    ///
    /// Snapping rather than rejecting: the table view is an overview where a
    /// ball is a few pixels, so an exact click is not something a player can
    /// be asked for. `free_spot` walks the same rule a referee would — on the
    /// spot, or as near behind it as the other balls allow.
    pub fn put_down(&mut self, state: &DailyPoolState, at: [f64; 2]) -> bool {
        let Ok(spec) = state.spec() else {
            return false;
        };
        let zone = state
            .ball_in_hand
            .unwrap_or(pool_rules::BallInHand::Anywhere);
        let geom = spec.geometry();
        let Some(spot) = pool_rules::free_spot(spec, &geom, &state.rack, at, zone) else {
            return false;
        };
        self.place = Some(spot);
        true
    }

    /// A click on the cloth while holding the ball: set it down, and put the
    /// cue down with it.
    ///
    /// One gesture rather than two, because left click means "there, done"
    /// everywhere else on this board and placement should not be the one thing
    /// that needs a key to finish. Right click still cancels, so the pair
    /// stays symmetric.
    pub fn place_and_commit(&mut self, state: &DailyPoolState, at: [f64; 2]) -> bool {
        if !self.put_down(state, at) {
            return false;
        }
        self.commit();
        true
    }

    /// Walk the held cue ball a step at a time, for the keyboard path. Falls
    /// back to the kitchen or the break spot when nothing has been set down
    /// yet, so the arrows always have something to move.
    pub fn nudge_placement(&mut self, state: &DailyPoolState, dx: isize, dy: isize) {
        let Ok(spec) = state.spec() else {
            return;
        };
        let from = self.place.unwrap_or_else(|| rack::break_spot(spec));
        let step = spec.ball_radius;
        // Screen "up" is toward the far rail, which is +y in table space.
        let to = [from[0] + dx as f64 * step, from[1] + dy as f64 * step];
        self.put_down(state, to);
    }

    /// Name the pocket nearest `at`, when the shot is one that has to call one.
    ///
    /// Nothing wrote `called_pocket` at all until this existed, and the server
    /// *refuses* an uncalled shot on the eight — so eight-ball could not be
    /// finished: the board reached a position where every shot was rejected
    /// and there was no gesture that would fix it. The same shape of bug as
    /// the ball-in-hand dead board, and found the same way.
    ///
    /// Generous about the distance, like every other pointer target on the
    /// table: a pocket is a few pixels on an overview, and "the corner one" is
    /// what the player means whatever pixel they hit.
    pub fn call_pocket_at(&mut self, state: &DailyPoolState, at: [f64; 2]) -> bool {
        if !state.requires_call() {
            return false;
        }
        let Ok(spec) = state.spec() else {
            return false;
        };
        let called = spec
            .geometry()
            .pockets
            .iter()
            .enumerate()
            .map(|(index, pocket)| {
                let (dx, dy) = (pocket.center[0] - at[0], pocket.center[1] - at[1]);
                (index as u8, dx.hypot(dy))
            })
            .filter(|(_, distance)| *distance <= spec.corner_mouth * 1.5)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match called {
            Some((index, _)) => {
                self.called_pocket = Some(index);
                true
            }
            None => false,
        }
    }

    /// Point at a bare spot on the cloth: a cushion, or a spot to send the
    /// cue ball to. A spot on top of the cue ball has no direction and leaves
    /// the aim where it was.
    pub fn aim_at_point(&mut self, state: &DailyPoolState, at: [f64; 2]) {
        let Some(cue) = self.cue_ball(state) else {
            return;
        };
        let (dx, dy) = (at[0] - cue[0], at[1] - cue[1]);
        if dx.hypot(dy) < 1e-9 {
            return;
        }
        self.azimuth = dy.atan2(dx).rem_euclid(std::f64::consts::TAU);
    }

    /// Move the tip across the cue ball's face, staying inside the miscue
    /// limit — the board will not let a player set up a shot the server would
    /// then refuse.
    pub fn nudge_tip(&mut self, dx: f64, dy: f64) {
        let tip = [self.tip[0] + dx, self.tip[1] + dy];
        let len = tip[0].hypot(tip[1]);
        self.tip = if len > MISCUE_LIMIT {
            [tip[0] / len * MISCUE_LIMIT, tip[1] / len * MISCUE_LIMIT]
        } else {
            tip
        };
    }

    /// Draw the cue back (positive) or push it in (negative), within the band.
    pub fn nudge_pull(&mut self, delta: f64) {
        self.pull = (self.pull + delta).clamp(0.0, 1.0);
    }
}

/// Where a cue ball that must be replaced starts out: the break spot, or the
/// nearest legal spot to it, which is also where a player would put it by hand
/// after a scratch. `None` only if the table is somehow too crowded to take
/// it, in which case the board says so rather than inventing a position.
fn opening_placement(state: &DailyPoolState) -> Option<[f64; 2]> {
    let spec = state.spec().ok()?;
    let geom = spec.geometry();
    let zone = state
        .ball_in_hand
        .unwrap_or(pool_rules::BallInHand::Anywhere);
    // Start from a spot that is *in* the zone being offered. The break spot is
    // a quarter of the way down the table, which is nowhere near the D, and
    // `free_spot` walks outward from where it is pointed rather than hunting
    // the table for somewhere legal.
    let from = match zone {
        pool_rules::BallInHand::TheD => rack::d_spot(spec),
        pool_rules::BallInHand::Anywhere | pool_rules::BallInHand::Kitchen => {
            rack::break_spot(spec)
        }
    };
    pool_rules::free_spot(spec, &geom, &state.rack, from, zone)
}

/// Whether a shot worth broadcasting has changed enough to broadcast now.
///
/// Three rules, in order: nothing to say if nothing moved; say it immediately
/// if the *mode* changed, because arming the stroke is the update whose timing
/// is the information; otherwise wait out the interval, since pointer motion
/// arrives per terminal cell and a sweep is dozens of reports a second.
pub(crate) fn should_share_aim(
    last: Option<PoolAimShare>,
    last_at: Option<Instant>,
    next: PoolAimShare,
) -> bool {
    match last {
        Some(previous) if previous == next => false,
        Some(previous) if previous.mode != next.mode => true,
        Some(_) => last_at.is_none_or(|at| at.elapsed() >= AIM_SHARE_INTERVAL),
        None => true,
    }
}

/// Point the opening aim at the first legal target, or down the table when
/// there is nothing to aim at yet.
fn default_aim(state: &DailyPoolState, place: Option<[f64; 2]>, target: Option<u8>) -> f64 {
    let from = place.or_else(|| {
        state
            .rack
            .get(CUE)
            .filter(|ball| ball.potted.is_none())
            .map(|ball| ball.pos)
    });
    let at = target
        .and_then(|id| state.rack.get(id))
        .filter(|ball| ball.potted.is_none())
        .map(|ball| ball.pos)
        .or_else(|| state.spec().ok().map(rack::foot_spot));
    match (from, at) {
        (Some(from), Some(at)) => (at[1] - from[1])
            .atan2(at[0] - from[0])
            .rem_euclid(std::f64::consts::TAU),
        _ => 0.0,
    }
}
