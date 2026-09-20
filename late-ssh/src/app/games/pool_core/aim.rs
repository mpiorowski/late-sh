//! The shot line: where the cue ball goes along an aim, and what it meets.
//!
//! An aim is a bearing from the cue ball. Everything else a player wants to
//! know about it is *derived* by walking that bearing across the table: the
//! first ball it reaches and where the cue ball will be when it touches it
//! (the ghost ball), the line the object ball leaves on and where that ends,
//! the stun line the cue ball leaves on, or the cushion the cue ball meets
//! first and where it comes off it. None of this is physics: it is the
//! straight-line geometry every player draws in their head before a shot,
//! and it is cheap enough to redo on every pointer event.
//!
//! The pot lines are the same geometry run backwards: for a target ball and
//! a pocket, the ghost ball is fixed, so the bearing that pots it is one
//! `atan2`. Whether the pot is *on* is whether the two straight legs are
//! clear, which is the same cast again.
//!
//! Surface-agnostic like the rest of the kernel: the board screen and a
//! future live table draw the same line.

use crate::app::games::pool_core::{
    ball::CUE,
    shot::BallFrame,
    table::{Geometry, TableSpec},
};

/// How far off a ball's centre the line may run and still count as *sighted*
/// on it, in ball radii. Past two the cue ball misses entirely; the extra is
/// so the cue panel keeps showing the ball while the aim is walked off its
/// edge rather than snapping to the rail behind it.
pub const SIGHT_REACH: f64 = 2.4;
/// Steeper than this and the object ball barely moves: not a pot anyone
/// would offer.
pub const MAX_POT_CUT: f64 = 80.0 * std::f64::consts::PI / 180.0;
/// Length of the drawn stun line, in ball radii, for a cue ball leaving a
/// full-speed contact at right angles. Scaled down by the sine of the cut,
/// since that is the share of its speed the cue ball keeps.
const TANGENT_STUB: f64 = 8.0;

/// What the cue ball's centre reaches first along the line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// An object ball, with the cue ball's centre at the moment of contact.
    Ball { id: u8, ghost: [f64; 2] },
    /// A cushion, with the cue ball's centre at the moment of contact and
    /// the rail's inward normal, for the rebound.
    Cushion { at: [f64; 2], normal: [f64; 2] },
    /// Straight into a pocket mouth, at the point the centre leaves the cloth.
    Pocket { index: u8, at: [f64; 2] },
    /// Off the table with nothing in the way: only possible from a cue ball
    /// that is not on the cloth to begin with.
    Nothing { at: [f64; 2] },
}

impl Hit {
    /// Where the cue ball's centre stops on this leg.
    pub fn at(&self) -> [f64; 2] {
        match self {
            Self::Ball { ghost, .. } => *ghost,
            Self::Cushion { at, .. } | Self::Pocket { at, .. } | Self::Nothing { at } => *at,
        }
    }
}

/// The object ball's own leg after contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectLeg {
    pub id: u8,
    pub from: [f64; 2],
    /// What the object ball reaches first along its line.
    pub hit: Hit,
    /// The cut angle, signed: positive sends the object ball to the right of
    /// the shot line as seen from behind the cue ball, negative to the left.
    pub cut: f64,
}

/// One drawn segment of a shot line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Leg {
    pub from: [f64; 2],
    pub to: [f64; 2],
    pub kind: LegKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegKind {
    /// Cue ball to its first contact.
    Cue,
    /// The object ball onward from contact.
    Object(u8),
    /// The cue ball's stun line off the object ball.
    Tangent,
    /// The cue ball off a cushion.
    Rebound,
}

/// Everything a renderer draws for an aim and every number a readout shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotLine {
    pub from: [f64; 2],
    pub azimuth: f64,
    pub hit: Hit,
    /// The ball the line runs closest to, within `SIGHT_REACH` and before
    /// anything stops it, with how far off its centre the line passes in ball
    /// radii. Positive is to the right of the ball as seen from behind the
    /// cue ball, which is the side the cue panel draws it on.
    pub sighted: Option<(u8, f64)>,
    pub object: Option<ObjectLeg>,
    /// Where the cue ball's stun line ends. `None` on a full-ball contact,
    /// where the cue ball stops dead.
    pub tangent: Option<[f64; 2]>,
    /// Where the cue ball ends up after its first cushion.
    pub rebound: Option<[f64; 2]>,
}

impl ShotLine {
    pub fn target(&self) -> Option<u8> {
        self.sighted.map(|(id, _)| id)
    }

    pub fn ghost(&self) -> Option<[f64; 2]> {
        match self.hit {
            Hit::Ball { ghost, .. } => Some(ghost),
            Hit::Cushion { .. } | Hit::Pocket { .. } | Hit::Nothing { .. } => None,
        }
    }

