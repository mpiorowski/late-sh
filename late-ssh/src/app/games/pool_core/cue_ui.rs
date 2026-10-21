//! The cue panel: the shooter's-eye view, and where the precision lives.
//!
//! The table view (`table_ui`) is an overview — a ball there is three pixels
//! and you cannot aim on it. This panel is the other half of that trade. It
//! shows two things large: the target ball out in the distance and the cue
//! ball right in front of you, with the alignment between them drawn as a
//! sighting line. A fraction of a degree of aim moves the alignment mark
//! visibly here while moving nothing at all on the table.
//!
//! Layout, top to bottom, following the wireframe:
//!
//! ```text
//!            ( 7 )        target ball, sized by distance
//!              ·
//!              ·          sighting line, offset shows the aim error
//!            (   )        cue ball, with the tip mark on its face
//!             ═╪═         the cue, drawn back by the current power
//! ```
//!
//! The wireframe's cue-angle readout was elevation, which this build does not
//! model — the cue stays level, so there is no masse and no jumping. The same
//! space carries the aim bearing instead, which is the angle that does change
//! and the one worth reading off during the aim step.

use crate::app::games::pool_core::{
    canvas::{Canvas, Rgb, mix},
    cue::{MISCUE_LIMIT, PULL_ROWS, PowerBand, ShotMode},
    table_ui::{self, CUE_BALL, FAULT, GUIDE, WHITE, ball_colour, is_stripe},
};

/// The panel's own background. Public so the board screen fills its canvas
/// with the same colour the panel paints over it.
pub const BACKDROP: Rgb = [14, 20, 18];

/// How much of the drawn cue ball's radius the tip may actually be placed
/// within: `MISCUE_LIMIT` lands here, not at the ball's edge.
///
/// The face is still magnified — a true-to-life face spends most of a small
/// ball drawing the part nobody may strike — but magnifying it all the way to
/// the rim put maximum screw on the very bottom pixel of the ball, which reads
/// as "the edge is the limit" and makes every extreme a click away. Inset, the
/// mark for a full draw sits visibly inside the ball and the extremes have to
/// be aimed at rather than fallen into. The rim between this and the ball's
/// edge is still the ball: a click there clamps to the limit (`nudge_tip`
/// already does) rather than falling through to the cue below it.
pub const TIP_FACE: f64 = 0.78;

/// Clear air below the cue ball before the cue begins, in radii of the ball.
///
/// Both a drawing and a hit-test number, which is why it is one constant: the
/// cue at rest starts here, and so does the strip where a click arms the
/// stroke. Without it the stroke zone began at the ball's own edge, so a click
/// aimed at the bottom of the face and landing a pixel low armed the cue
/// instead of placing the tip.
pub const CUE_GAP: f64 = 0.55;
const CUE_WOOD: Rgb = [186, 146, 92];
const CUE_TIP: Rgb = [80, 120, 170];
const TIP_MARK: Rgb = [210, 60, 60];
const DIM: Rgb = [70, 84, 78];
const RAIL_TARGET: Rgb = [120, 92, 64];

/// Everything the panel needs. All angles in radians, offsets in ball radii.
#[derive(Clone, Copy, Debug)]
pub struct CueView {
    /// The ball being shot at, or `None` when the target is a cushion.
    pub target: Option<u8>,
    /// That ball is not one the striker may hit first, so the shot as aimed
    /// is a foul. Said **in words** here rather than drawn: the panel's ball
    /// is a single huge disc with nothing beside it to compare against, so a
    /// coloured rim on it reads as decoration where the same rim on the table
    /// reads as "this one, not those". A line of red text cannot be mistaken
    /// for styling. Nothing is said about a legal target or a cushion — the
    /// panel only speaks up when the shot as aimed is a foul.
    pub target_fault: bool,
    /// Cue ball to target, in metres. Only scales the drawing.
    pub distance: f64,
    /// How far the aim line passes from the target's centre, in ball radii.
    /// Zero is dead centre; ±2 is a complete miss.
    pub aim_offset: f64,
    /// Tip placement on the cue ball's face: `[across, up]` in ball radii.
    pub tip: [f64; 2],
    /// Where the cue is, 0 (at the ball) to 1 (a full pull), *within* the
    /// armed band — the gesture, not the speed. Drawn one terminal row per
    /// row of pointer travel, so it moves exactly as the hand does.
    pub pull: f64,
    /// What the pointer is wired to, so that control is the bright one.
    pub mode: ShotMode,
    /// The shot has been struck and is on its way. The cue is drawn thrown
    /// forward through the ball and stays there until the shot is over — a
    /// stroke that snapped straight back to rest left the player unsure it had
    /// registered at all.
    pub follow_through: bool,
}

