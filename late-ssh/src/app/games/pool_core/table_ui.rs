//! Drawing the table: cloth, rails, pockets, balls, aim guide.
//!
//! ## Balls are drawn bigger than they are
//!
//! A 2.25 inch ball on a 7 foot table is 1/35th of its length. Fit that table
//! into 100 columns of half blocks and a true-scale ball is three pixels
//! across — not enough to tell fifteen of them apart. So the table view draws
//! every ball at `MIN_BALL_PX` or its true size, whichever is larger, and is
//! honest about being an overview. **The physics always uses the true radius**;
//! nothing here feeds back into the simulation.
//!
//! Precision lives in the cue panel (`cue_ui`), which shows the target ball
//! and the cue ball close up. That split is what the wireframe is for.
//!
//! ## Telling balls apart
//!
//! Colour does the work, using the standard pool colours, and a stripe is told
//! from its solid the way a real one is: the hue runs as a band across the
//! middle and both ends are white. Drawn vertically it degrades on its own —
//! at three pixels the caps are a column each side, at ten they are two clean
//! stripes. Every ball also has its outermost shell shaded in its own hue,
//! which is what keeps a rack of overlapping discs reading as fifteen balls.
//! It has to be the ball's *own* shell: separating them with a ring of cloth
//! outside each one meant every ball took a bite out of the neighbour drawn
//! before it, and a cluster came out as rectangles. Numbers appear once the
//! ball is wide enough to hold them. The info panel carries the full list
//! whatever the size.

use crate::app::games::pool_core::{
    ball::CUE,
    canvas::{Canvas, Rgb, mix},
    rack,
    shot::BallFrame,
    table::{Geometry, PocketKind, TableSpec},
};

pub const CLOTH: Rgb = [22, 96, 62];
/// The room around the table. The table keeps its aspect ratio, so whichever
/// dimension it does not fill is left over — and left over as *cloth* it read
/// as a green table of the wrong shape rather than as a table in a room.
pub const SURROUND: Rgb = [24, 24, 27];
pub const RAIL: Rgb = [78, 52, 34];
pub const RAIL_DARK: Rgb = [52, 34, 22];
pub const POCKET: Rgb = [10, 10, 12];
/// The pocket being called, filled rather than ringed. A ring drawn round a
/// corner sat on the mouth chord, which is inside the cloth and diagonal to
/// everything, so it read as a circle floating beside the pocket rather than
/// on it. Filling the hole cannot be in the wrong place.
pub const POCKET_CALLED: Rgb = [150, 158, 152];
pub const CUE_BALL: Rgb = [242, 240, 232];
pub const WHITE: Rgb = [245, 243, 236];
pub const GUIDE: Rgb = [200, 214, 205];
pub const GHOST: Rgb = [150, 180, 165];

/// Smallest a ball may be drawn, in pixels. Below about two and a half the
/// disc stops reading as round and the stripe band has nowhere to go, and at
/// exactly that floor a rack is a row of dots — three is where the balls start
/// looking like balls on a small terminal.
const MIN_BALL_PX: f64 = 3.0;
/// Pixels added to a ball's true size, rather than a factor multiplying it.
///
/// The exaggeration exists because a true-scale ball is a speck, and that
/// stops being true the moment there is room — a *factor* keeps inflating
/// forever, so a terminal big enough to draw the table properly ends up with
/// balls half again as large as the table says they are. Adding a fixed pixel
/// and a half is self-limiting: it doubles a two-pixel ball and is a rounding
/// error on a twelve-pixel one.
///
/// It also removes the need to know which game is being drawn. A snooker ball
/// is smaller *and* on a longer table, so its true size is smaller at every
/// terminal width — and the floor plus the boost handle that for free, where a
/// per-table factor had to be calibrated against a reference table.
const BALL_BOOST: f64 = 1.5;
/// Rail thickness in ball *radii*, once the table is big enough for it.
///
/// A real rail is nearer two ball diameters and would eat a quarter of a
/// terminal, so this sits between the hairline the rails used to be and the
/// slab a real table wears. It has to be more than a hairline because a corner
/// pocket straddles the corner: without wood to sit in, half the pocket has
/// nowhere to be.
const RAIL_BALLS: f64 = 1.6;
/// Rails never go below this, so a cramped board still has a table edge.
const MIN_RAIL_PX: f64 = 2.0;
/// Drawn pockets come in a little under the jaw tips they reach for. A mouth
/// is measured tip to tip, but from above the jaws hide the outer sliver of
/// it, and a disc drawn to the full width looks like more table missing than
/// there is. Corners give up slightly more than sides because their disc runs
/// diagonally and so covers more cloth for the same mouth.
const CORNER_TRIM: f64 = 0.85;
const SIDE_TRIM: f64 = 0.9;
/// Half-width of a stripe's coloured band, as a fraction of the ball's radius.
/// Everything outside it is white. Low enough that the hue still reads on a
/// three-pixel ball, high enough that the caps are two clear stripes on a
/// large one.
const STRIPE_BAND: f64 = 0.55;
/// A ball at least this wide gets its number drawn in the cell.
const DIGIT_BALL_PX: f64 = 3.0;
/// A ball at least this wide carries its number twice, one copy above the
/// other — a real ball does the same so the number reads from either side.
/// Two pixel rows to the terminal row, so this is three rows of ball.
const REPEAT_BALL_PX: f64 = 6.0;