    /// The legs in drawing order: the cue ball's first, so what comes after
    /// contact is painted over it.
    pub fn legs(&self) -> Vec<Leg> {
        let mut legs = vec![Leg {
            from: self.from,
            to: self.hit.at(),
            kind: LegKind::Cue,
        }];
        if let Some(object) = self.object {
            legs.push(Leg {
                from: object.from,
                to: object.hit.at(),
                kind: LegKind::Object(object.id),
            });
        }
        if let Some(to) = self.tangent {
            legs.push(Leg {
                from: self.hit.at(),
                to,
                kind: LegKind::Tangent,
            });
        }
        if let Some(to) = self.rebound {
            legs.push(Leg {
                from: self.hit.at(),
                to,
                kind: LegKind::Rebound,
            });
        }
        legs
    }
}

/// A bearing that sends `target` at a pocket, and how thin the cut is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PotLine {
    pub target: u8,
    pub pocket: u8,
    pub azimuth: f64,
    /// Unsigned cut angle in radians. Zero is a full ball.
    pub cut: f64,
}

/// Walk `azimuth` from `from` and report everything on the way.
pub fn shot_line(
    spec: &TableSpec,
    geom: &Geometry,
    balls: &[BallFrame],
    from: [f64; 2],
    azimuth: f64,
) -> ShotLine {
    let (sin, cos) = azimuth.sin_cos();
    let dir = [cos, sin];
    let radius = spec.ball_radius;
    let hit = cast(spec, geom, balls, from, dir, &[CUE]);
    let reach = distance(from, hit.at());

    let sighted = match hit {
        Hit::Ball { id, .. } => balls
            .iter()
            .find(|ball| ball.id == id)
            .map(|ball| (id, offset_of(from, dir, ball.pos, radius))),
        Hit::Cushion { .. } | Hit::Pocket { .. } | Hit::Nothing { .. } => balls
            .iter()
            .filter(|ball| !ball.potted && ball.id != CUE)
            .filter_map(|ball| {
                let along = dot(sub(ball.pos, from), dir);
                let offset = offset_of(from, dir, ball.pos, radius);
                (along > 0.0 && along < reach && offset.abs() <= SIGHT_REACH)
                    .then_some((along, (ball.id, offset)))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, sighted)| sighted),
    };

    let (object, tangent) = match hit {
        Hit::Ball { id, ghost } => {
            let target = balls.iter().find(|ball| ball.id == id);
            match target {
                Some(target) => {
                    let u = unit(sub(target.pos, ghost));
                    let object_hit = cast(spec, geom, balls, target.pos, u, &[CUE, id]);
                    let cut = cross(dir, u).atan2(dot(dir, u));
                    // What the cue ball keeps is the component across the
                    // object ball's line, so the stub is that share of a
                    // full stub. Under half a radius it is not worth a mark.
                    let keep = cut.sin().abs();
                    let tangent = if keep * TANGENT_STUB < 0.5 {
                        None
                    } else {
                        let t = unit(sub(dir, scale(u, dot(dir, u))));
                        Some(add(ghost, scale(t, keep * TANGENT_STUB * radius)))
                    };
                    (
                        Some(ObjectLeg {
                            id,
                            from: target.pos,
                            hit: object_hit,
                            cut,
                        }),
                        tangent,
                    )
                }
                None => (None, None),
            }
        }
        Hit::Cushion { .. } | Hit::Pocket { .. } | Hit::Nothing { .. } => (None, None),
    };

    let rebound = match hit {
        Hit::Cushion { at, normal } => {
            let out = sub(dir, scale(normal, 2.0 * dot(dir, normal)));
            Some(cast(spec, geom, balls, at, out, &[CUE]).at())
        }
        Hit::Ball { .. } | Hit::Pocket { .. } | Hit::Nothing { .. } => None,
    };

    ShotLine {
        from,
        azimuth,
        hit,
        sighted,
        object,
        tangent,
        rebound,
    }
}

