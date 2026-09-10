//! Pool state for daily correspondence matches: eight-ball and nine-ball.
//!
//! One state type serves both games. They share a table, a rack format, a
//! physics kernel and a shot; what differs is the ruleset, and that is a field
//! (`PoolRules`) rather than a second struct. This is the one place the daily
//! domain's "a variant is one game with different setup" rule bends: the
//! roster carries two entries because 8-ball and 9-ball genuinely are two
//! games, but everything below the ruleset is shared.
//!
//! ## What is stored, and what is recomputed
//!
//! A single shot can generate thousands of integration steps. Storing that per
//! move would put megabytes into a JSONB column, so the state keeps only what
//! cannot be recomputed:
//!
//! - `rack` — where the balls are now. Authoritative, so opening a board never
//!   re-simulates the match to find out.
//! - `prev_rack` — where they were before the last shot. Exactly enough to
//!   re-simulate that one shot and animate it, which is the only shot anyone
//!   ever watches.
//! - `shots` — the inputs, for the move list and for a full replay.
//!
//! Re-simulation is safe because `pool_core` is deterministic and runs only on
//! the server; there is no client that could disagree about the result. The
//! table preset is stored by name and resolved through `table::preset` so a
//! retuned coefficient can never silently reinterpret a stored rack.

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::app::games::pool_core::{
    ball::CUE,
    cue::{ShotMode, Strike},
    rack,
    rules::{self, BallInHand, Foul, Group, PoolRules, Seat, Turn},
    rules_snooker,
    shot::{PERSIST_DECIMALS, RackState, Shot, ShotOutcome, Timeline},
    sim,
    table::{self, Geometry, TableSpec},
};

const STATE_VERSION: u8 = 1;

/// The table each game is played on. Stored per match by name, so changing
/// this only affects matches claimed afterwards.
///
/// Snooker gets the twelve-footer, which is the whole of what makes it a
/// different game to play rather than only a different set of rules.
pub fn default_table(rules: PoolRules) -> &'static TableSpec {
    match rules {
        PoolRules::EightBall | PoolRules::NineBall => &table::BAR_BOX_7FT,
        PoolRules::Snooker => &table::SNOOKER_12FT,
    }
}

/// One shot as played, for the move list and for a full replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PoolShotRecord {
    pub seat: Seat,
    pub shot: Shot,
    /// Human-readable summary, e.g. `"3, 6 down"` or `"foul: scratch"`.
    pub label: String,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyPoolState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    pub rules: PoolRules,
    /// Equipment preset name; resolved through `table::preset`.
    pub table: String,
    /// Seat 0 breaks. Assigned by coin flip at claim time.
    pub seats: [Uuid; 2],
    /// Rack seed, kept so the opening rack can be rebuilt for a full replay.
    pub seed: u64,
    pub turn: Seat,
    /// Eight-ball groups by seat, `None` while the table is open. Always
    /// `None` in nine-ball.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups: Option<[Group; 2]>,
    /// Set by a foul, consumed by the next shot's placement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ball_in_hand: Option<BallInHand>,
    /// Snooker only: the frame score, by seat. It is the *whole* result there
    /// — a snooker frame is not won by potting the last ball, it is won by
    /// being ahead when there is nothing left to pot.
    #[serde(default)]
    pub scores: [i32; 2],
    /// Snooker only: the striker potted a red and is on a colour of their
    /// choosing.
    #[serde(default)]
    pub on_colour: bool,
    /// Snooker only: this striker was left snookered by a foul and may treat
    /// any ball as the ball on.
    #[serde(default)]
    pub free_ball: bool,
    /// Snooker only: the last shot was a foul, so the incoming player may hand
    /// it straight back rather than play. Cleared the moment they do either.
    #[serde(default)]
    pub may_return: bool,
    /// The foul the incoming player is being compensated for, so the board can
    /// say why they have ball in hand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_foul: Option<Foul>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<Seat>,
    /// The frame is over. Usually implied by `winner`, but snooker can end
    /// level, and a drawn frame is still a finished one.
    #[serde(default)]
    pub finished: bool,
    pub rack: RackState,
    /// Positions before the last shot. `None` until the break is played.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_rack: Option<RackState>,
    pub shots: Vec<PoolShotRecord>,
}

