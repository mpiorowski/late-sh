//! The shot line: where the cue ball goes along an aim, and what it meets.
//!
//! An aim is a bearing from the cue ball. Everything else a player wants to
//! know about it is *derived* by walking that bearing across the table: the
//! first ball it reaches and where the cue ball will be when it touches it
//! (the ghost ball), the first stretch of the line the object ball leaves on,
//! the stun line the cue ball leaves on, or the cushion the cue ball meets
//! first and where it comes off it. None of this is physics: it is the
//! straight-line geometry every player draws in their head before a shot,
//! and it is cheap enough to redo on every pointer event.
//!
//! The one piece of physics the line does borrow is **throw**: a cut drags
//! the object ball a few degrees off the line of centres, toward the side the
//! cue ball is travelling to, and over a table's length that is more than a
//! pocket forgives. The object ball's leg leaves along `collide::throw`, the
//! same impulse model the simulator resolves the contact with, so the drawn
//! leg is where the ball goes and not where a diagram says it should.
//!
//! The object ball's leg is drawn short on purpose (`OBJECT_GUIDE_REACH`):
//! it shows which way the ball leaves, not where it ends up. Carried to the
//! first thing it met, it said whether a shot pots before the shot was
//! played, which is the part of the game the player is meant to judge.
//!
//! Surface-agnostic like the rest of the kernel: the board screen and a
//! future live table draw the same line.

use crate::app::games::pool_core::{
    ball::CUE,
    collide,
    cue::SPIN_PER_TIP,
    shot::BallFrame,
    table::{Geometry, TableSpec},
};

/// How far off a ball's centre the line may run and still count as *sighted*
/// on it, in ball radii. Past two the cue ball misses entirely; the extra is
/// so the cue panel keeps showing the ball while the aim is walked off its
/// edge rather than snapping to the rail behind it.
pub const SIGHT_REACH: f64 = 2.4;
/// How far the object ball's drawn leg reaches from its centre, as a share
/// of the playfield's length: enough to read the direction and too short to
/// line up a pocket across the table. Measured against the table rather than
/// the ball, because every table is scaled to fit the same panel: in ball
/// radii the snooker guide drew half the bar box's length on screen.
pub const OBJECT_GUIDE_REACH: f64 = 0.13;
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

/// The object ball's own leg after contact, as far as it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectLeg {
    pub id: u8,
    pub from: [f64; 2],
    /// Where the drawn leg ends: `OBJECT_GUIDE_REACH` of the table's length
    /// along the object ball's line, or sooner where its centre meets
    /// something.
    pub to: [f64; 2],
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
                to: object.to,
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

/// Walk `azimuth` from `from` and report everything on the way. `tip_side`
/// is the cue ball's english as the strike will apply it, in ball radii,
/// because it bends where the object ball goes.
pub fn shot_line(
    spec: &TableSpec,
    geom: &Geometry,
    balls: &[BallFrame],
    from: [f64; 2],
    azimuth: f64,
    tip_side: f64,
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
                    let out = object_direction(spec, dir, u, tip_side);
                    let object_hit = cast(spec, geom, balls, target.pos, out, &[CUE, id]);
                    let guide = OBJECT_GUIDE_REACH * spec.length;
                    let to = if distance(target.pos, object_hit.at()) < guide {
                        object_hit.at()
                    } else {
                        add(target.pos, scale(out, guide))
                    };
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
                            to,
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

/// Where the object ball leaves from a contact along the line of centres
/// `centres`, struck by a cue ball travelling along `dir` with `tip_side`
/// english: the line of centres turned by the throw.
pub fn object_direction(
    spec: &TableSpec,
    dir: [f64; 2],
    centres: [f64; 2],
    tip_side: f64,
) -> [f64; 2] {
    rotate(
        centres,
        collide::throw(spec, dir, centres, side_spin(tip_side)),
    )
}

/// The cue ball's english at contact as the collision model wants it,
/// `R·ωz / V`, from the tip offset the strike is given. Taken as it leaves
/// the tip: the spin decays far more slowly than the ball travels to any
/// object ball it can reach.
fn side_spin(tip_side: f64) -> f64 {
    -SPIN_PER_TIP * tip_side
}

/// The first thing a ball's centre reaches travelling along `dir` from
/// `from`, ignoring the balls in `ignore`.
///
/// A cushion is its segment *and* its two ends: the physics treats a jaw tip
/// as a point bumper a ball clips when its centre passes within a radius, so
/// the line does too, or it reports a clean drop where the table rattles.
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

    let rail_hit = geom.cushions.iter().filter_map(|cushion| {
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
    });
    // The jaw tips, as bumpers of one radius; the rebound is radial off the
    // tip, the way the resolver sends it.
    let jaw_hit = geom
        .cushions
        .iter()
        .flat_map(|cushion| [cushion.a, cushion.b])
        .filter_map(|tip| {
            let to = sub(tip, from);
            let along = dot(to, dir);
            if along <= 0.0 {
                return None;
            }
            let perp2 = dot(to, to) - along * along;
            if perp2 >= radius * radius {
                return None;
            }
            let back = (radius * radius - perp2).sqrt();
            let t = (along - back).max(0.0);
            let at = add(from, scale(dir, t));
            Some((t, unit(sub(at, tip))))
        });
    let cushion_hit = rail_hit.chain(jaw_hit).min_by(|a, b| a.0.total_cmp(&b.0));

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

/// `v` turned by `angle` radians, positive toward `ẑ × v`.
fn rotate(v: [f64; 2], angle: f64) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}
