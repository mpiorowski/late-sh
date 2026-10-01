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
//! `Shot` (`PoolDraft::shot`) and the seam sends it.
//!
//! The last section of the file is the other half of that rule. Pool is the
//! only daily game that animates, so it is the only one that needs a physics
//! worker, a playback queue and a reason to sit on news — and all of that
//! reads and writes `DailyBoardState`, which is shared. It lives here anyway,
//! because **every field it touches is a pool field**: the rule is about who
//! owns the code, not which struct the bytes sit in, and `state.rs` keeps arms
//! only. That is why this file imports from `state.rs` while `state.rs`
//! imports from it; the cycle is fine and the alternative is a thousand lines
//! of pool in the shared file, which is what the rule exists to prevent.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::app::common::primitives::Banner;
use crate::app::games::pool_core::{
    aim::{self, ShotLine},
    ball::CUE,
    cue::{MAX_SPEED, MISCUE_LIMIT, PowerBand, ShotMode, Strike},
    cue_ui::PanelHit,
    rack, rules as pool_rules,
    shot::{BallFrame, RackState, Shot, Timeline},
    sim,
    table::{Geometry, TableSpec},
};

use super::pool::{DailyPoolState, PoolAimShare, ReplayError};
use super::state::{DailyBoardState, DailyMatchDetail};

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
    /// Shots still to play after the current one. Only a replay fills it: a
    /// fresh shot is one timeline, a replayed visit is several, and the queue
    /// is what makes the second case the first case repeated.
    pub queue: Vec<Timeline>,
    /// This playback is a replay the player asked for rather than a shot
    /// arriving. Kept because the two want opposite things from the board: a
    /// fresh shot holds the result back until it has played out, where a replay
    /// is watched with the result already on screen — and because pressing the
    /// key again has to put the board back.
    pub replaying: bool,
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
    /// A board freshly built from a row: the rack as it stands, a draft aimed
    /// at something sensible, and nothing in flight.
    ///
    /// Here rather than in the caller's `match` so the shared detail builder
    /// stays one arm per game, however many fields this grows.
    pub fn new(state: DailyPoolState) -> Self {
        Self {
            draft: PoolDraft::new(&state),
            state,
            shot_in_flight: false,
            playback: None,
            queue: Vec::new(),
            replaying: false,
            watching: None,
        }
    }

    /// Nothing can be adjusted and nothing may be sent: a shot is on the wire,
    /// being re-simulated, or rolling, or a replay is. The one question the
    /// input layer and the renderer both ask, so they cannot answer it
    /// differently.
    ///
    /// `shot_pending` is `DailyBoardState::pool_shot_pending`, taken as an
    /// argument rather than read off the board because every caller is already
    /// holding the board's detail borrowed when it asks.
    pub fn is_busy(&self, shot_pending: bool) -> bool {
        self.shot_in_flight || self.playback.is_some() || self.replaying || shot_pending
    }

    /// A fresh shot has not finished showing, so what it did is not the
    /// board's to say yet: it is on the wire, waiting on its animation, or
    /// still rolling. The animation is the long part of that, seconds where
    /// the other two are a tick apiece.
    ///
    /// A replay does not count. It is watched with the result already known,
    /// so hiding the result for it would only take the panel away.
    pub fn withholds_result(&self, shot_pending: bool) -> bool {
        self.shot_in_flight || shot_pending || (self.playback.is_some() && !self.replaying)
    }

    /// Take over what the detail being replaced was in the middle of showing.
    ///
    /// The playback because a reload must not delete a shot that is still
    /// rolling, and the watched aim because the opponent is not going to
    /// re-broadcast it just because this board reloaded.
    pub fn adopt(&mut self, previous: &mut PoolDetail) {
        self.playback = previous.playback.take();
        self.queue = std::mem::take(&mut previous.queue);
        self.replaying = std::mem::take(&mut previous.replaying);
        self.watching = previous.watching.take();
    }

    /// The rack as it stood before the shot that is about to play, for the
    /// gap between a shot landing and its animation being ready.
    ///
    /// Without it the board shows the *settled* rack for the tick or two the
    /// re-simulation takes, and then rewinds and plays the shot — the balls
    /// snap to where they end up and then jump back, which reads as the board
    /// glitching. `None` before the break, where there is no previous rack.
    pub fn frames_before_shot(&self) -> Option<Vec<BallFrame>> {
        Some(
            self.state
                .prev_rack
                .as_ref()?
                .balls
                .iter()
                .map(|ball| BallFrame {
                    id: ball.id,
                    pos: ball.pos,
                    potted: ball.potted.is_some(),
                })
                .collect(),
        )
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

/// How far one cell of pointer travel moves whatever is armed.
///
/// A terminal cell is a coarse unit to aim in — one column is about a ball's
/// width at a metre and a half — so the rates above are a compromise between
/// sweeping the table and picking a thin cut, and neither end is well served.
/// Holding a modifier while the pointer moves picks a gear instead, which is
/// the same trade `h`/`l` against `H`/`L` makes on the keyboard.
///
/// **Ctrl is the reliable one.** Shift is not: xterm and most of its
/// descendants reserve Shift+mouse for the terminal's *own* selection, and
/// swallow the report rather than sending it — so on those terminals a
/// Shift-held sweep simply arrives as an ordinary one. Alt is read as coarse
/// as well for exactly that reason: it is the fallback that works where Shift
/// is eaten, and it costs nothing where Shift is not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AimGear {
    /// Ctrl: a tenth of the step, for the last fraction of a degree.
    Fine,
    #[default]
    Normal,
    /// Shift or Alt: four times the step, to cross the table in one sweep.
    Coarse,
}

