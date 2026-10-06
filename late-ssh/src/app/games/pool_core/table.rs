//! Equipment: table dimensions, cloth and ball coefficients, and the one
//! builder that turns a `TableSpec` into playable geometry.
//!
//! Everything here is SI (metres, kilograms, seconds, radians). Nothing in
//! the kernel hard-codes a dimension: swapping in a snooker table later is a
//! new `TableSpec` constant, not new code.
//!
//! ## Coordinate system
//!
//! Origin at the bottom-left corner of the playfield, `x` along the long
//! axis, `y` along the short one, `z` up out of the cloth. A ball's stored
//! position is the centre of the ball projected onto the cloth plane; the
//! centre itself is always `ball_radius` above it, since balls never leave
//! the table (no jump shots, decided in the design).
//!
//! ## Geometry
//!
//! `Geometry` is built once per spec and is the *only* description of the
//! table's shape the physics reads. Cushions and pocket mouths come out of
//! the same function on purpose: if the rails were built separately from the
//! mouths they could drift apart, and a gap between them is exactly the bug
//! where a ball leaks off the table.
//!
//! Cushions are plain segments in real space; a ball collides when its centre
//! is `ball_radius` from the segment. That covers the jaw tips for free — a
//! segment endpoint acts as a point bumper, which is what makes a ball rattle
//! in the jaws rather than being magically swallowed.
//!
//! A pocket is the chord between its two jaw tips. Potting is the ball's
//! centre crossing that chord outwards, so there is no capture radius to tune
//! and no way for a ball to be both "in a pocket" and "on the table".

use serde::{Deserialize, Serialize};

/// Inches to metres — every published table dimension is in inches.
const IN: f64 = 0.0254;

/// A table, its cloth, and its balls. All the physics constants live here so
/// tuning never needs a recompile of anything but this file.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableSpec {
    /// Preset name, for display and for the persisted state's provenance.
    pub name: &'static str,

    // ── Table ──────────────────────────────────────────────────────────
    /// Playfield length, cushion nose to cushion nose (metres).
    pub length: f64,
    /// Playfield width, cushion nose to cushion nose (metres).
    pub width: f64,
    /// Height of the cushion nose above the cloth. Standard is 63.5% of ball
    /// diameter; this is what decides how much a cushion converts spin, so it
    /// is a real physics input, not decoration.
    pub cushion_height: f64,
    /// The baulk line, in metres from the baulk (low-x) end. The kitchen is
    /// behind it and it is painted on the cloth, so the rule and the marking
    /// are the same number.
    pub head_string: f64,
    /// Radius of the D drawn on the baulk line. Snooker plays out of it; pool
    /// only wears it.
    pub d_radius: f64,
    /// Corner pocket mouth, jaw tip to jaw tip.
    pub corner_mouth: f64,
    /// Side pocket mouth, jaw tip to jaw tip.
    pub side_mouth: f64,

    // ── Balls ──────────────────────────────────────────────────────────
    pub ball_radius: f64,
    pub ball_mass: f64,

    // ── Cloth ──────────────────────────────────────────────────────────
    /// Sliding friction, ball against cloth. Decides how quickly a struck
    /// ball settles into a natural roll.
    pub mu_slide: f64,
    /// Rolling resistance. Decides how far a rolling ball runs — one of the
    /// two coefficients every player notices.
    pub mu_roll: f64,
    /// Spinning (vertical-axis) friction. Dimensionless; see
    /// `spin_decel` for the derivation from the contact patch.
    pub mu_spin: f64,

    // ── Impacts ────────────────────────────────────────────────────────
    pub e_ball_ball: f64,
    pub mu_ball_ball: f64,
    pub e_cushion: f64,
    pub mu_cushion: f64,

    pub gravity: f64,
}

impl TableSpec {
    /// Moment of inertia of a solid sphere, `2/5 m r²`.
    pub fn inertia(&self) -> f64 {
        0.4 * self.ball_mass * self.ball_radius * self.ball_radius
    }

