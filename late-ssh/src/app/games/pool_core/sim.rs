//! The shot loop, and the file that owns the determinism guarantee.
//!
//! ## Why substeps rather than analytic event times
//!
//! The textbook event-based simulator solves for the exact time of the next
//! contact. For ball-against-ball under quadratic motion that is a quartic,
//! and near a grazing approach the quartic is close to double-rooted — which
//! shows up as balls passing through each other or as a root at `t ≈ 0` that
//! stalls the loop. Those are the two failure modes that actually bite.
//!
//! So the loop here keeps the part that matters and drops the part that is
//! fragile. Ball motion inside a step is still the *exact* closed form from
//! `ball.rs`, so trajectories never accumulate integration error. Only the
//! contact *time* is found numerically, by bisecting a boolean "has anything
//! newly overlapped" predicate for a **fixed 40 iterations**. Fixed iteration
//! count, no convergence test, no early exit: the work done is a pure
//! function of the state, which is what makes replays bit-identical.
//!
//! ## Determinism rules for this module
//!
//! - No `HashMap` or `HashSet` anywhere: iteration order must be positional.
//! - No `rand`: randomness enters only through `rack.rs`, behind a seed.
//! - Every loop bound is a constant, never data-dependent-until-converged.
//! - Simultaneous contacts resolve in a sorted, total order.

use crate::app::games::pool_core::{
    ball::{Ball, CUE},
    collide,
    cue::Strike,
    shot::{BallFrame, Pot, RackState, ShotEvent, ShotOutcome, TimedEvent, Timeline},
    table::{Geometry, TableSpec},
};

/// Position sample rate. Fast enough that linear interpolation between
/// samples is invisible at the 15fps the terminal repaints at, cheap enough
/// that a six second shot is a few hundred kilobytes of transient memory.
pub const TIMELINE_HZ: f64 = 120.0;

/// Longest single substep. Also the step used once everything is coasting.
const MAX_STEP: f64 = 5.0e-3;
/// Shortest substep. Stops a ball sitting exactly on a state transition from
/// pinning the loop at zero progress.
const MIN_STEP: f64 = 1.0e-6;
/// A step may not move the fastest ball more than this fraction of a radius,
/// which is what keeps a contact from being stepped clean over.
const TRAVEL_FRACTION: f64 = 0.25;
/// Fixed bisection depth. Constant by design — see the module docs.
const BISECT_ITERS: u32 = 40;
/// Guard rails. A real shot settles in under ten seconds and a few thousand
/// steps; these exist so a bug is a truncated shot rather than a hung session.
const MAX_STEPS: u32 = 400_000;
const MAX_TIME: f64 = 60.0;
/// Contacts resolved at a single instant before the loop gives up trying.
const MAX_RESOLVES: u32 = 64;
/// Extra separation applied after resolving an overlap, so the same contact
/// cannot immediately retrigger. A micron: far below anything renderable, far
/// above the round-off that would let the contact reappear next step.
const SEPARATION_SLOP: f64 = 1.0e-6;
/// A step shorter than this counts as no progress.
const MIN_PROGRESS: f64 = 1.0e-7;
/// Consecutive no-progress steps tolerated before the loop settles the slow
/// balls by hand. See `simulate`.
const MAX_STALL: u32 = 256;
/// Speed under which a ball is settled by the stall guard. One centimetre a
/// second is a ball that has stopped as far as anyone watching is concerned.
const CREEP_SPEED: f64 = 0.01;

/// A contact that needs resolving. Ordered so simultaneous contacts always
/// resolve in the same sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Contact {
    /// Two ball indices, always `low < high`.
    Pair(usize, usize),
    /// Ball index, cushion index.
    Cushion(usize, usize),
}

pub struct SimResult {
    pub rack: RackState,
    pub timeline: Timeline,
    pub outcome: ShotOutcome,
}

