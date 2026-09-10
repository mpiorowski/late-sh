//! The shooter's-eye view of the whole table: the same rack, seen from behind
//! the cue ball along the line of the shot.
//!
//! ## Why this is a raycaster and not a 3D renderer
//!
//! Everything worth seeing on a pool table lies on one plane. So instead of
//! projecting geometry onto the screen — which needs near-plane clipping, a
//! polygon rasteriser, and a horizon that has to be handled as a special case
//! — this walks the *screen* and asks each pixel what it is looking at: cast a
//! ray, intersect the cloth, and look up what is at that spot on the table.
//! Rays that pass above the horizon never hit the plane and are the room, for
//! free. The whole projection is four lines of arithmetic and there is no
//! clipping anywhere.
//!
//! It also buys the reverse mapping, which is what the mouse needs: a click is
//! a pixel, a pixel is a ray, a ray is a point on the cloth. The overview's
//! `View::to_table` has an exact twin here rather than an approximation.
//!
//! Only the balls are drawn as projected objects, because they are the only
//! things standing *off* the cloth. They are painted far to near, so a ball in
//! front covers one behind it — which is the whole point of the view, since a
//! ball hidden behind another is a shot you cannot take.

use crate::app::games::pool_core::{
    ball::CUE,
    canvas::{Canvas, Rgb},
    shot::BallFrame,
    table::{Geometry, PocketKind, TableSpec},
    table_ui::{
        CLOTH, GHOST, GUIDE, MARKING, POCKET, POCKET_CALLED, RAIL, RAIL_DARK, SURROUND, marking_at,
        paint_ball,
    },
};

/// Eye height above the cloth, in metres. Low — down near the cue rather than
/// up where a standing player's head is — because from up there the table
/// reads as an oval, and lining a pocket up behind a ball is the one thing
/// this view is for.
const EYE_HEIGHT: f64 = 0.30;
/// How far behind the cue ball the eye sits, in metres. **This is the dial for
/// how much perspective there is.** Standing right behind the ball makes the
/// near end tower over the far one; walking backwards flattens the whole thing
/// out toward a parallel projection, and the framing below compensates so the
/// table stays the same size on screen. Dolly back, zoom in.
const EYE_BACK: f64 = 0.55;
/// Where the cue ball and the far rail sit, as fractions of the canvas height.
/// **The framing is solved for these**, rather than the picture being whatever
/// a fixed lens happens to produce.
///
/// Fixing the lens instead meant the table landed wherever the aspect ratio
/// and the cue ball's position sent it: off the bottom of a wide short
/// terminal, a thin band across a tall one, and a different composition every
/// time the cue ball moved up the table. What a player needs is constant — the
/// ball they are about to hit near the bottom with room for a stroke behind
/// it, the far rail near the top with a little of the room showing above it —
/// so that is what is pinned, and the lens is what gives.
const CUE_ROW: f64 = 0.88;
const SKY_ROW: f64 = 0.26;
/// Horizontal focal length as a multiple of the canvas width. This one *is* a
/// lens: it sets how wide the field is, and a bar box wants enough to keep
/// both near rails in frame beside the cue ball.
const FOCAL_W: f64 = 1.05;
/// Balls never draw smaller than this, however far away they are. A true
/// perspective puts the far corner ball under a pixel, and a ball you cannot
/// see is worse than a ball drawn slightly too large.
const MIN_BALL_PX: f64 = 1.5;

/// Where the eye is and which way it looks. Built from the shot itself, so the
/// view *is* the aim: there is no camera to fly and nothing to get lost in.
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    at: [f64; 2],
    forward: [f64; 2],
    right: [f64; 2],
    focal: f64,
    rise: f64,
    centre: (f64, f64),
    height: f64,
}