/// The printed colour of each ball. Stripes reuse their solid's hue, which is
/// exactly how a real set works.
pub fn ball_colour(id: u8) -> Rgb {
    match id {
        CUE => CUE_BALL,
        1 | 9 => [232, 186, 42],  // yellow
        2 | 10 => [44, 92, 200],  // blue
        3 | 11 => [206, 54, 46],  // red
        4 | 12 => [126, 62, 168], // purple
        5 | 13 => [232, 128, 36], // orange
        6 | 14 => [38, 148, 84],  // green
        7 | 15 => [140, 46, 46],  // maroon
        8 => [26, 26, 30],        // black
        // Snooker takes its own id range, so nothing here is ambiguous: the
        // fifteen reds, then the six colours in ascending value. Brighter than
        // the pool set, because a snooker table is twice the size and every
        // ball on it is drawn at the floor.
        16..=30 => [200, 40, 34], // red
        31 => [236, 200, 52],     // yellow
        32 => [42, 156, 76],      // green
        33 => [140, 92, 44],      // brown
        34 => [46, 96, 214],      // blue
        35 => [232, 128, 168],    // pink
        36 => [26, 26, 30],       // black
        _ => [180, 180, 180],
    }
}

pub fn is_stripe(id: u8) -> bool {
    (9..=15).contains(&id)
}

/// Chalk on the cloth: the baulk line, the D and the spots. Barely lighter
/// than the cloth, because these are markings on a table and not guides for
/// the shot — anything brighter competes with the aim line, which a player
/// does need to see.
pub const MARKING: Rgb = [46, 116, 84];
/// Half-width of a spot, in ball radii.
const SPOT_RADIUS: f64 = 0.35;

/// Is this spot on the cloth painted?
///
/// **One predicate, sampled by both views.** The overview walks its cloth
/// pixels and the eye view already walks rays, so neither of them needs to
/// know how to *draw* a line — and the two cannot end up disagreeing about
/// where the baulk line is, which would be a strange thing for a player to
/// have to notice. `tol` is half a pixel's worth of table at the point being
/// asked about, so a mark comes out about a pixel wide in either view however
/// far away it is.
///
/// The baulk line is the only one that means anything: it is
/// `rules::head_string`, the same number the kitchen rule is enforced against,
/// so a player put in hand behind it can see where "behind it" ends. The D and
/// the spots are cosmetic, and are here because a pool table has them.
pub fn marking_at(spec: &TableSpec, at: [f64; 2], tol: f64) -> bool {
    let head = crate::app::games::pool_core::rules::head_string(spec);
    let mid = spec.width / 2.0;
    if (at[0] - head).abs() <= tol {
        return true;
    }
    // The D bulges back toward the baulk cushion, off the line rather than
    // across it.
    if at[0] <= head {
        let d = (at[0] - head).hypot(at[1] - mid);
        if (d - spec.d_radius).abs() <= tol {
            return true;
        }
    }
    // A dot is a dot at any distance. True to size where there are pixels to
    // spare, and otherwise held between one and three of them: smaller and it
    // falls between the samples on an overview where the whole table is a
    // hundred columns, larger and it swells into a blob far down the eye view
    // where one pixel covers a hand's width of cloth.
    let spot = (spec.ball_radius * SPOT_RADIUS)
        .clamp(tol * 1.5, tol * 3.0)
        // Far down the eye view a pixel covers a hand's width of cloth *in
        // depth* while covering very little across it, so a circle sized by
        // depth alone comes out as a wide smear. A spot is never bigger than a
        // ball and a half whatever the arithmetic says.
        .min(spec.ball_radius * 1.5);
    [rack::foot_spot(spec), [head, mid]]
        .into_iter()
        .any(|s| (at[0] - s[0]).hypot(at[1] - s[1]) <= spot)
}