/// Play one shot to a standstill.
///
/// `start` is taken by value conceptually — the returned `rack` is the new
/// state and the caller keeps the old one if it wants it.
pub fn simulate(
    spec: &TableSpec,
    geom: &Geometry,
    start: &RackState,
    strike: &Strike,
) -> SimResult {
    let mut balls = start.balls.clone();
    if let Some(cue) = balls.iter_mut().find(|b| b.id == CUE) {
        cue.vel = [0.0, 0.0];
        cue.spin = [0.0; 3];
        cue.potted = None;
        strike.apply(cue, spec);
    }

    let mut timeline = Timeline::new(TIMELINE_HZ);
    let mut outcome = ShotOutcome::default();
    let frame_dt = 1.0 / TIMELINE_HZ;

    timeline.push_frame(frame_of(&balls));

    let mut t = 0.0f64;
    let mut steps = 0u32;
    let mut stalled = 0u32;

    while balls.iter().any(|b| b.is_moving(spec.ball_radius)) {
        if steps >= MAX_STEPS || t >= MAX_TIME {
            outcome.truncated = true;
            break;
        }
        steps += 1;

        let to_next_frame = (timeline.frame_count() as f64 * frame_dt - t).max(MIN_STEP);
        let dt = step_size(spec, &balls).min(to_next_frame);

        let baseline = active_contacts(spec, geom, &balls);
        let hit = first_violation(spec, geom, &balls, &baseline, dt);
        let taken = hit.unwrap_or(dt);

        advance_all(spec, &mut balls, taken);
        t += taken;

        // Stall guard. A cluster of balls settling against each other can
        // trade contacts faster than the clock advances: every step finds a
        // fresh overlap, bisects down to nothing, and resolves an impact too
        // small to change anything. When that has gone on long enough, the
        // remaining motion is by definition below what the guard's own speed
        // threshold calls moving, so park it rather than grind.
        if taken < MIN_PROGRESS {
            stalled += 1;
            if stalled >= MAX_STALL {
                for b in balls.iter_mut() {
                    if b.speed() < CREEP_SPEED {
                        b.vel = [0.0, 0.0];
                        b.spin = [0.0; 3];
                    }
                }
                stalled = 0;
            }
        } else {
            stalled = 0;
        }

        if hit.is_some() {
            resolve_at(
                spec,
                geom,
                &mut balls,
                &baseline,
                t,
                &mut outcome,
                &mut timeline,
            );
        }

        pot_balls(geom, &mut balls, t, &mut outcome, &mut timeline);

        if t + 1e-12 >= timeline.frame_count() as f64 * frame_dt {
            timeline.push_frame(frame_of(&balls));
        }
    }

    // Pad out to the frame that covers `t`, so frame `i` is always exactly
    // `i / hz` seconds in and the last one is the settled rack. `sample`
    // relies on that mapping; a final frame pushed at an off-grid time would
    // quietly shear the whole animation.
    let want = (t * TIMELINE_HZ).ceil() as usize + 1;
    while timeline.frame_count() < want {
        timeline.push_frame(frame_of(&balls));
    }
    timeline.duration = t;
    outcome.duration = t;

    for b in balls.iter_mut() {
        b.vel = [0.0, 0.0];
        b.spin = [0.0; 3];
    }

    SimResult {
        rack: RackState { balls },
        timeline,
        outcome,
    }
}

fn frame_of(balls: &[Ball]) -> Vec<BallFrame> {
    balls
        .iter()
        .map(|b| BallFrame {
            id: b.id,
            pos: b.pos,
            potted: b.potted.is_some(),
        })
        .collect()
}

/// The largest step that is safe for every ball right now.
fn step_size(spec: &TableSpec, balls: &[Ball]) -> f64 {
    let mut dt = MAX_STEP;
    let mut fastest = 0.0f64;
    for b in balls.iter().filter(|b| b.potted.is_none()) {
        fastest = fastest.max(b.speed());
        let trans = b.time_to_transition(spec);
        if trans.is_finite() {
            dt = dt.min(trans.max(MIN_STEP));
        }
    }
    if fastest > 0.0 {
        dt = dt.min(TRAVEL_FRACTION * spec.ball_radius / fastest);
    }
    dt.clamp(MIN_STEP, MAX_STEP)
}

