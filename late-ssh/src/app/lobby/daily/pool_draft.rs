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
    /// watching an opponent pick a ball and walk the aim across it is the only
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

/// The shot under construction, and what the pointer is currently wired to.
///
/// Kept out of `Shot` because these are aiming aids, not part of the move: the
/// target is what the aim is measured *against*, and the server only ever
/// receives the resulting azimuth.
///
/// **Nothing here is sequenced.** Target, spin, aim and stroke are all live at
/// once and a player may strike at any moment; `mode` only says which of them
/// the mouse is steering. The reason it is a mode at all rather than a held
/// key is that a terminal has no key-up event to observe (see `ShotMode`).
pub struct PoolDraft {
    pub mode: ShotMode,
    /// The ball being shot at, or `None` when the target is a cushion point.
    pub target: Option<u8>,
    /// The spot being aimed at, in table coordinates. The target ball's centre
    /// while a ball is picked; an arbitrary point when it is a cushion.
    pub aim_at: [f64; 2],
    /// How far the aim line passes from `aim_at`, in ball radii, measured
    /// across the shot line. Zero is dead centre and past ±2 the cue ball
    /// misses the ball entirely.
    pub aim_offset: f64,
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
    aim_offset: f64,
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

/// Aim offset per keypress, in ball radii: enough to walk across a ball in a
/// few presses, with the shifted step for the last fraction of a degree.
const AIM_STEP: f64 = 0.12;
const AIM_FINE_STEP: f64 = 0.02;
/// The offset past which the cue ball misses the object ball altogether.
/// Slightly over two radii, so a deliberate swerve past it is still allowed.
const AIM_LIMIT: f64 = 2.4;
/// Tip movement per keypress, in ball radii.
const TIP_STEP: f64 = 0.05;
/// Pull per keypress, for the keyboard-only path.
const PULL_STEP: f64 = 0.05;

// ── Pointer sensitivity ───────────────────────────────────────────────
//
// All three are per *terminal cell*, and a cell is about twice as tall as it
// is wide, so the vertical rates are roughly double their horizontal twins to
// keep the gesture feeling isotropic.

/// Aim offset per column of pointer travel: a whole ball's width in about
/// twenty-five columns, which is a comfortable sweep at the minimum width.
const AIM_PER_COLUMN: f64 = 0.08;
/// The same walk on the other axis — how far off centre rather than which way
/// — at the doubled rate a cell's shape asks for.
const AIM_PER_ROW: f64 = 0.16;
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
            target,
            aim_at: default_aim(state, target),
            aim_offset: 0.0,
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

    /// Distance from the cue ball to the aim point, for the cue panel's
    /// depth scaling.
    pub fn aim_distance(&self, state: &DailyPoolState) -> f64 {
        let Some(cue) = self.cue_ball(state) else {
            return 1.0;
        };
        let (dx, dy) = (self.aim_at[0] - cue[0], self.aim_at[1] - cue[1]);
        dx.hypot(dy)
    }

    /// The point the cue ball is actually sent at: `aim_at` slid sideways by
    /// the aim offset, across the shot line.
    ///
    /// Doing it this way rather than nudging an angle is what makes the offset
    /// mean the same thing at every distance — half a ball off is half a ball
    /// off whether the target is a foot away or the length of the table, and
    /// that is exactly what the cue panel draws.
    pub fn aim_point(&self, state: &DailyPoolState) -> Option<[f64; 2]> {
        let cue = self.cue_ball(state)?;
        let spec = state.spec().ok()?;
        let (dx, dy) = (self.aim_at[0] - cue[0], self.aim_at[1] - cue[1]);
        let len = dx.hypot(dy);
        if len < 1e-9 {
            return None;
        }
        // Left-hand normal to the shot line.
        let (nx, ny) = (-dy / len, dx / len);
        let slide = self.aim_offset * spec.ball_radius;
        Some([self.aim_at[0] + nx * slide, self.aim_at[1] + ny * slide])
    }

    /// Where the cue ball's centre will be at the moment it touches the target
    /// — the "ghost ball" every player aims with.
    ///
    /// `None` when the aim misses the target ball, which is itself the useful
    /// answer: the ghost vanishing is how the board says the line is off it.
    pub fn ghost(&self, state: &DailyPoolState) -> Option<[f64; 2]> {
        let cue = self.cue_ball(state)?;
        let aim = self.aim_point(state)?;
        let spec = state.spec().ok()?;
        let target = state.rack.get(self.target?)?;
        if target.potted.is_some() {
            return None;
        }

        let (dx, dy) = (aim[0] - cue[0], aim[1] - cue[1]);
        let len = dx.hypot(dy);
        if len < 1e-9 {
            return None;
        }
        let dir = [dx / len, dy / len];
        // Closest approach of the aim line to the target's centre.
        let to_target = [target.pos[0] - cue[0], target.pos[1] - cue[1]];
        let along = to_target[0] * dir[0] + to_target[1] * dir[1];
        if along <= 0.0 {
            return None; // the target is behind the cue ball
        }
        let perp = (to_target[0] * to_target[0] + to_target[1] * to_target[1] - along * along)
            .max(0.0)
            .sqrt();
        let touch = 2.0 * spec.ball_radius;
        if perp >= touch {
            return None; // the line passes clean by
        }
        let back = (touch * touch - perp * perp).sqrt();
        let hit = along - back;
        Some([cue[0] + dir[0] * hit, cue[1] + dir[1] * hit])
    }