    /// Deceleration of a sliding ball's contact-point slip speed.
    ///
    /// While sliding, the slip direction is constant and its magnitude falls
    /// linearly at `7/2 μ_s g` — the `7/2` is the linear `1` plus the angular
    /// `5/2` from the friction torque. This is what makes the sliding phase
    /// closed-form rather than integrated.
    pub fn slip_decel(&self) -> f64 {
        3.5 * self.mu_slide * self.gravity
    }

    /// Deceleration of a rolling ball's speed.
    pub fn roll_decel(&self) -> f64 {
        self.mu_roll * self.gravity
    }

    /// Angular deceleration of spin about the vertical axis.
    ///
    /// From a circular contact patch of radius `a`: the friction torque is
    /// `2/3 a μ m g`, and dividing by `I = 2/5 m R²` gives `5 a μ g / (3 R²)`.
    /// Writing that as `5 μ_sp g / (2 R)` makes `μ_sp = 2 a μ / (3 R)`, which
    /// for a 1 mm patch and `μ = 0.2` on a 57.15 mm ball is about 0.0047 —
    /// hence the default. Keeping the `5 μ g / 2R` form means `mu_spin` reads
    /// on the same scale as the other two cloth coefficients.
    pub fn spin_decel(&self) -> f64 {
        2.5 * self.mu_spin * self.gravity / self.ball_radius
    }

    /// How far above the ball's centre the cushion nose touches it. Positive
    /// for a standard cushion (the nose is above centre, which is why a
    /// cushion can convert follow into extra forward roll).
    pub fn cushion_contact_height(&self) -> f64 {
        self.cushion_height - self.ball_radius
    }

    pub fn geometry(&self) -> Geometry {
        Geometry::build(self)
    }
}

/// A 7-foot bar box: the default. This is the "amateur, as seen in a bar"
/// table the game is specced against. Its 78x39 inch playfield also gives a
/// larger ball-to-table ratio than a 9-footer, which the terminal render
/// needs — a true-scale ball is only a few subpixels wide either way, but
/// every bit helps.
pub const BAR_BOX_7FT: TableSpec = TableSpec {
    name: "7ft bar box",
    length: 78.0 * IN,
    width: 39.0 * IN,
    head_string: 19.5 * IN,
    d_radius: 11.7 * IN,
    cushion_height: 0.635 * 2.25 * IN,
    // A tenth over a real bar box (4.5 / 5.0 inches). Aiming through a
    // terminal costs the player accuracy the equipment never took from them —
    // a ball is a few pixels and the pointer moves in whole cells — so the
    // pockets give back roughly what the rendering takes away. Bar boxes are
    // the buckets of the pool world anyway; this is at the loose end of real,
    // not outside it.
    corner_mouth: 4.95 * IN,
    side_mouth: 5.5 * IN,
    ball_radius: 1.125 * IN,
    ball_mass: 0.163,
    // Bar cloth is slower and more worn than tournament cloth: a shade more
    // rolling resistance than the 0.010 usually quoted for a fast table.
    mu_slide: 0.2,
    mu_roll: 0.015,
    mu_spin: 0.0047,
    e_ball_ball: 0.95,
    mu_ball_ball: 0.06,
    e_cushion: 0.85,
    mu_cushion: 0.2,
    gravity: 9.80665,
};

/// A 9-foot tournament table on faster cloth with tighter pockets. Not on the
/// roster; it exists so `TableSpec` is exercised by more than one preset and
/// so a future stake tier or a "pro table" variant is a constant, not a
/// refactor.
pub const PRO_9FT: TableSpec = TableSpec {
    name: "9ft tournament",
    length: 100.0 * IN,
    width: 50.0 * IN,
    head_string: 25.0 * IN,
    d_radius: 15.0 * IN,
    corner_mouth: 4.54 * IN,
    side_mouth: 5.09 * IN,
    mu_roll: 0.010,
    ..BAR_BOX_7FT
};