/// Where the panel put the things a click can land on, in canvas pixels.
///
/// The panel is the shot laid out vertically, so its geometry doubles as the
/// input map: the target ball and the sighting line above the cue ball arm the
/// aim, the cue ball's face arms spin, and the cue below it arms the stroke.
/// Handing the caller these rather than having it recompute them is what keeps
/// the picture and the pointer agreeing.
#[derive(Clone, Copy, Debug)]
pub struct PanelHit {
    pub cue: (f64, f64),
    pub cue_radius: f64,
    /// Radius of the *settable* face, which is smaller than the drawn ball —
    /// see `TIP_FACE`. A click is turned into a tip offset against this.
    pub tip_radius: f64,
    /// First row that belongs to the cue. Below the ball with `CUE_GAP` of
    /// clear air between, so the two zones do not touch.
    pub cue_top: f64,
    pub target: (f64, f64),
    pub target_radius: f64,
}

impl Default for CueView {
    fn default() -> Self {
        Self {
            target: None,
            target_fault: false,
            distance: 0.5,
            aim_offset: 0.0,
            tip: [0.0, 0.0],
            pull: 0.0,
            mode: ShotMode::Idle,
            follow_through: false,
        }
    }
}

/// Draw the panel. Returns what it drew where, so the caller can turn a click
/// into a tip placement or into the mode that part of the panel stands for.
pub fn draw(canvas: &mut Canvas, view: &CueView) -> PanelHit {
    let w = canvas.cols() as f64;
    let h = canvas.height() as f64;
    canvas.fill_rect(0, 0, canvas.cols() as i32, canvas.height() as i32, BACKDROP);

    let centre_x = w / 2.0;
    let target_y = h * 0.14;
    // The balls take a share of the panel rather than a fixed size, so the
    // whole drawing grows with the terminal. A modest share: past about a
    // sixth they stop being easier to aim at and start crowding out the cue.
    // The height cap is generous enough that a tall panel never shrinks them
    // — the extra rows are for the cue, not against the balls.
    let cue_r = (w * 0.16).clamp(3.0, h * 0.3);

    // A distant ball is a smaller ball. Floored so a long table shot still
    // leaves something aimable rather than a single pixel — and high enough
    // that the rim the ball wears (see `draw_target`) cannot swallow the
    // whole of it, which at a two-pixel floor it did.
    let shrink = (0.55 / view.distance.max(0.15)).clamp(0.35, 1.0);
    let target_r = (cue_r * 0.85 * shrink).max(3.0);

    // The cue ball sits low, the target high — the wireframe's depth cue. Not
    // as low as it looks, though: everything under the cue ball is the cue,
    // and the cue is the part that *moves*. Sitting the ball at two thirds
    // left the stroke a sliver to happen in, which is precisely the gesture a
    // player is trying to read.
    //
    // **Capped in absolute pixels as well as by the fraction.** Once the
    // sighting line is long enough to read, more of it buys nothing, where
    // more cue keeps buying resolution on the draw-and-push gesture that sets
    // the speed. So a tall panel spends its extra rows below the ball: the
    // ball stops descending and the cue grows.
    let sight = (cue_r * 2.5).max(6.0);
    let cue_y = (h * 0.58).min(target_y + target_r + cue_r + sight);

    draw_target(canvas, view, centre_x, target_y, target_r);
    draw_sighting_line(canvas, view, centre_x, target_y, target_r, cue_y, cue_r);
    draw_cue_ball(canvas, view, centre_x, cue_y, cue_r);
    draw_cue(canvas, view, centre_x, cue_y, cue_r, h);
    // Last, so it sits over the sighting line it shares a row with.
    if view.target_fault {
        draw_fault_warning(canvas, centre_x, target_y + target_r);
    }

    PanelHit {
        cue: (centre_x, cue_y),
        cue_radius: cue_r,
        tip_radius: cue_r * TIP_FACE,
        cue_top: cue_y + cue_rest_offset(cue_r),
        target: (centre_x, target_y),
        target_radius: target_r,
    }
}