    /// The move to send. `None` when there is nowhere to shoot from or the
    /// aim point sits on top of the cue ball, which has no direction.
    pub fn shot(&self, state: &DailyPoolState) -> Option<Shot> {
        let cue = self.cue_ball(state)?;
        let aim = self.aim_point(state)?;
        let (dx, dy) = (aim[0] - cue[0], aim[1] - cue[1]);
        if dx.hypot(dy) < 1e-9 {
            return None;
        }
        Some(Shot {
            place: self.place,
            azimuth: dy.atan2(dx),
            tip: self.tip,
            speed: self.power().clamp(0.05, 1.0) * MAX_SPEED,
            called_pocket: self.called_pocket,
            play_again: false,
        })
    }

    pub fn azimuth(&self, state: &DailyPoolState) -> f64 {
        self.shot(state).map(|shot| shot.azimuth).unwrap_or(0.0)
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
            aim_offset: self.aim_offset,
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
            self.aim_offset = restore.aim_offset;
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
            target: self.target,
            aim_at: self.aim_at,
            aim_offset: self.aim_offset,
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
            target: share.target,
            aim_at: share.aim_at,
            aim_offset: share.aim_offset,
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
    /// stroke has no meaningful neutral — half-drawn is not a thing anyone
    /// wants — so there it means put the cue down.
    pub fn reset(&mut self, state: &DailyPoolState) -> bool {
        match self.mode {
            // Nothing armed: put the whole shot back to square. Reaching for
            // "centre the spin" *after* committing it is the common case —
            // you look at the panel, decide against the english, and there is
            // nothing to re-arm and undo, because you already put the cue
            // down. So an idle right-click clears both adjustments at once.
            ShotMode::Idle => {
                let moved = self.aim_offset != 0.0 || self.tip != [0.0, 0.0];
                self.aim_offset = 0.0;
                self.tip = [0.0, 0.0];
                moved
            }
            ShotMode::Aim => {
                let moved = self.aim_offset != 0.0;
                self.aim_offset = 0.0;
                moved
            }
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
                // Sideways walks the aim across the ball. Up and down walk it
                // *in toward* and *out from* centre — the same range reached a
                // second way, because a pointer runs out of screen long before
                // an aim runs out of range, and a player aiming from the right
                // of the board had nowhere left to push.
                self.nudge_aim(dx * AIM_PER_COLUMN);
                self.spread_aim(dy * AIM_PER_ROW);
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
            // Both axes, same as the pointer: left/right picks the side, up
            // and down how far off centre. Screen "up" is toward the far rail
            // (`board_move_cursor` hands us +1 for up), and up is *in* toward
            // centre, hence the negation.
            ShotMode::Aim => {
                self.nudge_aim(dx as f64 * AIM_STEP);
                self.spread_aim(-dy as f64 * AIM_STEP);
            }
            ShotMode::Spin => self.nudge_tip(dx as f64 * TIP_STEP, dy as f64 * TIP_STEP),
            ShotMode::Place => self.nudge_placement(state, dx, dy),
            ShotMode::Stroke(_) => self.nudge_pull(-dy as f64 * PULL_STEP),
        }
    }

    /// `h`/`l`, and `H`/`L` a fifth as far for the last fraction of a degree.
    pub fn key_aim(&mut self, delta: isize, fine: bool) {
        let step = if fine { AIM_FINE_STEP } else { AIM_STEP };
        self.nudge_aim(delta as f64 * step);
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
        let current = self
            .target
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

    pub fn aim_at_ball(&mut self, state: &DailyPoolState, id: u8) {
        let Some(ball) = state.rack.get(id).filter(|b| b.potted.is_none()) else {
            return;
        };
        self.target = Some(id);
        self.aim_at = ball.pos;
        // A new target makes the old offset meaningless — it was measured
        // across a different line.
        self.aim_offset = 0.0;
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

    /// Aim at a bare point on the cloth: a cushion, or a spot to send the cue
    /// ball to. The offset was measured across the old line, so it goes too.
    pub fn aim_at_point(&mut self, at: [f64; 2]) {
        self.target = None;
        self.aim_at = at;
        self.aim_offset = 0.0;
    }

    pub fn nudge_aim(&mut self, delta: f64) {
        self.aim_offset = (self.aim_offset + delta).clamp(-AIM_LIMIT, AIM_LIMIT);
    }

    /// Walk the aim away from the target's centre (positive) or back toward it
    /// (negative), keeping the side it is already on.
    ///
    /// The second axis of the aim. Sideways motion says *which way* off centre
    /// and this says *how far*, so the whole range is reachable without ever
    /// running the pointer into the edge of the screen — which is what makes
    /// aiming from the right-hand side of a board possible at all.
    ///
    /// Coming back in stops dead at centre rather than crossing to the other
    /// side: sliding through zero would flip the shot to the far side of the
    /// ball without the player asking for it, and "dead on" is a place you
    /// want to be able to land on.
    pub fn spread_aim(&mut self, delta: f64) {
        let side = if self.aim_offset < 0.0 { -1.0 } else { 1.0 };
        let reach = (self.aim_offset.abs() + delta).clamp(0.0, AIM_LIMIT);
        self.aim_offset = side * reach;
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
fn default_aim(state: &DailyPoolState, target: Option<u8>) -> [f64; 2] {
    target
        .and_then(|id| state.rack.get(id))
        .filter(|ball| ball.potted.is_none())
        .map(|ball| ball.pos)
        .unwrap_or_else(|| match state.spec() {
            Ok(spec) => rack::foot_spot(spec),
            Err(_) => [0.0, 0.0],
        })
}
