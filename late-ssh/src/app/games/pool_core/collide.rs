//! Impact resolvers: ball against ball, and ball against cushion.
//!
//! Both follow the same two-part recipe — a normal impulse set by the
//! restitution, then a tangential impulse that tries to kill the slip at the
//! contact patch and is capped by Coulomb friction. The interesting physics
//! is entirely in *where* the contact is:
//!
//! - **Ball on ball** the contact is on the horizontal line of centres, so
//!   only `v` and the vertical spin `ωz` can change. Draw and follow pass
//!   through a collision untouched, which is why a stun-run-through or a draw
//!   shot still works after contact. Throw (a cut shot pushing the object
//!   ball off the line of centres) falls out of the same tangential impulse
//!   with no special case.
//! - **Ball on cushion** the nose sits *above* the ball's centre, so the
//!   contact offset has a vertical component. That is what lets a cushion
//!   trade horizontal spin for speed, and lets english change the rebound
//!   angle.
//!
//! The slip-killing impulse for a sphere is `-(2/7) m u_t` whenever the
//! impulse is perpendicular to the contact offset; the `2/7` is the same
//! `1 + 5/2` denominator that gives the sliding phase its `7/2`.

use crate::app::games::pool_core::{ball::Ball, table::TableSpec};

/// Approach speed below which an impact is not worth resolving. Well under a
/// perceptible tap, but enough to stop two balls resting in contact from
/// resolving against each other forever.
///
/// `sim::active_contacts` must use the *same* threshold when it decides a
/// contact is live. If the sweep offers a contact the resolver then refuses,
/// the step is cut short for nothing and the loop makes no progress.
pub const MIN_APPROACH: f64 = 1e-7;

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn len3(a: [f64; 3]) -> f64 {
    dot3(a, a).sqrt()
}

/// Resolve a ball-ball impact. Returns the normal approach speed that was
/// resolved, which the caller reports as impact strength (the renderer uses
/// it for the click, the rules layer only cares that contact happened).
///
/// Returns `None` when the pair is already separating — the sweep can offer
/// a pair that has just been resolved, and resolving it twice would inject
/// energy.
pub fn ball_ball(a: &mut Ball, b: &mut Ball, spec: &TableSpec) -> Option<f64> {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let dist = (dx * dx + dy * dy).sqrt();
    if dist == 0.0 {
        return None;
    }
    let n = [dx / dist, dy / dist];
    // Tangent, ẑ × n̂.
    let t = [-n[1], n[0]];

    let approach = (a.vel[0] - b.vel[0]) * n[0] + (a.vel[1] - b.vel[1]) * n[1];
    if approach <= MIN_APPROACH {
        return None;
    }

    let m = spec.ball_mass;
    let r = spec.ball_radius;

    // Equal masses, reduced mass m/2: with e = 1 this is a clean exchange of
    // the normal components.
    let jn = 0.5 * (1.0 + spec.e_ball_ball) * m * approach;
    a.vel[0] -= jn / m * n[0];
    a.vel[1] -= jn / m * n[1];
    b.vel[0] += jn / m * n[0];
    b.vel[1] += jn / m * n[1];

    // Relative surface speed along the tangent. Both balls' english adds with
    // the *same* sign: at the contact they are rubbing the same way round.
    let slip =
        (a.vel[0] - b.vel[0]) * t[0] + (a.vel[1] - b.vel[1]) * t[1] + r * (a.spin[2] + b.spin[2]);
    // Killing that slip takes -m·slip/7 (see the module docs).
    let want = -m * slip / 7.0;
    let cap = spec.mu_ball_ball * jn;
    let p = want.clamp(-cap, cap);

    a.vel[0] += p / m * t[0];
    a.vel[1] += p / m * t[1];
    b.vel[0] -= p / m * t[0];
    b.vel[1] -= p / m * t[1];
    let dwz = 2.5 * p / (m * r);
    a.spin[2] += dwz;
    b.spin[2] += dwz;

    Some(approach)
}

/// Resolve a ball-cushion impact.
///
/// `n` is the unit direction from the cushion's nearest point to the ball —
/// which is the inward normal on a rail and the radial direction on a jaw
/// tip, so a jaw acts as a point bumper with no extra code.
pub fn ball_cushion(ball: &mut Ball, n: [f64; 2], spec: &TableSpec) -> Option<f64> {
    let approach = -(ball.vel[0] * n[0] + ball.vel[1] * n[1]);
    if approach <= MIN_APPROACH {
        return None;
    }

    let m = spec.ball_mass;
    let r = spec.ball_radius;
    let inertia = spec.inertia();

    // Contact offset from the ball's centre: horizontally back against the
    // cushion, vertically up to the nose height.
    let sin_t = (spec.cushion_contact_height() / r).clamp(-1.0, 1.0);
    let cos_t = (1.0 - sin_t * sin_t).sqrt();
    let c = [-n[0] * r * cos_t, -n[1] * r * cos_t, r * sin_t];
    let c_hat = [c[0] / r, c[1] / r, c[2] / r];

    // Normal impulse, along the *contact* normal rather than the horizontal.
    //
    // This distinction is not cosmetic. The friction impulse below is built
    // perpendicular to `c_hat`, and `c_hat` is tilted up by the nose height —
    // so a friction impulse perpendicular to it still has a component along
    // the horizontal `n`. Resolve the normal along `n` instead and friction
    // is free to push the ball *off* the rail, adding to the rebound. That
    // pumps energy in: a ball then bounces between two cushions at a constant
    // speed higher than it was struck at, forever. Keeping both impulses in
    // the contact frame means friction can only ever oppose motion.
    let approach_c = approach * cos_t;
    let jn = (1.0 + spec.e_cushion) * m * approach_c;
    let normal_impulse = [-jn * c_hat[0], -jn * c_hat[1], -jn * c_hat[2]];

    // Slip of the ball's surface at the contact, with the normal component
    // projected out so the friction impulse is perpendicular to the offset —
    // the condition under which the 2/7 slip-kill factor holds.
    let w = ball.spin;
    let surface = {
        let rot = cross(w, c);
        [ball.vel[0] + rot[0], ball.vel[1] + rot[1], rot[2]]
    };
    let along = dot3(surface, c_hat);
    let slip = [
        surface[0] - along * c_hat[0],
        surface[1] - along * c_hat[1],
        surface[2] - along * c_hat[2],
    ];
    let slip_mag = len3(slip);

    let friction_impulse = if slip_mag > 1e-12 {
        let want = 2.0 / 7.0 * m * slip_mag;
        let mag = want.min(spec.mu_cushion * jn);
        [
            -mag * slip[0] / slip_mag,
            -mag * slip[1] / slip_mag,
            -mag * slip[2] / slip_mag,
        ]
    } else {
        [0.0; 3]
    };

    let total = [
        normal_impulse[0] + friction_impulse[0],
        normal_impulse[1] + friction_impulse[1],
        normal_impulse[2] + friction_impulse[2],
    ];

    // The cloth takes the vertical component: balls never leave the table, so
    // only the horizontal impulse reaches the velocity. Dropping it can only
    // remove energy, never add it, which is what keeps the no-jump-shots
    // simplification honest. The torque, however, uses the full 3D impulse at
    // the true contact offset — that is the whole reason the cushion can
    // convert spin.
    ball.vel[0] += total[0] / m;
    ball.vel[1] += total[1] / m;

    let torque = cross(c, total);
    ball.spin[0] += torque[0] / inertia;
    ball.spin[1] += torque[1] / inertia;
    ball.spin[2] += torque[2] / inertia;

    Some(approach)
}