fn advance_all(spec: &TableSpec, balls: &mut [Ball], dt: f64) {
    for b in balls.iter_mut() {
        b.advance(spec, dt);
    }
}

/// Every contact that is both interpenetrating **and closing**, in a stable
/// order.
///
/// The "and closing" half is load-bearing, not an optimisation. Without it, a
/// ball settling against a rail flips between touching and not touching
/// forever: the resolver nudges it clear by `SEPARATION_SLOP`, the next
/// step's baseline no longer contains the contact, so the step after that
/// sees a brand-new violation and bisects down to a near-zero step. That is a
/// live-lock, and it is what the step cap used to catch. A resting contact
/// has zero approach speed, so with this filter it simply never appears —
/// which is also what the resolvers already believed, since both of them
/// refuse a non-approaching pair.
fn active_contacts(spec: &TableSpec, geom: &Geometry, balls: &[Ball]) -> Vec<Contact> {
    let r = spec.ball_radius;
    let mut out = Vec::new();
    for i in 0..balls.len() {
        if balls[i].potted.is_some() {
            continue;
        }
        for j in (i + 1)..balls.len() {
            if balls[j].potted.is_some() {
                continue;
            }
            let dx = balls[j].pos[0] - balls[i].pos[0];
            let dy = balls[j].pos[1] - balls[i].pos[1];
            let d2 = dx * dx + dy * dy;
            if d2 >= (2.0 * r) * (2.0 * r) || d2 == 0.0 {
                continue;
            }
            let d = d2.sqrt();
            let closing = (balls[i].vel[0] - balls[j].vel[0]) * (dx / d)
                + (balls[i].vel[1] - balls[j].vel[1]) * (dy / d);
            if closing > collide::MIN_APPROACH {
                out.push(Contact::Pair(i, j));
            }
        }
        for (c, cushion) in geom.cushions.iter().enumerate() {
            let (dist, dir) = Geometry::cushion_separation(cushion, balls[i].pos);
            if dist >= r {
                continue;
            }
            let closing = -(balls[i].vel[0] * dir[0] + balls[i].vel[1] * dir[1]);
            if closing > collide::MIN_APPROACH {
                out.push(Contact::Cushion(i, c));
            }
        }
    }
    out.sort_unstable();
    out
}

/// The earliest time within `dt` at which a contact appears that was not
/// already present, or `None` if the whole step is clean.
fn first_violation(
    spec: &TableSpec,
    geom: &Geometry,
    balls: &[Ball],
    baseline: &[Contact],
    dt: f64,
) -> Option<f64> {
    let violated = |probe: f64| -> bool {
        let mut copy = balls.to_vec();
        advance_all(spec, &mut copy, probe);
        active_contacts(spec, geom, &copy)
            .iter()
            .any(|c| !baseline.contains(c))
    };

    if !violated(dt) {
        return None;
    }
    let mut lo = 0.0;
    let mut hi = dt;
    for _ in 0..BISECT_ITERS {
        let mid = 0.5 * (lo + hi);
        if violated(mid) { hi = mid } else { lo = mid }
    }
    Some(hi)
}