/// How far below the cue ball's centre the cue sits at rest: the ball, plus
/// the clear air the click map also uses.
fn cue_rest_offset(cue_r: f64) -> f64 {
    cue_r * (1.0 + CUE_GAP)
}

fn draw_target(canvas: &mut Canvas, view: &CueView, x: f64, y: f64, r: f64) {
    let Some(id) = view.target else {
        // Shooting at a cushion: draw a length of rail instead of a ball, so
        // the panel is never blank and the player can see what they picked.
        canvas.fill_rect(
            (x - r * 2.5) as i32,
            (y - r * 0.4) as i32,
            (x + r * 2.5) as i32,
            (y + r * 0.4) as i32,
            RAIL_TARGET,
        );
        return;
    };

    let colour = ball_colour(id);
    canvas.disc(x, y, r, colour);
    if is_stripe(id) {
        stripe_caps(canvas, x, y, r, r * 0.42);
    }
    // The target always wears its number: this is the one place there is room
    // for two digits, which is why the table view can go without.
    write_number(canvas, x, y, id);
}

/// The one thing the panel says about legality, and only when the answer is
/// no: a line of red under the target ball.
///
/// Words rather than a rim, because this ball is drawn alone and enormous —
/// there is nothing beside it for a coloured edge to be read against, which is
/// exactly what makes the same mark work on the table. Centred, and skipped
/// rather than clipped on a panel too narrow to hold it; the table is still
/// ringing the ball in red either way.
fn draw_fault_warning(canvas: &mut Canvas, centre_x: f64, below: f64) {
    const TEXT: &str = "Invalid Target";
    let width = TEXT.chars().count() as f64;
    if width > canvas.cols() as f64 {
        return;
    }
    // A whole terminal row below the ball, so the glyphs never land in the
    // cell the ball's own bottom pixel is in.
    let y = below + 2.0;
    let start = (centre_x - width / 2.0).round() as i32;
    for (index, ch) in TEXT.chars().enumerate() {
        canvas.glyph(start + index as i32, y.round() as i32, ch, FAULT);
    }
}

/// The white caps of a stripe, painted **only on the pixels the ball owns**.
///
/// They used to be two rectangles with the overshoot erased afterwards, and the
/// erase was a bug: it tested a pixel at `dx + 0.5` from the centre and wrote
/// to `round(cx + dx)`, which are two different pixels whenever the ball's
/// centre is not on a whole one. So it ate a column down one side of the ball
/// and left the squared-off corners standing on the other, which is what the
/// panel's big target ball looked wrong on the right for. Filling only what the
/// disc owns cannot be off by anything: it is the same test `disc` itself uses.
fn stripe_caps(canvas: &mut Canvas, cx: f64, cy: f64, r: f64, band: f64) {
    let span = r.ceil() as i32;
    let (px0, py0) = (cx.floor() as i32, cy.floor() as i32);
    for dy in -span..=span {
        for dx in -span..=span {
            let (px, py) = (px0 + dx, py0 + dy);
            let ddx = px as f64 + 0.5 - cx;
            let ddy = py as f64 + 0.5 - cy;
            if ddx * ddx + ddy * ddy > r * r || ddy.abs() < band {
                continue;
            }
            canvas.set(px, py, WHITE);
        }
    }
}