impl AimGear {
    /// Ctrl wins a Ctrl+Shift sweep: asking for both is asking for precision
    /// with a hand that has run out of desk, and the re-grip covers the rest.
    pub fn of(fine: bool, coarse: bool) -> Self {
        match (fine, coarse) {
            (true, _) => Self::Fine,
            (false, true) => Self::Coarse,
            (false, false) => Self::Normal,
        }
    }

    pub fn scale(self) -> f64 {
        match self {
            Self::Fine => 0.1,
            Self::Normal => 1.0,
            Self::Coarse => 4.0,
        }
    }
}

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
            // out they have it. Right click puts it back where it lay.
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
            self.tip[0],
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
            // Back where it lay: a potted cue ball has only its opening spot
            // to go back to, one still on the table goes back to where it
            // is, which is no placement at all.
            ShotMode::Place => {
                let was = self.place;
                let back = if state.must_place() {
                    opening_placement(state)
                } else {
                    None
                };
                self.carry(state, back);
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
    pub fn pointer_moved(
        &mut self,
        x: u16,
        y: u16,
        button_down: bool,
        gear: AimGear,
    ) -> PointerOutcome {
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
        // The gear scales the *travel*, not the rate, so it applies to
        // whichever delta-steered mode is armed without either of them having
        // to know it exists.
        let dx = (x as f64 - last_x as f64) * gear.scale();
        let dy = (y as f64 - last_y as f64) * gear.scale();
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
        self.carry(state, Some(spot));
        true
    }

    /// Move the held cue ball to `spot` (`None` is where it lies) and keep
    /// the line on the ball it was on.
    ///
    /// Ball in hand is played as pick the ball, then walk the cue ball until
    /// the shot is straight. A bearing that stayed put while the ball moved
    /// swung the line off onto a rail at every step, so the player was
    /// aiming again instead of placing. The line is re-laid dead at the ball
    /// it was sighted on; a line on a bare spot of cloth is a bearing and
    /// stays one.
    fn carry(&mut self, state: &DailyPoolState, spot: Option<[f64; 2]>) {
        let followed = self.target(state);
        self.place = spot;
        if let Some(id) = followed
            && let Some(ball) = state.rack.get(id).filter(|b| b.potted.is_none())
        {
            self.aim_at_point(state, ball.pos);
        }
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
    /// **The click has to land on the pocket**, which is a hole straddling the
    /// table's edge and not a disc of cloth in front of one. Two tests, and
    /// both are needed: within a mouth's width of it (so a click by one corner
    /// cannot name another), and at or past its mouth chord give or take a
    /// ball's radius (`Geometry::pocket_depth`, the same line that decides a
    /// ball has dropped) less three half-radii — which is the disc the pocket
    /// is drawn as, so what the eye reads as the hole is what the pointer can
    /// name, and a click that lands just short of the mouth still counts.
    ///
    /// The depth test is the one that matters. A plain radius of one and a
    /// half mouths is a tenth of the table's length around each of six
    /// pockets, and while naming a pocket outranked everything the board would
    /// not take a target or a direction anywhere in it — which, once the shot
    /// has to be called, is most of the cloth the eight is likely to be near.
    /// Cloth in front of a pocket is somewhere to aim; the hole is not.
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
            // On the hole, not on the cloth in front of it. The slack is
            // three half-radii, which reaches the inner edge of the disc the
            // pocket is *drawn* as — so what the eye reads as the hole is what
            // the pointer can name, and no more.
            .filter(|(_, pocket)| Geometry::pocket_depth(pocket, at) >= -spec.ball_radius * 1.5)
            .map(|(index, pocket)| {
                let (dx, dy) = (pocket.center[0] - at[0], pocket.center[1] - at[1]);
                (index as u8, dx.hypot(dy))
            })
            .filter(|(_, distance)| *distance <= spec.corner_mouth)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match called {
            Some((index, _)) => {
                self.called_pocket = Some(index);
                true
            }
            None => false,
        }
    }

    /// What a left click on the cloth means, in the order a player reads the
    /// table: **that ball, that pocket, that spot**.
    ///
    /// Holding the cue ball, the click sets it down instead — that is the whole
    /// of the mode, so it also ends it. Left click is "there, done" everywhere
    /// else on this board and placement should not be the one thing that needs
    /// a key to finish. With any *other* mode running the click is that mode's
    /// commit, and re-targeting mid-aim would throw away the very adjustment
    /// the click is there to keep.
    ///
    /// Naming a pocket used to come first, on the grounds that down to the
    /// eight there is only one legal ball left and the pockets are the only
    /// thing worth pointing at. That is true of the pockets and false of the
    /// rest of the cloth: the pocket reach covered a tenth of the table's
    /// length around each of six of them, so once the shot had to be called the
    /// board would not take a target or a direction anywhere near a rail —
    /// which is where the eight usually is by then. Ordered this way each
    /// gesture keeps its own meaning and none of them is unreachable.
    pub fn click_table(&mut self, state: &DailyPoolState, at: [f64; 2]) -> bool {
        if self.mode == ShotMode::Place {
            return self.place_and_commit(state, at);
        }
        if self.mode != ShotMode::Idle {
            return false;
        }
        let Ok(spec) = state.spec() else {
            return false;
        };
        // Generous: the drawn ball is bigger than life, so the click target
        // should be too, or the picture and the pointer disagree.
        let reach = spec.ball_radius * 3.0;
        let hit = state
            .legal_targets()
            .into_iter()
            .filter_map(|id| state.rack.get(id).map(|ball| (id, ball.pos)))
            .map(|(id, pos)| (id, (pos[0] - at[0]).hypot(pos[1] - at[1])))
            .filter(|(_, distance)| *distance <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((id, _)) = hit {
            self.aim_at_ball(state, id);
            return true;
        }
        // A click on a pocket, for the shot that has to name one. Refused
        // everywhere else, so it costs the other games nothing.
        if self.call_pocket_at(state, at) {
            return true;
        }
        // Bare cloth: a cushion to play off, or a spot to send the cue ball to.
        self.aim_at_point(state, at);
        true
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

// ── The board's pool seam ─────────────────────────────────────────────
//
// Pool is the only daily game that animates, so it is the only one that needs
// a worker, a queue and a reason to sit on news. All of that reads and writes
// `DailyBoardState`, which is shared — but every field it touches is a pool
// field, so the code is pool's and `state.rs` keeps arms only. Same rule as
// the rest of this file, one struct further out.

/// Start playing back any shot this session has not shown yet.
///
/// Both sides run through here on reload, which is why there is only one code
/// path: your own shot animates when the canonical row comes back, and so does
/// the opponent's. Simulating locally the moment you fire would be faster by a
/// round trip and would put a second copy of the physics in the loop, which is
/// exactly what the server-as-referee split exists to avoid.
///
/// **The re-simulation does not run on the tick.** Gathering the inputs is
/// local memory; running the shot is thousands of integration steps, so it
/// goes to a blocking thread and comes back through `poll_pool_timeline`.
pub(super) fn start_pool_playback(board: &mut DailyBoardState) {
    let Some(pool) = board.detail.as_mut().and_then(DailyMatchDetail::pool_mut) else {
        return;
    };
    let played = pool.state.move_count();
    // The canonical row is back, so whatever was in flight has landed.
    if board.pool_animated.is_some_and(|seen| played > seen) {
        pool.shot_in_flight = false;
    }
    match board.pool_animated {
        // First load: take the history as already seen. Opening a match should
        // show you the table as it stands, not replay the shot that happened
        // before you arrived.
        None => board.pool_animated = Some(played),
        Some(seen) if played > seen => {
            board.pool_animated = Some(played);
            // A newer shot landing first simply replaces the receiver and the
            // older animation is dropped, which is the same thing the board
            // would do anyway.
            let Some((spec, start, strike)) = pool.state.last_shot_sim() else {
                return;
            };
            pool.replaying = false;
            pool.queue.clear();
            board.timeline_rx = Some(simulate_off_tick(spec, start, strike));
            board.pool_shot_pending = true;
        }
        Some(_) => {}
    }
}

/// Collect shots that finished re-simulating and start the first playing.
/// Returns whether anything changed, like the other tick drains.
pub(super) fn poll_pool_timeline(board: &mut DailyBoardState) -> bool {
    let Some(rx) = &mut board.timeline_rx else {
        return false;
    };
    let mut timelines = match rx.try_recv() {
        Ok(timelines) => timelines,
        Err(oneshot::error::TryRecvError::Empty) => return false,
        // The worker is gone, so no animation is coming. The board still shows
        // the settled rack, which is the truth either way, and it has to let
        // go of everything that was waiting on the animation: the held result,
        // and a replay that would otherwise keep the board until `r` again.
        Err(oneshot::error::TryRecvError::Closed) => {
            tracing::error!(
                match_id = %board.match_id,
                "pool physics worker went away before sending a timeline"
            );
            board.timeline_rx = None;
            board.pool_shot_pending = false;
            if let Some(pool) = board.detail.as_mut().and_then(DailyMatchDetail::pool_mut) {
                pool.replaying = false;
            }
            return true;
        }
    };
    board.timeline_rx = None;
    board.pool_shot_pending = false;
    let Some(pool) = board.detail.as_mut().and_then(DailyMatchDetail::pool_mut) else {
        return false;
    };
    if timelines.is_empty() {
        pool.replaying = false;
        return true;
    }
    // Reversed once, so the queue pops off the end in shot order.
    timelines.reverse();
    let first = timelines.pop().expect("not empty");
    pool.queue = timelines;
    pool.playback = Some(PoolPlayback::new(first));
    true
}

/// Retire a finished playback. Returns whether the board is animating, so the
/// render loop keeps repainting while it is.
pub(super) fn drive_pool_playback(board: &mut DailyBoardState) -> bool {
    let Some(pool) = board.detail.as_mut().and_then(DailyMatchDetail::pool_mut) else {
        return false;
    };
    match &pool.playback {
        Some(playback) if playback.finished() => {
            // A replay of a whole visit runs straight into the next shot, which
            // is how a visit is watched: one break, not four clips.
            pool.playback = pool.queue.pop().map(PoolPlayback::new);
            pool.replaying = pool.replaying && pool.playback.is_some();
            // The last frame differs from the settled rack, so the swap back is
            // itself a repaint.
            true
        }
        Some(_) => true,
        None => false,
    }
}

/// Whether the open board is mid-shot. Drives the render loop's hot tick.
///
/// The wait for a re-simulation counts: the board is holding the pre-shot rack
/// for it and wants the tick that collects it promptly.
pub(super) fn pool_is_animating(board: &DailyBoardState) -> bool {
    board.timeline_rx.is_some()
        || board
            .detail
            .as_ref()
            .and_then(DailyMatchDetail::pool)
            .is_some_and(|pool| pool.playback.is_some())
}

/// One shot simulated on a blocking thread, for `poll_pool_timeline` to
/// collect. The tick path runs no physics.
fn simulate_off_tick(
    spec: &'static TableSpec,
    start: RackState,
    strike: Strike,
) -> oneshot::Receiver<Vec<Timeline>> {
    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let geom = spec.geometry();
        let _ = tx.send(vec![sim::simulate(spec, &geom, &start, &strike).timeline]);
    });
    rx
}

/// The visit from shot `from` onward, replayed from the opening rack on a
/// blocking thread.
///
/// Every way a history fails to replay ends the same for the board (nothing
/// plays) and is said here, since nobody upstream will.
fn replay_off_tick(
    match_id: Uuid,
    state: DailyPoolState,
    from: usize,
) -> oneshot::Receiver<Vec<Timeline>> {
    let (tx, rx) = oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let timelines = match state.replay(from) {
            Ok(timelines) => timelines,
            Err(ReplayError::UnknownTable) => {
                tracing::warn!(%match_id, "pool replay dropped: unknown table");
                Vec::new()
            }
            Err(ReplayError::Refused { shot, reason }) => {
                tracing::warn!(
                    %match_id,
                    shot,
                    reason,
                    "pool replay dropped: the rules refuse a stored shot"
                );
                Vec::new()
            }
            Err(ReplayError::Diverged) => {
                tracing::warn!(
                    %match_id,
                    "pool replay dropped: the history ends on a different rack than the stored one"
                );
                Vec::new()
            }
        };
        let _ = tx.send(timelines);
    });
    rx
}