/// The pots on offer for `target` from `from`: one per pocket the ball can be
/// sent to along two clear straight lines at a playable cut, easiest first.
pub fn pot_lines(
    spec: &TableSpec,
    geom: &Geometry,
    balls: &[BallFrame],
    from: [f64; 2],
    target: u8,
) -> Vec<PotLine> {
    let Some(ball) = balls.iter().find(|ball| ball.id == target && !ball.potted) else {
        return Vec::new();
    };
    let mut lines: Vec<PotLine> = geom
        .pockets
        .iter()
        .enumerate()
        .filter_map(|(index, pocket)| {
            let to_pocket = unit(sub(pocket.center, ball.pos));
            let ghost = sub(ball.pos, scale(to_pocket, 2.0 * spec.ball_radius));
            let dir = unit(sub(ghost, from));
            let cut = dot(dir, to_pocket).clamp(-1.0, 1.0).acos();
            if cut > MAX_POT_CUT {
                return None;
            }
            let cue_leg = cast(spec, geom, balls, from, dir, &[CUE]);
            if !matches!(cue_leg, Hit::Ball { id, .. } if id == target) {
                return None;
            }
            let object_leg = cast(spec, geom, balls, ball.pos, to_pocket, &[CUE, target]);
            if !matches!(object_leg, Hit::Pocket { index: hit, .. } if hit == index as u8) {
                return None;
            }
            Some(PotLine {
                target,
                pocket: index as u8,
                azimuth: dir[1].atan2(dir[0]),
                cut,
            })
        })
        .collect();
    lines.sort_by(|a, b| a.cut.total_cmp(&b.cut));
    lines
}

/// The first thing a ball's centre reaches travelling along `dir` from
/// `from`, ignoring the balls in `ignore`.
pub fn cast(
    spec: &TableSpec,
    geom: &Geometry,
    balls: &[BallFrame],
    from: [f64; 2],
    dir: [f64; 2],
    ignore: &[u8],
) -> Hit {
    let radius = spec.ball_radius;
    let touch = 2.0 * radius;

    let ball_hit = balls
        .iter()
        .filter(|ball| !ball.potted && !ignore.contains(&ball.id))
        .filter_map(|ball| {
            let to = sub(ball.pos, from);
            let along = dot(to, dir);
            if along <= 0.0 {
                return None;
            }
            let perp2 = dot(to, to) - along * along;
            if perp2 >= touch * touch {
                return None;
            }
            let back = (touch * touch - perp2).sqrt();
            Some(((along - back).max(0.0), ball.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    let cushion_hit = geom
        .cushions
        .iter()
        .filter_map(|cushion| {
            let approach = dot(dir, cushion.normal);
            if approach >= -1e-12 {
                return None;
            }
            let clearance = dot(sub(from, cushion.a), cushion.normal);
            let t = ((radius - clearance) / approach).max(0.0);
            let at = add(from, scale(dir, t));
            // Past the end of the rail is the pocket mouth, not the rail.
            let ab = sub(cushion.b, cushion.a);
            let s = dot(sub(at, cushion.a), ab) / dot(ab, ab);
            (0.0..=1.0).contains(&s).then_some((t, cushion.normal))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));

    // Where the centre leaves the playfield if nothing stops it. A ray that
    // reaches this without meeting a rail went through a pocket mouth.
    let exit = [
        (dir[0] > 0.0).then(|| (spec.length - from[0]) / dir[0]),
        (dir[0] < 0.0).then(|| -from[0] / dir[0]),
        (dir[1] > 0.0).then(|| (spec.width - from[1]) / dir[1]),
        (dir[1] < 0.0).then(|| -from[1] / dir[1]),
    ]
    .into_iter()
    .flatten()
    .fold(f64::INFINITY, f64::min)
    .max(0.0);

    let rail_or_exit = match cushion_hit {
        Some((t, normal)) if t <= exit + 1e-9 => (t, Some(normal)),
        Some(_) | None => (exit, None),
    };

    match ball_hit {
        Some((t, id)) if t <= rail_or_exit.0 => Hit::Ball {
            id,
            ghost: add(from, scale(dir, t)),
        },
        Some(_) | None => {
            let at = add(from, scale(dir, rail_or_exit.0));
            if let Some(normal) = rail_or_exit.1 {
                return Hit::Cushion { at, normal };
            }
            if !at[0].is_finite() || !at[1].is_finite() {
                return Hit::Nothing { at: from };
            }
            let pocket = geom
                .pockets
                .iter()
                .enumerate()
                .map(|(index, pocket)| (index as u8, distance(pocket.center, at)))
                .filter(|(_, d)| *d <= spec.corner_mouth)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match pocket {
                Some((index, _)) => Hit::Pocket { index, at },
                None => Hit::Nothing { at },
            }
        }
    }
}

/// How far the line passes from a ball's centre, in radii, signed to the
/// screen: positive to the right as seen from behind the cue ball.
fn offset_of(from: [f64; 2], dir: [f64; 2], ball: [f64; 2], radius: f64) -> f64 {
    -cross(dir, sub(ball, from)) / radius
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(v: [f64; 2], k: f64) -> [f64; 2] {
    [v[0] * k, v[1] * k]
}

fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

fn unit(v: [f64; 2]) -> [f64; 2] {
    let len = dot(v, v).sqrt();
    if len < 1e-12 {
        [0.0, 0.0]
    } else {
        scale(v, 1.0 / len)
    }
}