/// What one shot did, for the service to turn into a row update and an event.
pub struct PoolShotResult {
    pub outcome: ShotOutcome,
    pub label: String,
    pub foul: Option<Foul>,
    /// The rack is over and this seat took it.
    pub winner: Option<Seat>,
    /// The rack is over however it ended — including level, which snooker can
    /// do and the other two cannot.
    pub finished: bool,
}

impl DailyPoolState {
    pub fn new(rules: PoolRules, challenger: Uuid, claimer: Uuid) -> Self {
        // The coin flip decides who breaks, which in pool is the whole of the
        // opening advantage.
        let seats = if rand::random::<bool>() {
            [challenger, claimer]
        } else {
            [claimer, challenger]
        };
        let seed = rand::random::<u64>();
        let spec = default_table(rules);
        Self {
            version: STATE_VERSION,
            revision: 0,
            rules,
            table: spec.name.to_string(),
            seats,
            seed,
            turn: 0,
            groups: None,
            // Snooker breaks from the D — the cue ball is *in hand* for the
            // opening shot, which is a rule and not a nicety: where you break
            // from decides what the pack does.
            ball_in_hand: matches!(rules, PoolRules::Snooker).then_some(BallInHand::TheD),
            scores: [0, 0],
            on_colour: false,
            free_ball: false,
            may_return: false,
            last_foul: None,
            winner: None,
            finished: false,
            rack: rack::build(spec, rules.rack_kind(), seed).rounded(PERSIST_DECIMALS),
            prev_rack: None,
            shots: Vec::new(),
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily pool state version: {}",
            state.version
        );
        Ok(state)
    }