/// How much of the match a replay shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReplaySpan {
    /// `r`: the shot just played.
    LastShot,
    /// `R`: every shot of the visit it belongs to.
    LastVisit,
}

/// `r` / `R`: watch the last shot again, or the whole of the last visit.
///
/// The two take different roads. The last shot is one simulation off the rack
/// it was played on, which the state stores, so it owes nothing to the
/// history before it. A visit has to be played forward from the opening rack
/// (`DailyPoolState::replay`), which judges the whole match again and can
/// refuse. Both run on a blocking thread, for the same reason a fresh shot's
/// simulation does: the tick path runs no physics.
///
/// Pressing it again while a replay is rolling puts the board back, so the key
/// is its own way out.
///
/// Open to whoever is looking: both players, a spectator, a finished match.
/// Nothing here can become a move, and the shot it shows has already been
/// played.
pub(crate) fn start_pool_replay(board: &mut DailyBoardState, span: ReplaySpan) {
    let match_id = board.match_id;
    let Some(pool) = board.detail.as_mut().and_then(DailyMatchDetail::pool_mut) else {
        return;
    };
    if pool.replaying {
        pool.playback = None;
        pool.queue.clear();
        pool.replaying = false;
        board.timeline_rx = None;
        board.pool_shot_pending = false;
        return;
    }
    let rx = match span {
        ReplaySpan::LastShot => {
            let Some((spec, start, strike)) = pool.state.last_shot_sim() else {
                return;
            };
            simulate_off_tick(spec, start, strike)
        }
        ReplaySpan::LastVisit => {
            let Some(from) = pool.state.visit_start() else {
                return;
            };
            replay_off_tick(match_id, pool.state.clone(), from)
        }
    };
    pool.playback = None;
    pool.queue.clear();
    pool.replaying = true;
    board.timeline_rx = Some(rx);
    // A replay is asked for, so there is no result to hold back: the board is
    // already showing it.
    board.pool_shot_pending = false;
}