/// Resolve every newly-touching contact at the current instant.
///
/// Loops because resolving one contact can leave another still approaching —
/// a ball pinched between two others, or a ball in a pocket jaw touching both
/// jaw tips. `MAX_RESOLVES` bounds it; the resolvers themselves refuse a
/// non-approaching pair, so a stable pile converges rather than buzzing.
fn resolve_at(
    spec: &TableSpec,
    geom: &Geometry,
    balls: &mut [Ball],
    baseline: &[Contact],
    t: f64,
    outcome: &mut ShotOutcome,
    timeline: &mut Timeline,
) {
    for _ in 0..MAX_RESOLVES {
        let contacts: Vec<Contact> = active_contacts(spec, geom, balls)
            .into_iter()
            .filter(|c| !baseline.contains(c))
            .collect();
        if contacts.is_empty() {
            return;
        }
        let mut resolved_any = false;
        for contact in contacts {
            match contact {
                Contact::Pair(i, j) => {
                    let (left, right) = balls.split_at_mut(j);
                    let (a, b) = (&mut left[i], &mut right[0]);
                    separate(a, b, spec.ball_radius);
                    if let Some(speed) = collide::ball_ball(a, b, spec) {
                        resolved_any = true;
                        record_contact(a.id, b.id, speed, t, outcome, timeline);
                    }
                }
                Contact::Cushion(i, c) => {
                    let cushion = &geom.cushions[c];
                    let (dist, dir) = Geometry::cushion_separation(cushion, balls[i].pos);
                    let push = spec.ball_radius - dist + SEPARATION_SLOP;
                    if push > 0.0 {
                        balls[i].pos[0] += dir[0] * push;
                        balls[i].pos[1] += dir[1] * push;
                    }
                    if let Some(speed) = collide::ball_cushion(&mut balls[i], dir, spec) {
                        resolved_any = true;
                        let id = balls[i].id;
                        if outcome.first_contact.is_some() {
                            outcome.cushion_after_contact = true;
                        }
                        if !outcome.balls_to_rail.contains(&id) {
                            outcome.balls_to_rail.push(id);
                        }
                        timeline.events.push(TimedEvent {
                            t,
                            event: ShotEvent::BallHitCushion { ball: id, speed },
                        });
                    }
                }
            }
        }
        if !resolved_any {
            return;
        }
    }
}

/// Push two overlapping balls apart along their line of centres so the
/// contact cannot immediately retrigger. Symmetric, so it adds no momentum.
fn separate(a: &mut Ball, b: &mut Ball, radius: f64) {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let dist = (dx * dx + dy * dy).sqrt();
    if dist == 0.0 || dist >= 2.0 * radius {
        return;
    }
    let push = (2.0 * radius - dist) * 0.5 + SEPARATION_SLOP;
    let (nx, ny) = (dx / dist, dy / dist);
    a.pos[0] -= nx * push;
    a.pos[1] -= ny * push;
    b.pos[0] += nx * push;
    b.pos[1] += ny * push;
}

fn record_contact(
    a: u8,
    b: u8,
    speed: f64,
    t: f64,
    outcome: &mut ShotOutcome,
    timeline: &mut Timeline,
) {
    if outcome.first_contact.is_none() {
        if a == CUE {
            outcome.first_contact = Some(b);
        } else if b == CUE {
            outcome.first_contact = Some(a);
        }
    }
    timeline.events.push(TimedEvent {
        t,
        event: ShotEvent::BallHitBall { a, b, speed },
    });
}

/// Drop any ball whose centre has crossed a pocket's mouth chord.
///
/// The chord test is the whole capture model: there is no radius to tune, and
/// a ball cannot be simultaneously on the table and in a pocket. A potted
/// ball is parked at the mouth centre so its stored position still means
/// something to the renderer.
fn pot_balls(
    geom: &Geometry,
    balls: &mut [Ball],
    t: f64,
    outcome: &mut ShotOutcome,
    timeline: &mut Timeline,
) {
    for ball in balls.iter_mut() {
        if ball.potted.is_some() {
            continue;
        }
        for (index, pocket) in geom.pockets.iter().enumerate() {
            if Geometry::pocket_depth(pocket, ball.pos) > 0.0 {
                ball.potted = Some(index as u8);
                ball.pos = pocket.center;
                ball.vel = [0.0, 0.0];
                ball.spin = [0.0; 3];
                outcome.potted.push(Pot {
                    ball: ball.id,
                    pocket: index as u8,
                });
                if ball.id == CUE {
                    outcome.cue_potted = true;
                }
                timeline.events.push(TimedEvent {
                    t,
                    event: ShotEvent::BallPotted {
                        ball: ball.id,
                        pocket: index as u8,
                    },
                });
                break;
            }
        }
    }
}