    /// The equipment this match was claimed on. An error rather than a
    /// fallback: a rack means nothing on a table it was not played on.
    pub fn spec(&self) -> Result<&'static TableSpec> {
        table::preset(&self.table)
            .ok_or_else(|| anyhow::anyhow!("unknown pool table preset: {}", self.table))
    }

    pub fn geometry(&self) -> Result<Geometry> {
        Ok(self.spec()?.geometry())
    }

    pub fn user_of(&self, seat: Seat) -> Uuid {
        self.seats[seat as usize]
    }

    pub fn seat_of(&self, user_id: Uuid) -> Option<Seat> {
        self.seats
            .iter()
            .position(|id| *id == user_id)
            .map(|i| i as Seat)
    }

    pub fn turn_user(&self) -> Uuid {
        self.user_of(self.turn)
    }

    pub fn move_count(&self) -> usize {
        self.shots.len()
    }

    pub fn is_finished(&self) -> bool {
        self.winner.is_some() || self.finished
    }

    /// The view the rules layer takes of the match right now.
    pub fn game_state(&self) -> rules::GameState {
        rules::GameState {
            on_colour: self.on_colour,
            free_ball: self.free_ball,
            rack: self.rack.clone(),
            turn: self.turn,
            groups: self.groups,
            shots_taken: self.shots.len() as u32,
            ball_in_hand: self.ball_in_hand,
        }
    }

    pub fn legal_targets(&self) -> Vec<u8> {
        self.rules.legal_targets(&self.game_state())
    }

    /// Whether this shot must name a pocket. Only the eight ever does.
    pub fn requires_call(&self) -> bool {
        self.rules.requires_call(&self.game_state())
    }

    /// Whether the cue ball has to be placed before the next shot. True after
    /// a scratch, when there is no cue ball on the table to shoot at all.
    pub fn must_place(&self) -> bool {
        self.rack.get(CUE).is_none_or(|b| b.potted.is_some())
    }

    /// Re-simulate the most recent shot for playback. `None` before the break,
    /// and on a state whose table preset this build no longer knows.
    pub fn last_timeline(&self) -> Option<Timeline> {
        let record = self.shots.last()?;
        let spec = self.spec().ok()?;
        let geom = spec.geometry();
        let mut start = self.prev_rack.clone()?;
        apply_placement(&mut start, record.shot.place).ok()?;
        let strike = strike_of(&record.shot).ok()?;
        Some(sim::simulate(spec, &geom, &start, &strike).timeline)
    }

    /// Play one shot: place the cue ball if asked, strike, simulate, judge,
    /// and fold the ruling back into the state.
    ///
    /// Every rejection here is a real illegality the client should have caught,
    /// so they are errors rather than silent no-ops — an optimistic client that
    /// gets one wrong needs to hear about it and reload.
    pub fn apply_shot(&mut self, seat: Seat, shot: &Shot) -> Result<PoolShotResult> {
        ensure!(!self.is_finished(), "the rack is over");
        ensure!(self.turn == seat, "not your turn");
        let spec = self.spec()?;
        let geom = spec.geometry();

        // Handing the shot straight back after a foul. It is a move like any
        // other — it takes the turn, joins the history and resets the clock —
        // but no ball moves, so it never reaches the simulator and there is
        // nothing to animate.
        if shot.play_again {
            ensure!(
                self.may_return,
                "there is nothing to hand back: the last shot was not a foul"
            );
            self.prev_rack = None;
            self.may_return = false;
            self.free_ball = false;
            self.on_colour = false;
            self.ball_in_hand = None;
            self.turn = rules::other_seat(seat);
            self.shots.push(PoolShotRecord {
                seat,
                shot: *shot,
                label: "play it again".to_string(),
                at: Utc::now(),
            });
            return Ok(PoolShotResult {
                outcome: ShotOutcome::default(),
                label: "play it again".to_string(),
                foul: None,
                winner: None,
                finished: false,
            });
        }

        if self.requires_call() {
            ensure!(
                shot.called_pocket
                    .is_some_and(|p| (p as usize) < geom.pockets.len()),
                "call a pocket for the eight"
            );
        }

        let before = self.game_state();
        let mut start = self.rack.clone();

        match shot.place {
            Some(at) => {
                let zone = self
                    .ball_in_hand
                    .ok_or_else(|| anyhow::anyhow!("you do not have ball in hand"))?;
                ensure!(
                    rules::placement_ok(spec, &geom, &start, at, zone),
                    "the cue ball cannot go there"
                );
                apply_placement(&mut start, Some(at))?;
            }
            // A potted cue ball has to be put somewhere before it can be hit.
            None if self.must_place() => bail!("place the cue ball first"),
            None => {}
        }

        let strike = strike_of(shot)?;
        let result = sim::simulate(spec, &geom, &start, &strike);

        // Whether the incoming player would be left unable to hit a ball on —
        // which is what decides a free ball, and which the rules layer cannot
        // work out for itself because it never sees the table. Only asked when
        // it could matter, since it walks every ball against every other.
        let snookered = self.rules.scores() && {
            let mut after = before.clone();
            after.rack = result.rack.clone();
            after.turn = rules::other_seat(seat);
            after.on_colour = false;
            after.free_ball = false;
            let on = self.rules.legal_targets(&after);
            rules_snooker::is_snookered(spec, &geom, &after.rack, &on)
        };
        let ruling = self
            .rules
            .judge(&before, &result.outcome, shot.called_pocket, snookered);

        let mut rack = result.rack.rounded(PERSIST_DECIMALS);
        // Spot what the ruling sent back up before anything else looks at the
        // rack: the incoming player's placement has to see the spotted balls.
        for id in &ruling.balls_to_spot {
            spot_ball(spec, &geom, &mut rack, *id);
        }

        let label = shot_label(&result.outcome, &ruling, self.rules);
        self.prev_rack = Some(self.rack.clone());
        self.rack = rack;
        self.groups = ruling.group_assignment.or(self.groups);
        self.ball_in_hand = ruling.ball_in_hand;
        self.last_foul = ruling.foul;
        self.scores[seat as usize] += ruling.points;
        self.scores[rules::other_seat(seat) as usize] += ruling.penalty;
        self.on_colour = ruling.next_on_colour;
        self.free_ball = ruling.free_ball;
        // The offer to hand the shot straight back only exists after a foul,
        // and only until the fouled player does something with it.
        self.may_return = ruling.foul.is_some();
        self.winner = match (ruling.winner, ruling.frame_over) {
            (Some(seat), _) => Some(seat),
            // A frame is not won by potting the last ball; it is won by being
            // ahead when there is nothing left to pot. Level is a draw here —
            // a real tie re-spots the black, which is a whole frame state for
            // an outcome that turns up once in a very long while.
            (None, true) => rules_snooker::frame_winner(self.scores),
            (None, false) => None,
        };
        self.finished = ruling.frame_over || self.winner.is_some();
        self.turn = match ruling.turn {
            Turn::Keep => seat,
            Turn::Pass => rules::other_seat(seat),
        };
        self.shots.push(PoolShotRecord {
            seat,
            shot: *shot,
            label: label.clone(),
            at: Utc::now(),
        });

        Ok(PoolShotResult {
            outcome: result.outcome,
            label,
            foul: ruling.foul,
            winner: self.winner,
            finished: self.is_finished(),
        })
    }
}

