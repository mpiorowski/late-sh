//! lava lamp: a lava lamp on its base. Wax pools on the floor of the glass,
//! blobs swell off it, climb, meet and part, and sink again, lit from the
//! bulb underneath. Ported from ascii.rest's `lava-lamp` (MIT, @bas3line).

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::piece::{TextFrame, js_round};

pub(crate) const COLS: usize = 30;
pub(crate) const ROWS: usize = 27;

const RAMP: [char; 9] = ['.', ':', '-', '=', '+', '*', '#', '%', '@'];
const CW: f64 = 0.6; // a cell's width in em; the wax is measured in em
const CH: f64 = 1.2; // a cell's height in em
const T0: f64 = 9.0; // seconds in: one blob near the cap, one climbing, one leaving the pool
const THRESH: f64 = 0.3;
const TOP: f64 = 8.6; // the highest a blob's centre goes, in em
const FLOOR: f64 = 24.4; // the glass floor, in em
/// Seconds per trip, where in the trip, side of the middle, sway, reach.
const BLOBS: [[f64; 5]; 3] = [
    [24.0, 0.42, -0.5, 0.7, 3.0],
    [27.0, 0.08, 0.6, 0.6, 2.7],
    [31.0, 0.74, 0.1, 0.8, 3.3],
];
/// The rows of the glass, its floor row last.
const GLASS: [usize; 2] = [4, 21];
const WAX: char = '~';

/// The lamp's half width at a row, in columns: cap, glass, waist, base.
fn half(r: f64) -> f64 {
    if r < 1.0 {
        1.9
    } else if r < 4.0 {
        0.9 + r
    } else if r < 20.0 {
        4.1 + 0.26 * (r - 4.0)
    } else if r < 21.0 {
        7.6
    } else {
        6.8 + (r - 21.0)
    }
}

fn fall(q: f64) -> f64 {
    if q < 1.0 { (1.0 - q) * (1.0 - q) } else { 0.0 }
}

fn smooth(e: f64) -> f64 {
    if e <= 0.0 {
        0.0
    } else if e >= 1.0 {
        1.0
    } else {
        e * e * (3.0 - 2.0 * e)
    }
}

/// The inside of the glass, in em from the middle, at a row.
fn room(r: f64) -> f64 {
    (half(r.max(GLASS[0] as f64).min((GLASS[1] - 1) as f64)) - 1.1) * CW
}

fn flip(glyph: char) -> char {
    match glyph {
        '/' => '\\',
        '\\' => '/',
        '▏' => '▕',
        '▕' => '▏',
        other => other,
    }
}

/// The lamp, drawn once: an edge per row from the half width, a thin bar
/// set where the glass actually stands in the cell, a slash where it leans.
/// Wax cells are `~` until a frame decides what fills them.
fn shell() -> &'static TextFrame {
    static SHELL: OnceLock<TextFrame> = OnceLock::new();
    SHELL.get_or_init(|| {
        let mid = COLS as f64 / 2.0;
        let mut shell = TextFrame::blank(COLS, ROWS);
        for r in 0..ROWS {
            let rf = r as f64;
            let x = mid - half(rf);
            let lean = half(rf + 0.5) - half(rf - 0.5);
            let c = x.floor() as usize;
            let glyph = if r == 0 {
                '_'
            } else if r < GLASS[0] || r >= GLASS[1] - 1 {
                if lean > 0.0 || r < GLASS[0] {
                    '/'
                } else {
                    '\\'
                }
            } else {
                let f = x - c as f64;
                if f < 0.34 {
                    '▏'
                } else if f < 0.67 {
                    '│'
                } else {
                    '▕'
                }
            };
            let row = r * COLS;
            if r == 0 {
                for k in c..COLS - c {
                    shell.cells[row + k] = '_';
                }
            }
            shell.cells[row + c] = glyph;
            shell.cells[row + COLS - 1 - c] = flip(glyph);
            // The cap's rim, the glass's floor sitting in the base, and the
            // base's foot.
            if r == 3 || r == 21 || r == ROWS - 1 {
                for k in c + 1..COLS - 1 - c {
                    shell.cells[row + k] = '_';
                }
            }
            if r >= GLASS[0] && r < GLASS[1] {
                for k in c + 1..COLS - 1 - c {
                    shell.cells[row + k] = WAX;
                }
            }
        }
        shell
    })
}

struct Ball {
    x: f64,
    y: f64,
    r2: f64,
    sx: f64,
    sy: f64,
}