/// The ball's own colour, shaded for its outermost shell. Darker, except where
/// the ball is already almost black and there is no darker to go.
fn edge_shade(colour: Rgb) -> Rgb {
    let luma = 0.299 * colour[0] as f64 + 0.587 * colour[1] as f64 + 0.114 * colour[2] as f64;
    if luma < 60.0 {
        mix(colour, [255, 255, 255], 0.3)
    } else {
        mix(colour, [0, 0, 0], 0.35)
    }
}

/// Maps table coordinates (metres) onto canvas pixels.
#[derive(Clone, Copy, Debug)]
pub struct View {
    scale: f64,
    offset: (f64, f64),
    ball_px: f64,
    rail_px: f64,
}

impl View {
    /// Fit `spec`'s table into a canvas, preserving aspect and leaving room
    /// for the rails.
    pub fn fit(spec: &TableSpec, canvas: &Canvas) -> Self {
        Self::fit_area(spec, canvas.cols(), canvas.height())
    }

    /// The same fit from a size alone: columns and *pixel* rows.
    ///
    /// The mouse hit test needs this. Turning a click back into a spot on the
    /// cloth means inverting the very mapping the renderer used, and the input
    /// path has no canvas to hand — so both go through here rather than
    /// through two copies of the arithmetic that could drift apart.
    pub fn fit_area(spec: &TableSpec, cols: u16, height: u16) -> Self {
        let fit = |margin: f64| {
            let usable_w = (cols as f64 - 2.0 * margin).max(1.0);
            let usable_h = (height as f64 - 2.0 * margin).max(1.0);
            (usable_w / spec.length).min(usable_h / spec.width)
        };
        // The rails scale with the table, and the table is scaled to fit
        // *inside* them — so the fit runs twice: once at the thinnest rail to
        // learn how big a ball would be, and again once the rail that follows
        // from it is known. Two fixed passes, not a loop: the second scale is
        // never larger than the first, so it cannot grow the rail it was
        // derived from, and the result stays deterministic.
        let rail = (spec.ball_radius * fit(MIN_RAIL_PX + 1.0) * RAIL_BALLS).max(MIN_RAIL_PX);
        let scale = fit(rail + 1.0);
        let drawn_w = spec.length * scale;
        let drawn_h = spec.width * scale;
        Self {
            scale,
            offset: (
                (cols as f64 - drawn_w) / 2.0,
                (height as f64 - drawn_h) / 2.0,
            ),
            ball_px: (spec.ball_radius * scale + BALL_BOOST).max(MIN_BALL_PX),
            rail_px: rail,
        }
    }

    pub fn to_px(&self, at: [f64; 2]) -> (f64, f64) {
        (
            self.offset.0 + at[0] * self.scale,
            self.offset.1 + at[1] * self.scale,
        )
    }

    /// Canvas pixel back to table coordinates — the mouse hit test.
    pub fn to_table(&self, px: (f64, f64)) -> [f64; 2] {
        [
            (px.0 - self.offset.0) / self.scale,
            (px.1 - self.offset.1) / self.scale,
        ]
    }