/// A full-size snooker table: twelve feet of it, with balls barely over two
/// inches and a playfield nearly twice the bar box's in each direction.
///
/// **The pockets are deliberately not snooker pockets.** A real one is 86mm
/// against a 52.5mm ball — 1.6 ball widths, *tighter* than a pool table's 2.2
/// — and tight pockets on a table drawn a hundred columns wide would make the
/// game about the rendering rather than about the shot. These are cut to the
/// same generosity as the bar box's, so what makes snooker hard here is the
/// thing that makes it hard in life: the distance.
pub const SNOOKER_12FT: TableSpec = TableSpec {
    name: "12ft snooker",
    length: 3.569,
    width: 1.778,
    head_string: 0.737,
    d_radius: 0.292,
    cushion_height: 0.635 * 0.0525,
    // 2.2 and 2.4 ball widths, matching the bar box rather than the real
    // article. See above.
    corner_mouth: 0.1155,
    side_mouth: 0.126,
    ball_radius: 0.02625,
    ball_mass: 0.142,
    // Snooker cloth is napped and runs faster than bar cloth.
    mu_slide: 0.19,
    mu_roll: 0.0100,
    mu_spin: 0.0044,
    e_ball_ball: 0.94,
    mu_ball_ball: 0.06,
    e_cushion: 0.86,
    mu_cushion: 0.2,
    gravity: 9.80665,
};

/// Every table this build knows how to play on.
pub const PRESETS: [&TableSpec; 3] = [&BAR_BOX_7FT, &PRO_9FT, &SNOOKER_12FT];

/// Look a preset up by `name`.
///
/// A stored match records the table it was played on and resolves it through
/// here, so retuning a preset's coefficients — or retiring one — surfaces as
/// an error on the match that needs it rather than silently reinterpreting a
/// rack that was never played on it. Unknown names are the caller's problem.
pub fn preset(name: &str) -> Option<&'static TableSpec> {
    PRESETS.into_iter().find(|spec| spec.name == name)
}