impl Eye {
    /// Stand behind `cue` looking down `azimuth`, framed for this canvas.
    ///
    /// The vertical scale is *solved*, not chosen. A point at depth `d` sits
    /// `EYE_HEIGHT / d` of a `rise` below the horizon, so pinning two of them
    /// — the cue ball at `CUE_ROW`, the far end of the table at `SKY_ROW` —
    /// is two equations in `rise` and the horizon, and they have one answer:
    ///
    /// ```text
    /// rise = h(CUE_ROW - SKY_ROW) / (EYE_HEIGHT (1/EYE_BACK - 1/far))
    /// ```
    ///
    /// Which is why the perspective can be dialled with `EYE_BACK` alone: pull
    /// the eye back and the picture flattens, while `rise` grows to keep the
    /// table the same size on screen. Dolly back, zoom in.
    pub fn behind(cue: [f64; 2], azimuth: f64, spec: &TableSpec, canvas: &Canvas) -> Self {
        let (sin, cos) = azimuth.sin_cos();
        let forward = [cos, sin];
        // Screen +x is `up × forward`, not the `forward × up` a right-handed
        // world would give — because the overview draws +y *downward*, the way
        // screen rows run, and a view that disagreed with it about which side
        // of the table a ball is on would be a mirror image of the picture the
        // player already has in their head.
        let right = [-forward[1], forward[0]];
        let (w, h) = (canvas.cols() as f64, canvas.height() as f64);
        let at = [
            cue[0] - forward[0] * EYE_BACK,
            cue[1] - forward[1] * EYE_BACK,
        ];

        // How far away the furthest corner of the table is, along the look.
        // The corners rather than the far rail's midpoint, because which of
        // them is furthest depends on the angle and getting it wrong crops the
        // table exactly when the shot is down the diagonal.
        let far = [
            [0.0, 0.0],
            [spec.length, 0.0],
            [0.0, spec.width],
            [spec.length, spec.width],
        ]
        .into_iter()
        .map(|corner| (corner[0] - at[0]) * forward[0] + (corner[1] - at[1]) * forward[1])
        .fold(EYE_BACK * 1.5, f64::max);

        let spread = EYE_HEIGHT * (1.0 / EYE_BACK - 1.0 / far);
        let rise = h * (CUE_ROW - SKY_ROW) / spread;
        Self {
            at,
            forward,
            right,
            focal: w * FOCAL_W,
            rise,
            centre: (w / 2.0, h * CUE_ROW - EYE_HEIGHT / EYE_BACK * rise),
            height: EYE_HEIGHT,
        }
    }

    /// The spot on the cloth a pixel is looking at, or `None` above the
    /// horizon. This is the mouse hit test as well as the renderer.
    pub fn to_table(&self, px: f64, py: f64) -> Option<[f64; 2]> {
        self.probe(px, py).map(|(at, _)| at)
    }

    /// The same, with how much table one pixel covers there.
    ///
    /// Anything drawn *on* the cloth — the chalk lines — has to be widened to
    /// match, or it thins out with distance and disappears into the gaps
    /// between rays exactly where the table is smallest.
    pub fn probe(&self, px: f64, py: f64) -> Option<([f64; 2], f64)> {
        let sx = px - self.centre.0;
        let sy = py - self.centre.1;
        // At or above the horizon the ray never comes down to the cloth.
        if sy <= 0.5 {
            return None;
        }
        let depth = self.height * self.rise / sy;
        let across = sx / self.focal * depth;
        Some((
            [
                self.at[0] + self.forward[0] * depth + self.right[0] * across,
                self.at[1] + self.forward[1] * depth + self.right[1] * across,
            ],
            depth / sy,
        ))
    }

    /// A ball standing on the cloth at `at`: where to draw its disc, how big,
    /// and how far away it is.
    ///
    /// **The disc sits on its contact point rather than being projected from
    /// its centre.** The vertical is scaled far harder than the horizontal, so
    /// a ball's own height off the cloth — under three centimetres — came out
    /// as several pixels of lift, and balls near the far rail floated up over
    /// it and into the room. Standing a sprite on the spot it touches is both
    /// what a low view looks like and something that cannot happen in.
    pub fn sprite(&self, at: [f64; 2], radius: f64) -> Option<(f64, f64, f64, f64)> {
        let (x, foot, depth) = self.to_screen(at, 0.0)?;
        let r = (radius / depth * self.focal).max(MIN_BALL_PX);
        Some((x, foot - r, r, depth))
    }

    /// Project a point standing `up` metres above the cloth. `None` when it is
    /// level with or behind the eye, where perspective has no answer.
    pub fn to_screen(&self, at: [f64; 2], up: f64) -> Option<(f64, f64, f64)> {
        let d = [at[0] - self.at[0], at[1] - self.at[1]];
        let depth = d[0] * self.forward[0] + d[1] * self.forward[1];
        if depth <= 0.05 {
            return None;
        }
        let across = d[0] * self.right[0] + d[1] * self.right[1];
        let rise = up - self.height;
        Some((
            self.centre.0 + across / depth * self.focal,
            self.centre.1 - rise / depth * self.rise,
            depth,
        ))
    }
}

/// What to draw on the cloth under the balls, in table coordinates.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sight {
    pub aim_from: Option<[f64; 2]>,
    pub aim_to: Option<[f64; 2]>,
    pub ghost: Option<[f64; 2]>,
    pub called_pocket: Option<u8>,
}