    pub fn ball_px(&self) -> f64 {
        self.ball_px
    }

    /// Rail thickness in pixels, outside the playfield on every side.
    pub fn rail_px(&self) -> f64 {
        self.rail_px
    }
}

/// What to draw on top of the balls this frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Overlay {
    /// Aim line from the cue ball, in table coordinates.
    pub aim_to: Option<[f64; 2]>,
    /// Ghost ball at the contact point of the current aim.
    pub ghost: Option<[f64; 2]>,
    /// Ball to ring as the current target.
    pub highlight: Option<u8>,
    /// Ring the pocket being called.
    pub called_pocket: Option<u8>,
}

/// Draw the whole table into `canvas`.
pub fn draw(
    canvas: &mut Canvas,
    spec: &TableSpec,
    geom: &Geometry,
    view: &View,
    balls: &[BallFrame],
    overlay: &Overlay,
) {
    draw_bed(canvas, spec, view);
    draw_pockets(canvas, spec, geom, view, overlay.called_pocket);

    if let Some(to) = overlay.aim_to
        && let Some(cue) = balls.iter().find(|b| b.id == CUE && !b.potted)
    {
        let from = view.to_px(cue.pos);
        canvas.line(from, view.to_px(to), GUIDE, 1, 2);
    }
    if let Some(at) = overlay.ghost {
        let (x, y) = view.to_px(at);
        ring(canvas, x, y, view.ball_px(), GHOST);
    }
    for ball in balls.iter().filter(|b| !b.potted) {
        draw_ball(canvas, view, ball, overlay.highlight == Some(ball.id));
    }
}

fn draw_bed(canvas: &mut Canvas, spec: &TableSpec, view: &View) {
    // The room first, so whatever the fit leaves over is floor and not a
    // stray margin of cloth. Done here rather than left to the caller's
    // background colour: the surround is part of the drawing, and a caller
    // that got it wrong would look like a rendering bug in this file.
    canvas.fill_rect(0, 0, canvas.cols() as i32, canvas.height() as i32, SURROUND);
    let (x0, y0) = view.to_px([0.0, 0.0]);
    let (x1, y1) = view.to_px([spec.length, spec.width]);
    let (x0, y0, x1, y1) = (
        x0.round() as i32,
        y0.round() as i32,
        x1.round() as i32,
        y1.round() as i32,
    );

    // Rails first, then the cloth inside them. The outermost pixel is the
    // dark edge of the wood, so the table reads as a solid object against the
    // room rather than a coloured rectangle.
    let rail = view.rail_px().round() as i32;
    canvas.fill_rect(x0 - rail, y0 - rail, x1 + rail, y1 + rail, RAIL_DARK);
    canvas.fill_rect(
        x0 - rail + 1,
        y0 - rail + 1,
        x1 + rail - 1,
        y1 + rail - 1,
        RAIL,
    );
    // Flat cloth, deliberately. A shaded half looked like a seam down the
    // middle of the table, and on a surface where every mark is potentially a
    // ball or a guide, decoration that reads as a boundary is worse than none.
    canvas.fill_rect(x0, y0, x1, y1, CLOTH);

    // The chalk, sampled rather than drawn: `marking_at` is the same predicate
    // the eye view asks, so the two views cannot disagree about where the
    // baulk line is.
    let unit = view.to_px([1.0, 0.0]).0 - view.to_px([0.0, 0.0]).0;
    if unit > 0.0 {
        let tol = 0.5 / unit;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let at = view.to_table((x as f64 + 0.5, y as f64 + 0.5));
                if marking_at(spec, at, tol) {
                    canvas.set(x, y, MARKING);
                }
            }
        }
    }
}