fn write_number(canvas: &mut Canvas, x: f64, y: f64, id: u8) {
    let Some(text) = table_ui::printed_label(id) else {
        return;
    };
    let fg = table_ui::label_ink(id);
    let start = x - (text.len() as f64) / 2.0;
    for (i, ch) in text.chars().enumerate() {
        canvas.glyph((start + i as f64).round() as i32, y.round() as i32, ch, fg);
    }
}

/// The dotted line from the cue ball up to the target, offset sideways by the
/// current aim error. A centred line means the aim is through the middle of
/// the target ball.
fn draw_sighting_line(
    canvas: &mut Canvas,
    view: &CueView,
    x: f64,
    target_y: f64,
    target_r: f64,
    cue_y: f64,
    cue_r: f64,
) {
    let bright = view.mode == ShotMode::Aim;
    let colour = if bright { GUIDE } else { DIM };
    // The offset is in ball radii, so scale it by the target's drawn radius —
    // that keeps "half a ball off" looking like half a ball at any distance.
    let dx = view.aim_offset * target_r;
    // The line thickens with the panel. A hairline is right when the balls are
    // three pixels across and lost when they are twenty.
    let weight = ((cue_r / 4.0).round() as i32).clamp(1, 3);
    for step in 0..weight {
        let shift = step as f64 - (weight - 1) as f64 / 2.0;
        canvas.line(
            (x + shift, cue_y - cue_r),
            (x + dx + shift, target_y + target_r),
            colour,
            1,
            if bright { 1 } else { 2 },
        );
    }
    // A tick where the line meets the target, so a small error is still
    // visible when the line itself is nearly vertical.
    let tick = if bright { WHITE } else { colour };
    for step in 0..weight {
        let shift = step as f64 - (weight - 1) as f64 / 2.0;
        canvas.set(
            (x + dx + shift).round() as i32,
            (target_y + target_r).round() as i32,
            tick,
        );
    }
}

fn draw_cue_ball(canvas: &mut Canvas, view: &CueView, x: f64, y: f64, r: f64) {
    canvas.disc(x, y, r, CUE_BALL);
    // Shade the lower right so the white ball reads as a sphere rather than a
    // hole in the panel.
    //
    // The sweep has to use the same pixel-centre convention `disc` does, or
    // the two disagree about which pixels are inside and the ones the shading
    // misses are left stranded at full white — more of them the bigger the
    // ball, which is how a panel that grew showed up a bug that a fixed-size
    // one had been hiding.
    let span = r.ceil() as i32;
    let (px0, py0) = (x.floor() as i32, y.floor() as i32);
    for dy in -span..=span {
        for dx in -span..=span {
            let (px, py) = (px0 + dx, py0 + dy);
            let ddx = px as f64 + 0.5 - x;
            let ddy = py as f64 + 0.5 - y;
            if ddx * ddx + ddy * ddy > r * r {
                continue;
            }
            let t = ((ddx + ddy) / (2.0 * r) + 0.5).clamp(0.0, 1.0);
            canvas.set(px, py, mix(CUE_BALL, [150, 148, 140], t * 0.6));
        }
    }

    // The miscue limit, drawn **only while the tip is being placed**. It used
    // to sit there in grey the whole time, and a grey circle inside a white
    // ball reads as part of the ball rather than as a boundary — it was the
    // busiest thing on a panel whose job is to show one ball clearly. The
    // limit itself is unchanged: the tip still lives inside `TIP_FACE` of the
    // drawn ball whether or not the ring is there to say so.
    if view.mode == ShotMode::Spin {
        table_ui::ring(canvas, x, y, r * TIP_FACE, GUIDE);
    }

    // The tip mark, on the same magnified scale. Screen y grows downward while
    // `tip[1]` is "up the face", hence the negation — get this wrong and
    // follow looks like draw.
    let scale = r * TIP_FACE / MISCUE_LIMIT;
    let mark_x = x + view.tip[0] * scale;
    let mark_y = y - view.tip[1] * scale;
    canvas.disc(mark_x, mark_y, (r * 0.18).max(1.0), TIP_MARK);
}

/// Most of the room below the cue ball a full pull may use. The rest is the
/// butt of the cue, which has to stay on screen: a cue drawn back until it
/// vanishes off the bottom reads as no cue at all.
const PULL_SHARE: f64 = 0.72;

