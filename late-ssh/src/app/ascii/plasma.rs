//! plasma: the old demo-scene effect. Four sine fields, one a set of rings
//! round a wandering centre, summed and read as soft blobs of density.
//! Ported from ascii.rest's `plasma` (MIT, @bas3line); ascii.rest draws it at
//! 64x22, here it fills whatever area it is given.

use std::f64::consts::PI;

use super::piece::TextFrame;

const RAMP: [char; 12] = ['.', ',', '-', '~', ':', ';', '=', '+', '*', '#', '%', '@'];
const PERIOD: f64 = 30.0; // seconds; every term turns a whole number of times in one loop
const GAIN: f64 = 0.32; // ramp sweeps per unit of the summed field: low, so the blobs are broad

/// The field at `t` seconds, `cols` x `rows` cells, centred on the area.
pub(crate) fn frame(t: f64, cols: usize, rows: usize) -> TextFrame {
    let n = RAMP.len();
    let w = (2.0 * PI) / PERIOD;
    // Cell centres in cell widths, the rows stretched to their true height.
    let xs: Vec<f32> = (0..cols)
        .map(|c| (c as f64 + 0.5 - cols as f64 / 2.0) as f32)
        .collect();
    let ys: Vec<f32> = (0..rows)
        .map(|r| ((r as f64 + 0.5 - rows as f64 / 2.0) * 2.0) as f32)
        .collect();

    let a = w * t;
    let (ca, sa) = (a.cos(), a.sin());
    let (cx, cy) = (14.0 * a.sin(), 9.0 * (2.0 * a).cos()); // the rings' centre
    let mut out = TextFrame::blank(cols, rows);
    for (r, y) in ys.iter().enumerate() {
        let y = f64::from(*y);
        for (c, x) in xs.iter().enumerate() {
            let x = f64::from(*x);
            let mut v = (x * 0.11 + 3.0 * a).sin();
            v += (y * 0.13 - 2.0 * a).sin();
            v += ((x * ca + y * sa) * 0.09 + 4.0 * a).sin();
            v += ((x - cx).hypot(y - cy) * 0.17 - 5.0 * a).sin();
            // Up the ramp and back down, evenly, drifting through it once a loop.
            let u = v * GAIN + a / PI;
            let k = (u - 2.0 * (u / 2.0 + 0.5).floor()).abs();
            let i = ((k * n as f64).floor() as usize).min(n - 1);
            out.cells[r * cols + c] = RAMP[i];
        }
    }
    out
}

#[cfg(test)]
#[path = "plasma_test.rs"]
mod plasma_test;