/// Pockets are drawn at the size they actually are, scaled with the table
/// rather than with the exaggerated ball. Deriving the hole from `ball_px`
/// meant retuning a mouth changed how a pocket *played* and not how it
/// *looked*, which is the worst possible way for a player to find out.
///
/// **Every pocket is drawn on the table's edge, straddling it.** A side pocket
/// already was: its mouth chord lies along the rail, so a disc on the chord's
/// midpoint is half cloth and half wood. A corner's mouth chord is the
/// *diagonal* across the corner, and a disc centred on that sits wholly inside
/// the cloth — a black blob a ball's width in from the corner, which is
/// exactly what it looked like. So a corner is drawn on the **corner point**
/// instead, which is `mouth/2` further out along the chord's outward normal,
/// with a radius that reaches its own jaw tips. The four corners and the two
/// sides then line up on the same edge, as on a real table.
fn draw_pockets(
    canvas: &mut Canvas,
    spec: &TableSpec,
    geom: &Geometry,
    view: &View,
    called: Option<u8>,
) {
    let unit = view.to_px([1.0, 0.0]).0 - view.to_px([0.0, 0.0]).0;
    // Clip to the rails, so the half that is not on the cloth lands in the
    // wood and nothing spills into the room.
    let rail = view.rail_px().round() as i32;
    let (rx0, ry0) = view.to_px([0.0, 0.0]);
    let (rx1, ry1) = view.to_px([spec.length, spec.width]);
    let bounds = (
        rx0.round() as i32 - rail,
        ry0.round() as i32 - rail,
        rx1.round() as i32 + rail,
        ry1.round() as i32 + rail,
    );

    for (index, pocket) in geom.pockets.iter().enumerate() {
        let colour = if called == Some(index as u8) {
            POCKET_CALLED
        } else {
            POCKET
        };
        let mouth = match pocket.kind {
            PocketKind::Corner => spec.corner_mouth,
            PocketKind::Side => spec.side_mouth,
        };
        // One rule for both: the disc is centred on the table's edge and
        // reaches its own jaw tips, then trimmed a little. For a side the
        // centre is the chord midpoint, already on the rail, `mouth/2` from
        // each tip. For a corner it is the corner point — `mouth/2` further
        // out along the chord's outward normal — with the tips `mouth/√2`
        // away. The trims come off the radius only, so the centres stay on the
        // edge and the six stay aligned.
        let (at, reach) = match pocket.kind {
            PocketKind::Corner => (
                [
                    pocket.center[0] + pocket.outward[0] * mouth / 2.0,
                    pocket.center[1] + pocket.outward[1] * mouth / 2.0,
                ],
                mouth / std::f64::consts::SQRT_2 * CORNER_TRIM,
            ),
            PocketKind::Side => (pocket.center, mouth / 2.0 * SIDE_TRIM),
        };
        let (cx, cy) = view.to_px(at);
        // Never smaller than the ball is drawn: a pocket a ball visibly does
        // not fit into is a lie in the other direction.
        let radius = (reach * unit).max(view.ball_px() * 1.1);
        let span = radius.ceil() as i32;
        let r2 = radius * radius;
        for dy in -span..=span {
            for dx in -span..=span {
                let (x, y) = (cx.floor() as i32 + dx, cy.floor() as i32 + dy);
                if x < bounds.0 || y < bounds.1 || x > bounds.2 || y > bounds.3 {
                    continue;
                }
                let ddx = x as f64 + 0.5 - cx;
                let ddy = y as f64 + 0.5 - cy;
                if ddx * ddx + ddy * ddy <= r2 {
                    canvas.set(x, y, colour);
                }
            }
        }
    }
}

/// One ball: a plain disc for a solid, a banded one for a stripe.
///
/// A ball needs an edge of its own. Balls are drawn larger than life while
/// their positions stay true, so a fresh rack has every disc overlapping its
/// neighbours; without one, fifteen balls are one coloured smudge.
///
/// **That edge has to live inside the ball.** A ring of cloth painted just
/// *outside* each one separated them beautifully in isolation and was a
/// disaster in a cluster: each ball's halo carved a bite out of the neighbour
/// drawn before it, and overlapping balls came out as rectangles with their
/// stripes hanging off the side. Shading the ball's own outermost shell can
/// only ever change pixels the ball already owns.
///
/// Shaded rather than outlined, too: a darker rim of the ball's own hue is
/// what a sphere's edge looks like, where a contrasting outline reads as
/// decoration. The 8 goes the other way, since nothing is darker than it.
fn draw_ball(canvas: &mut Canvas, view: &View, ball: &BallFrame, highlighted: bool) {
    let (x, y) = view.to_px(ball.pos);
    let r = view.ball_px();
    paint_ball(canvas, x, y, r, ball.id);
    if highlighted {
        ring(canvas, x, y, r + 1.2, GUIDE);
    }
}