/// One straight cushion. `normal` points into the playfield and is a unit
/// vector; the physics only ever needs the segment plus that normal.
#[derive(Clone, Copy, Debug)]
pub struct Cushion {
    pub a: [f64; 2],
    pub b: [f64; 2],
    pub normal: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketKind {
    Corner,
    Side,
}

/// A pocket is the chord between its jaw tips. `outward` points away from the
/// playfield, so "the ball's centre has crossed to the far side of the chord"
/// is a sign test on `(c - jaw_a) · outward`.
#[derive(Clone, Copy, Debug)]
pub struct Pocket {
    pub kind: PocketKind,
    pub jaw_a: [f64; 2],
    pub jaw_b: [f64; 2],
    pub outward: [f64; 2],
    /// Midpoint of the chord: where the render draws the pocket, and where a
    /// potted ball is parked so its final position is still meaningful.
    pub center: [f64; 2],
}

/// The built table: cushions and pockets, always produced together.
#[derive(Clone, Debug)]
pub struct Geometry {
    pub cushions: Vec<Cushion>,
    pub pockets: Vec<Pocket>,
}

/// What to call pocket `index` out loud — for the called-shot prompt, the move
/// history, and the result line.
///
/// Lives beside `Geometry::build` on purpose: the names are positional, so the
/// only way they stay right is by sitting next to the list that fixes the
/// order. Table coordinates put the break end at low `x` and the foot end at
/// high `x`, which is also how the board draws it, so "top" and "bottom" mean
/// what the player sees.
pub fn pocket_name(index: u8) -> &'static str {
    match index {
        0 => "bottom left",
        1 => "top left",
        2 => "top right",
        3 => "bottom right",
        4 => "bottom side",
        5 => "top side",
        _ => "pocket",
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn norm(v: [f64; 2]) -> [f64; 2] {
    let len = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if len == 0.0 {
        [0.0, 0.0]
    } else {
        [v[0] / len, v[1] / len]
    }
}

impl Geometry {
    /// Build the rails and mouths from a spec.
    ///
    /// Each rail runs from one jaw tip to the next. A corner mouth is cut at
    /// 45 degrees, so its jaw sits `mouth/√2` along each rail from the corner
    /// point; a side mouth is square to its rail, so its jaws sit `mouth/2`
    /// either side of the middle. Those offsets are the *only* place the
    /// mouth widths are used, which is what keeps the rails and the pockets
    /// from ever disagreeing about where a mouth begins.
    fn build(spec: &TableSpec) -> Self {
        let (l, w) = (spec.length, spec.width);
        // Along-rail offset from a corner point to the corner jaw tip.
        let c = spec.corner_mouth / std::f64::consts::SQRT_2;
        // Along-rail offset from a side pocket's centre to its jaw tips.
        let s = spec.side_mouth / 2.0;
        let mid = l / 2.0;

        let up = [0.0, 1.0];
        let down = [0.0, -1.0];
        let right = [1.0, 0.0];
        let left = [-1.0, 0.0];

        let cushions = vec![
            // Bottom rail (y = 0), split by the bottom side pocket.
            Cushion {
                a: [c, 0.0],
                b: [mid - s, 0.0],
                normal: up,
            },
            Cushion {
                a: [mid + s, 0.0],
                b: [l - c, 0.0],
                normal: up,
            },
            // Top rail (y = w).
            Cushion {
                a: [c, w],
                b: [mid - s, w],
                normal: down,
            },
            Cushion {
                a: [mid + s, w],
                b: [l - c, w],
                normal: down,
            },
            // Left rail (x = 0) and right rail (x = l): no side pockets.
            Cushion {
                a: [0.0, c],
                b: [0.0, w - c],
                normal: right,
            },
            Cushion {
                a: [l, c],
                b: [l, w - c],
                normal: left,
            },
        ];

        let corner = |jaw_a: [f64; 2], jaw_b: [f64; 2], outward: [f64; 2]| Pocket {
            kind: PocketKind::Corner,
            jaw_a,
            jaw_b,
            outward: norm(outward),
            center: [(jaw_a[0] + jaw_b[0]) / 2.0, (jaw_a[1] + jaw_b[1]) / 2.0],
        };

        let pockets = vec![
            corner([c, 0.0], [0.0, c], [-1.0, -1.0]),
            corner([0.0, w - c], [c, w], [-1.0, 1.0]),
            corner([l - c, w], [l, w - c], [1.0, 1.0]),
            corner([l, c], [l - c, 0.0], [1.0, -1.0]),
            Pocket {
                kind: PocketKind::Side,
                jaw_a: [mid - s, 0.0],
                jaw_b: [mid + s, 0.0],
                outward: [0.0, -1.0],
                center: [mid, 0.0],
            },
            Pocket {
                kind: PocketKind::Side,
                jaw_a: [mid - s, w],
                jaw_b: [mid + s, w],
                outward: [0.0, 1.0],
                center: [mid, w],
            },
        ];

        Self { cushions, pockets }
    }

    /// Signed distance from a point to a cushion, plus the outward direction
    /// from the cushion to the point. Positive means "on the playing side".
    ///
    /// Clamping to the segment is what makes a jaw tip a point bumper: past
    /// the end of a rail the nearest feature is the endpoint, so a ball
    /// clipping a jaw bounces off it rather than off the rail's infinite line.
    pub fn cushion_separation(cushion: &Cushion, p: [f64; 2]) -> (f64, [f64; 2]) {
        let ab = sub(cushion.b, cushion.a);
        let ap = sub(p, cushion.a);
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 == 0.0 {
            0.0
        } else {
            ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
        };
        let closest = [cushion.a[0] + ab[0] * t, cushion.a[1] + ab[1] * t];
        let d = sub(p, closest);
        let dist = (d[0] * d[0] + d[1] * d[1]).sqrt();
        if dist == 0.0 {
            // Degenerate: sitting exactly on the rail. Push out along the
            // cushion's own normal so the resolver still has a direction.
            (0.0, cushion.normal)
        } else {
            (dist, [d[0] / dist, d[1] / dist])
        }
    }

    /// How far past a pocket's mouth chord a point sits. Positive means the
    /// ball has dropped.
    pub fn pocket_depth(pocket: &Pocket, p: [f64; 2]) -> f64 {
        let d = sub(p, pocket.jaw_a);
        d[0] * pocket.outward[0] + d[1] * pocket.outward[1]
    }
}