/// Draw the table and the rack from the eye's point of view.
pub fn draw(
    canvas: &mut Canvas,
    spec: &TableSpec,
    geom: &Geometry,
    eye: &Eye,
    balls: &[BallFrame],
    sight: &Sight,
) {
    paint_cloth(canvas, spec, geom, eye, sight);
    // The aim line is *projected*, not sampled off the cloth like everything
    // else. A line a few millimetres wide falls between the rays on a short
    // canvas and vanishes exactly where it is needed; two projected endpoints
    // and a dotted line between them are crisp at any size. Drawn under the
    // balls, so a ball in the way hides it — which is the truth about the shot.
    if let (Some(from), Some(to)) = (sight.aim_from, sight.aim_to)
        // On the cloth, not at ball height: the balls are sprites standing on
        // their contact points, so a line drawn level with their centres would
        // float above the table they are sitting on.
        && let (Some(a), Some(b)) = (eye.to_screen(from, 0.0), eye.to_screen(to, 0.0))
    {
        canvas.line((a.0, a.1), (b.0, b.1), GUIDE, 1, 2);
    }
    paint_balls(canvas, spec, eye, balls);
    if let Some(at) = sight.ghost
        && let Some((x, y, r, _)) = eye.sprite(at, spec.ball_radius)
    {
        ring(canvas, x, y, r, GHOST);
    }
}

/// One ray per pixel: what is at that spot on the table?
fn paint_cloth(canvas: &mut Canvas, spec: &TableSpec, geom: &Geometry, eye: &Eye, sight: &Sight) {
    // The rail is a band outside the playfield, in metres rather than pixels:
    // in perspective its drawn thickness has to shrink with distance, and
    // measuring it on the table is what makes that happen by itself.
    let rail = spec.ball_radius * 3.0;
    for py in 0..canvas.height() as i32 {
        for px in 0..canvas.cols() as i32 {
            let Some((at, span)) = eye.probe(px as f64 + 0.5, py as f64 + 0.5) else {
                canvas.set(px, py, SURROUND);
                continue;
            };
            canvas.set(px, py, cloth_at(spec, geom, sight, at, rail, span * 0.5));
        }
    }
}

/// The colour of one spot on the table, or the room around it.
fn cloth_at(
    spec: &TableSpec,
    geom: &Geometry,
    sight: &Sight,
    at: [f64; 2],
    rail: f64,
    tol: f64,
) -> Rgb {
    let (x, y) = (at[0], at[1]);
    let outside = (-x).max(x - spec.length).max((-y).max(y - spec.width));
    if outside > rail {
        return SURROUND;
    }

    // A pocket swallows the cloth and the rail alike, so it is checked first.
    for (index, pocket) in geom.pockets.iter().enumerate() {
        let mouth = match pocket.kind {
            PocketKind::Corner => spec.corner_mouth,
            PocketKind::Side => spec.side_mouth,
        };
        let centre = [
            pocket.center[0] + pocket.outward[0] * mouth / 2.0,
            pocket.center[1] + pocket.outward[1] * mouth / 2.0,
        ];
        let reach = match pocket.kind {
            PocketKind::Corner => mouth / std::f64::consts::SQRT_2,
            PocketKind::Side => mouth / 2.0,
        };
        if (centre[0] - x).hypot(centre[1] - y) <= reach {
            return if sight.called_pocket == Some(index as u8) {
                POCKET_CALLED
            } else {
                POCKET
            };
        }
    }

    if outside > 0.0 {
        // The outer edge of the wood is darker, the same way the overview
        // draws it, so the table reads as an object and not a green shape.
        return if outside > rail * 0.85 {
            RAIL_DARK
        } else {
            RAIL
        };
    }

    if marking_at(spec, at, tol) {
        return MARKING;
    }
    CLOTH
}

/// Far to near, so a ball in front covers one behind it. That occlusion is the
/// point of the view: a ball you cannot see is a ball you cannot hit.
fn paint_balls(canvas: &mut Canvas, spec: &TableSpec, eye: &Eye, balls: &[BallFrame]) {
    let mut drawn: Vec<(f64, f64, f64, f64, u8)> = balls
        .iter()
        .filter(|ball| !ball.potted)
        .filter_map(|ball| {
            let (x, y, r, depth) = eye.sprite(ball.pos, spec.ball_radius)?;
            Some((depth, x, y, r, ball.id))
        })
        .collect();
    drawn.sort_by(|a, b| b.0.total_cmp(&a.0));
    for (_, x, y, r, id) in drawn {
        paint_ball(canvas, x, y, r, id);
    }
}

fn ring(canvas: &mut Canvas, cx: f64, cy: f64, radius: f64, colour: Rgb) {
    let inner = (radius - 1.0).max(0.0);
    let (r2, inner2) = (radius * radius, inner * inner);
    let span = radius.ceil() as i32;
    for dy in -span..=span {
        for dx in -span..=span {
            let (x, y) = (cx.floor() as i32 + dx, cy.floor() as i32 + dy);
            let ddx = x as f64 + 0.5 - cx;
            let ddy = y as f64 + 0.5 - cy;
            let d2 = ddx * ddx + ddy * ddy;
            if d2 <= r2 && d2 > inner2 {
                canvas.set(x, y, colour);
            }
        }
    }
}

/// The cue ball's own spot, for a caller that wants to stand behind it.
pub fn cue_spot(balls: &[BallFrame]) -> Option<[f64; 2]> {
    balls
        .iter()
        .find(|ball| ball.id == CUE && !ball.potted)
        .map(|ball| ball.pos)
}