/// Put the cue ball at `at`, taking it back out of the pocket if it was in
/// one. Placement is validated by the caller; this only moves the ball.
fn apply_placement(racked: &mut RackState, at: Option<[f64; 2]>) -> Result<()> {
    let Some(at) = at else {
        return Ok(());
    };
    let cue = racked
        .balls
        .iter_mut()
        .find(|b| b.id == CUE)
        .ok_or_else(|| anyhow::anyhow!("this rack has no cue ball"))?;
    cue.pos = at;
    cue.potted = None;
    Ok(())
}

/// Return `id` to the table on the foot spot, or as near behind it as the
/// other balls allow. A ball with nowhere at all to go stays down — a table so
/// crowded that `free_spot` fails has no legal square left on it.
/// Put a ball back on the table.
///
/// A pool ball goes on the foot spot; a snooker colour goes back on its *own*
/// spot, which is the whole reason the colours have names. Either way the
/// preferred spot is only preferred — `free_spot` walks it back the way a
/// referee would when something is already sitting there.
fn spot_ball(spec: &TableSpec, geom: &Geometry, racked: &mut RackState, id: u8) {
    let preferred = rules_snooker::COLOURS
        .contains(&id)
        .then(|| {
            rack::colour_spots(spec)
                .into_iter()
                .find(|(colour, _)| *colour == id)
                .map(|(_, at)| at)
        })
        .flatten()
        .unwrap_or_else(|| rack::foot_spot(spec));
    let Some(at) = rules::free_spot(spec, geom, racked, preferred, BallInHand::Anywhere) else {
        return;
    };
    if let Some(ball) = racked.balls.iter_mut().find(|b| b.id == id) {
        ball.pos = at;
        ball.potted = None;
    }
}

fn strike_of(shot: &Shot) -> Result<Strike> {
    Strike::new(shot.azimuth, shot.tip[0], shot.tip[1], shot.speed)
        .map_err(|e| anyhow::anyhow!("illegal shot: {}", strike_error(e)))
}

fn strike_error(error: crate::app::games::pool_core::cue::StrikeError) -> &'static str {
    use crate::app::games::pool_core::cue::StrikeError;
    match error {
        StrikeError::Miscue => "the tip is too far off centre",
        StrikeError::BadSpeed => "that is not a playable stroke speed",
        StrikeError::BadAim => "that is not a direction",
    }
}

/// One line for the move list. Reads as a commentator would call it: what went
/// down, what was given away, and whether that was the rack.
fn shot_label(outcome: &ShotOutcome, ruling: &rules::Ruling, rules_kind: PoolRules) -> String {
    let mut parts = Vec::new();

    let potted: Vec<String> = outcome
        .potted_object_balls()
        .map(|id| id.to_string())
        .collect();
    if potted.is_empty() {
        parts.push(match outcome.first_contact {
            Some(hit) => format!("{hit}, no pot"),
            None => "no contact".to_string(),
        });
    } else {
        parts.push(format!("{} down", potted.join(", ")));
    }

    if let Some(foul) = ruling.foul {
        parts.push(format!("foul: {}", foul.label()));
    }
    if ruling.winner.is_some() {
        parts.push(
            match rules_kind {
                PoolRules::EightBall => "eight ball, rack over",
                PoolRules::NineBall => "nine ball, rack over",
                PoolRules::Snooker => "frame over",
            }
            .to_string(),
        );
    }
    parts.join(" · ")
}

/// What one player is lining up, as the other player's board draws it.
///
/// Everything a spectator needs to see a shot being *composed*, and nothing
/// else: no cue-ball position (they have the rack), no speed (the band and the
/// pull say it), no identity (the event carries that). Small and `Copy`, since
/// it rides a broadcast channel that fans out to every session on the replica.
///
/// **Not persisted and not acknowledged.** It describes an intention, and an
/// intention that is one event out of date is worth exactly as much as one
/// that is current — the next event carries the whole state, so a dropped one
/// costs nothing and there is nothing to reconcile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolAimShare {
    pub target: Option<u8>,
    pub aim_at: [f64; 2],
    pub aim_offset: f64,
    pub tip: [f64; 2],
    pub pull: f64,
    pub mode: ShotMode,
    pub place: Option<[f64; 2]>,
    pub called_pocket: Option<u8>,
}

#[cfg(test)]
#[path = "pool_test.rs"]
mod pool_test;