/// The lamp at `t` seconds of play time.
pub(crate) fn frame(t: f64) -> TextFrame {
    let shell = shell();
    let mid = COLS as f64 / 2.0;
    let m = (0.5f64 * 0.5 + 0.85 * 0.85).sqrt();
    let (ly, lz) = (0.5 / m, 0.85 / m); // toward the light: below, and in front

    let tt = T0 + t;
    let mut balls: Vec<Ball> = BLOBS
        .iter()
        .map(|&[period, phase, side, sway, rad]| {
            let a = 2.0 * PI * (tt / period + phase);
            let y = TOP + ((FLOOR - TOP) * (1.0 + a.cos())) / 2.0;
            // Drawn out by the climb, round again at the turns.
            let stretch = 1.0 + 0.3 * a.sin().abs();
            let sx = stretch;
            let sy = 1.0 / (stretch * stretch);
            // Kept clear of the glass at the narrowest row it reaches, and
            // smaller where the glass is.
            let top = ((y - 0.67 * rad / sy.sqrt()) / CH).floor();
            let fit = room(top);
            let r = rad.min((fit * sx.sqrt()) / 0.85);
            let reach = (0.8 * r) / sx.sqrt();
            let x = side + sway * (a * 0.5 + phase * 9.0).sin();
            Ball {
                x: (reach - fit).max((fit - reach).min(x)),
                y,
                r2: r * r,
                sx,
                sy,
            }
        })
        .collect();
    // The pool: three low lumps either side of the middle, shifting their weight.
    for j in -1..=1 {
        let j = f64::from(j);
        let x = j * 2.5 + 0.35 * (tt * 0.31 + j * 2.1).sin();
        balls.push(Ball {
            x,
            y: FLOOR + 0.6 + 0.3 * (tt * 0.43 + j * 1.3).sin(),
            r2: 2.6 * 2.6,
            sx: 0.3,
            sy: 1.0,
        });
    }
    let floor_room = room((GLASS[1] - 1) as f64);
    let field = |x: f64, y: f64| {
        let mut f = 0.0;
        for b in &balls {
            let (dx, dy) = (x - b.x, y - b.y);
            f += fall((dx * dx * b.sx + dy * dy * b.sy) / b.r2);
        }
        // A meniscus: the pool climbs a little up the glass at either side.
        let edge = (x.abs() / floor_room).min(1.0);
        f + 0.55 * smooth((y - FLOOR + 0.9 + 2.0 * edge.powi(4)) / 1.1)
    };

    let mut fields = vec![0f64; ROWS * COLS];
    for r in GLASS[0]..GLASS[1] {
        for c in 0..COLS {
            if shell.at(c, r) == WAX {
                fields[r * COLS + c] = field((c as f64 + 0.5 - mid) * CW, (r as f64 + 0.5) * CH);
            }
        }
    }
    let wax = |r: usize, c: usize| shell.at(c, r) != WAX || fields[r * COLS + c] >= THRESH;

    let mut out = shell.clone();
    for r in 0..ROWS {
        for c in 0..COLS {
            if shell.at(c, r) != WAX {
                continue;
            }
            let k = r * COLS + c;
            let x = (c as f64 + 0.5 - mid) * CW;
            let y = (r as f64 + 0.5) * CH;
            let f = fields[k];
            if f < THRESH {
                out.cells[k] = ' ';
                continue;
            }
            // The wax as a dome over its outline, its slope from the field's.
            let gx = (field(x + 0.3, y) - field(x - 0.3, y)) / 0.6;
            let gy = (field(x, y + 0.3) - field(x, y - 0.3)) / 0.6;
            let kk = 0.8 / (f - THRESH + 0.04).sqrt();
            let (nx, ny) = (-gx * kk, -gy * kk);
            let lit = ((ny * ly + lz) / (nx * nx + ny * ny + 1.0).sqrt()).max(0.0);
            let warm = smooth((y - TOP) / (FLOOR - TOP)); // brighter nearer the bulb
            // Where the wax meets clear oil, an outline that follows the way
            // the edge faces.
            let up = !wax(r - 1, c);
            let dn = !wax(r + 1, c);
            let lf = !wax(r, c - 1);
            let rt = !wax(r, c + 1);
            out.cells[k] = match (up, dn, lf, rt) {
                (false, false, false, false) => {
                    let lit = (0.05 + 0.75 * lit + 0.2 * warm).min(1.0);
                    RAMP[1 + js_round(lit * (RAMP.len() - 2) as f64) as usize]
                }
                (true, true, true, _) => '(',
                (true, true, false, true) => ')',
                (true, true, false, false) => '-',
                (true, false, true, _) | (true, false, _, true) => '.',
                (true, false, false, false) => '-',
                (false, true, true, _) | (false, true, _, true) => '\'',
                (false, true, false, false) => '-',
                (false, false, true, true) => '|',
                (false, false, true, false) => '(',
                (false, false, false, true) => ')',
            };
        }
    }
    out
}

#[cfg(test)]
#[path = "lava_lamp_test.rs"]
mod lava_lamp_test;