/// News of a finish this session has been told about but is not yet showing,
/// because the pool board it happened on is still playing the shot that ended
/// it.
///
/// Held on `DailyState` rather than on the board, so closing the board
/// releases it rather than losing it.
pub(super) struct PoolFinishHold {
    pub banner: Banner,
    pub own_win: bool,
    pub own_loss: bool,
    /// When it was held. The hold is released by the shot finishing; this is
    /// the backstop for the case where it never does (a reload that errored, a
    /// physics worker that went away), because news that never arrives is
    /// worse than news that arrives late.
    at: Instant,
}

impl PoolFinishHold {
    pub(super) fn new(banner: Banner, own_win: bool, own_loss: bool, at: Instant) -> Self {
        Self {
            banner,
            own_win,
            own_loss,
            at,
        }
    }

    /// Whether the hold is up at `now`: the board has finished showing the
    /// shot, or the backstop has expired.
    pub(super) fn released(&self, board: Option<&DailyBoardState>, now: Instant) -> bool {
        now.saturating_duration_since(self.at) >= FINISH_HOLD_MAX || !pool_board_is_rolling(board)
    }
}

/// Longest a finish is held waiting for a shot to play out. A shot is seconds;
/// this is the belt to that braces.
pub(super) const FINISH_HOLD_MAX: Duration = Duration::from_secs(20);

/// Whether news that `match_id` has finished should wait.
///
/// It should when this is the open board, the board is pool, and the shot that
/// ended the match has not been watched yet — which at the moment the event
/// lands means the reload carrying it is still in flight. Announcing through
/// the animation is announcing before the table has got there, and at a real
/// table nobody tells you the rack is over while the balls are still moving.
pub(super) fn pool_defers_finish(board: Option<&DailyBoardState>, match_id: Uuid) -> bool {
    board.is_some_and(|board| {
        board.match_id == match_id
            && board
                .detail
                .as_ref()
                .and_then(DailyMatchDetail::pool)
                .is_some()
            && pool_board_is_rolling(Some(board))
    })
}

/// The open board is a pool board with a shot somewhere between the wire and
/// the last frame of its animation.
fn pool_board_is_rolling(board: Option<&DailyBoardState>) -> bool {
    board.is_some_and(|board| {
        board.reloading()
            || board.timeline_rx.is_some()
            || board
                .detail
                .as_ref()
                .and_then(DailyMatchDetail::pool)
                .is_some_and(|pool| pool.shot_in_flight || pool.playback.is_some())
    })
}