/// One ball at a place and a size, however the caller worked those out.
///
/// Shared with the shooter's-eye view, which arrives at the same three numbers
/// through a projection rather than a scale — a ball has to look like the same
/// ball in both, or the two views are two games.
pub(super) fn paint_ball(canvas: &mut Canvas, x: f64, y: f64, r: f64, id: u8) {
    let colour = ball_colour(id);

    canvas.disc(x, y, r, colour);
    if r >= 2.0 {
        ring(canvas, x, y, r, edge_shade(colour));
    }

    // A stripe is painted the way a real one looks from above: the colour is a
    // band across the middle and the two ends are white. Drawn *after* the
    // shading, so the white runs all the way to the ball's edge and the caps
    // read as caps rather than as two blobs floating beside it. It degrades on
    // its own — at three pixels the caps are a column each side, at ten they
    // are two clean stripes with the hue running between them.
    if is_stripe(id) {
        let band = r * STRIPE_BAND;
        let span = r.ceil() as i32;
        for dy in -span..=span {
            for dx in -span..=span {
                let (px, py) = (x.floor() as i32 + dx, y.floor() as i32 + dy);
                let ddx = px as f64 + 0.5 - x;
                let ddy = py as f64 + 0.5 - y;
                if ddx * ddx + ddy * ddy <= r * r && ddx.abs() > band {
                    canvas.set(px, py, WHITE);
                }
            }
        }
    }

    write_number(canvas, x, y, r, id);
}

/// The printed number, where the ball is big enough to carry it.
///
/// Three sizes, because the same table is played on an 80-column laptop and a
/// wall-sized terminal:
///
/// - too small: nothing. Half a number is worse than none.
/// - room for the digits: one copy, centred.
/// - room to spare: two copies, one above the other, the way a real ball
///   carries its number twice so it reads from either side.
///
/// Two-digit balls used to be skipped entirely, which meant the stripes — the
/// balls whose identity is hardest to read from hue alone — were the ones
/// wearing no number at all.
fn write_number(canvas: &mut Canvas, x: f64, y: f64, r: f64, id: u8) {
    if !(1..=15).contains(&id) {
        return;
    }
    let text = id.to_string();
    // Big enough to paint the number out of pixels instead of borrowing the
    // terminal's font: a glyph is drawn at the cell's size whatever the ball
    // is, so on a large table the number stops growing with the ball it is on
    // and turns into a speck. Painted digits scale with the disc.
    if paint_number(canvas, x, y, r, &text, id) {
        return;
    }
    let digits = text.len() as f64;
    // A digit is one cell wide, so the number needs `digits` columns of ball
    // to sit in, plus a little so it is not touching the rim.
    if r < DIGIT_BALL_PX.max(digits * 1.4) {
        return;
    }
    // The eight is nearly black; everything else is light enough for ink.
    let fg = if id == 8 { WHITE } else { [20, 20, 22] };
    let rows: &[f64] = if r >= REPEAT_BALL_PX {
        &[-2.0, 2.0]
    } else {
        &[0.0]
    };
    let start = x - digits / 2.0 + 0.5;
    for row in rows {
        for (i, ch) in text.chars().enumerate() {
            canvas.glyph(
                (start + i as f64).round() as i32,
                (y + row).round() as i32,
                ch,
                fg,
            );
        }
    }
}