/// Pixels the cue travels for a full pull: one terminal row (two pixel rows)
/// per row of pointer travel, so the drawn cue follows the hand exactly — or,
/// on a panel too short for that, as much of the room as can be spared.
///
/// It used to be the room times the *speed*, which put a full pull in the
/// `light` band a tenth of the way back: the hand moved fourteen rows and the
/// cue barely moved, then jumped to the ball when the shot fired.
fn pull_travel(space: f64) -> f64 {
    (space * PULL_SHARE).min(PULL_ROWS * 2.0)
}

/// The cue, pointing up at the ball from below and drawn back by the pull.
fn draw_cue(canvas: &mut Canvas, view: &CueView, x: f64, cue_y: f64, cue_r: f64, h: f64) {
    // Scale the draw-back to the room below the ball, not to the ball's size.
    // Pulling back by a fixed multiple of the radius looks right on a tall
    // panel and slides the whole cue off the bottom of a short one — at which
    // point the player has no cue at exactly the moment they are aiming it.
    let rest_y = cue_y + cue_rest_offset(cue_r) + 1.0;
    let space = (h - rest_y).max(2.0);
    let travel = pull_travel(space);
    let pull = view.pull.clamp(0.0, 1.0) * travel;
    // Struck: the cue is thrown *through* where the ball was and left there
    // until the shot finishes playing. A cue that returned to rest the instant
    // the stroke registered gave the player nothing to tell a struck shot from
    // one that never took — which is exactly the moment they need telling.
    let tip_y = if view.follow_through {
        cue_y + cue_r * 0.2
    } else {
        rest_y + pull
    };
    let butt_y = h;
    if tip_y >= butt_y {
        return;
    }
    let shaft = if view.follow_through || view.mode.band().is_some() {
        CUE_WOOD
    } else {
        mix(CUE_WOOD, BACKDROP, 0.45)
    };
    // Tapered: one pixel at the tip, widening toward the butt.
    let mut y = tip_y;
    while y < butt_y {
        let t = (y - tip_y) / (butt_y - tip_y).max(1.0);
        let half = (t * cue_r * 0.35).round() as i32;
        let colour = if y < tip_y + 2.0 { CUE_TIP } else { shaft };
        for dx in -half..=half {
            canvas.set((x + dx as f64).round() as i32, y.round() as i32, colour);
        }
        y += 1.0;
    }
}

/// Readouts for the text row under the panel. Kept here so the wording and the
/// drawing cannot drift apart.
pub fn aim_label(azimuth: f64) -> String {
    let degrees = azimuth.to_degrees().rem_euclid(360.0);
    format!("aim {degrees:>5.1}°")
}

pub fn spin_label(tip: [f64; 2]) -> String {
    let side = match tip[0] {
        v if v > 0.02 => format!("{:.2} left", v.abs()),
        v if v < -0.02 => format!("{:.2} right", v.abs()),
        _ => "centre".to_string(),
    };
    let vert = match tip[1] {
        v if v > 0.02 => format!("{:.2} follow", v.abs()),
        v if v < -0.02 => format!("{:.2} draw", v.abs()),
        _ => "centre".to_string(),
    };
    format!("spin {side} · {vert}")
}

/// The stroke readout: which band is armed, how far the cue is drawn back
/// within it, and what that comes to in metres per second.
///
/// The bar is drawn against the *band*, not against the whole speed range, so
/// a full bar in `light` and a full bar in `strong` look the same and read
/// differently — which is the point of having bands at all.
pub fn power_label(power: f64, band: Option<PowerBand>, max_speed: f64) -> String {
    let within = band.map_or(power.clamp(0.0, 1.0), |band| band.within(power));
    let filled = (within * 10.0).round() as usize;
    let bar: String = "▓".repeat(filled) + &"░".repeat(10 - filled);
    let name = band.map(PowerBand::label).unwrap_or("stroke");
    format!("{name} {bar} {:.1} m/s", power.clamp(0.0, 1.0) * max_speed)
}