/// A 3x5 pixel font, digits only.
///
/// **The high bit is the left column**, so each literal reads on the page the
/// way it lands on the ball. The first cut had it the other way round and the
/// one asymmetric digit that nobody double-checked — the `1` — came out
/// mirrored, its flag hanging off the wrong side of the stem. Writing them
/// visually is what makes that a thing you can *see* in review.
///
/// Three columns is the narrowest a digit can be and still tell 6 from 8, and
/// five rows is the shortest that leaves a middle bar somewhere to go. Hand
/// written here rather than borrowed: the one other pixel font in the tree
/// lives in `arcade`, which `games` may not depend on.
const DIGIT_PIXELS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// The chunkiest the painted font goes. Past this the number stops reading as
/// a number and starts reading as a pattern, and the ball loses its colour to
/// its own label.
const MAX_DIGIT_SCALE: i32 = 4;
/// A ball this big paints its number rather than borrowing a terminal glyph —
/// one digit column (five pixels) plus a pixel of margin, half of it either
/// side of centre.
const PAINT_BALL_PX: f64 = 3.5;

/// Paint the number out of pixels, if the ball has room for it. Reports
/// whether it drew, so the caller can fall back to the terminal's own glyph.
///
/// The font is drawn at the largest whole scale that fits, so the number grows
/// with the ball instead of staying the size of one terminal cell — which was
/// the whole complaint: on a big table a borrowed glyph is a speck in the
/// middle of a disc, while three painted pixels a side is legible at a glance.
///
/// The scale is the largest whose block fits inside the *disc* — the test is
/// the block's corner against the radius, not its height, because a number
/// poking out of the ball reads as a mark on the cloth.
///
/// **Except that a ball big enough to paint on always gets painted**, even
/// when two digits will not fit inside the disc at the smallest scale: a
/// number sticking out a little is a small ugliness, while a table where the
/// single-digit balls wear painted numbers and the double-digit ones wear
/// terminal glyphs is two different alphabets on the same rack. Since every
/// ball on a table is drawn at the same size, the choice is all or nothing,
/// and it should be made once for the rack rather than per ball.
fn paint_number(canvas: &mut Canvas, x: f64, y: f64, r: f64, text: &str, id: u8) -> bool {
    let digits = text.len() as i32;
    // Per scale: three columns a digit, one column of space between them, and
    // one of margin all round.
    let block = |k: i32| {
        let w = (digits * 3 * k + (digits - 1) * k) as f64;
        let h = 5.0 * k as f64;
        (w, h)
    };
    let fits = |k: i32| {
        let (w, h) = block(k);
        (w / 2.0 + k as f64).hypot(h / 2.0 + k as f64) <= r
    };
    // Whether the *rack* paints is decided by the height of one digit column,
    // which no ball can be short of while another is not.
    if r < PAINT_BALL_PX {
        return false;
    }
    let scale = (1..=MAX_DIGIT_SCALE).rev().find(|k| fits(*k)).unwrap_or(1);

    // A painted digit lands on the ball's own colour, so it needs the contrast
    // the rim gets: dark ink on everything but the black eight.
    let fg = if id == 8 { WHITE } else { [16, 16, 18] };
    let (w, h) = block(scale);
    let left = x - w / 2.0;
    let top = y - h / 2.0;
    for (index, ch) in text.chars().enumerate() {
        let Some(glyph) = ch.to_digit(10).map(|d| DIGIT_PIXELS[d as usize]) else {
            return false;
        };
        let x0 = (left + (index as i32 * 4 * scale) as f64).round() as i32;
        let y0 = top.round() as i32;
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                // High bit leftmost, matching how the font reads on the page.
                if bits & (0b100 >> col) == 0 {
                    continue;
                }
                for py in 0..scale {
                    for px in 0..scale {
                        canvas.set(x0 + col * scale + px, y0 + row as i32 * scale + py, fg);
                    }
                }
            }
        }
    }
    true
}

/// The outermost shell of the disc of the same radius.
///
/// Defined as an annulus tested at pixel centres rather than as a circle
/// walked by angle, because the parametric version rounded a continuous point
/// to a pixel *index* — half a pixel off the convention `Canvas::disc` uses —
/// and the two disagreed about which pixels were the edge. On a ball three
/// pixels across that showed as cloth-coloured dots inside the rim and ball
/// colour outside it. Sharing the test with `disc` makes that impossible.
pub(super) fn ring(canvas: &mut Canvas, cx: f64, cy: f64, radius: f64, colour: Rgb) {
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
