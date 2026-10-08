//! donut: a torus turning on two axes, lit from the upper left. After Andy
//! Sloane's donut.c, with the ring sized so it never clips at any angle.
//! Ported from ascii.rest's `donut` (MIT, @bas3line).

use super::piece::{TextFrame, js_round};

pub(crate) const COLS: usize = 40;
pub(crate) const ROWS: usize = 22;

const RAMP: [char; 12] = ['.', ',', '-', '~', ':', ';', '=', '!', '*', '#', '$', '@'];
const R1: f64 = 1.0; // tube radius
const R2: f64 = 2.0; // ring radius
const K2: f64 = 6.0; // eye to centre
const ASPECT: f64 = 0.5; // a cell is about twice as tall as it is wide

/// The donut at `t` seconds of play time.
pub(crate) fn frame(t: f64) -> TextFrame {
    let cols = COLS as f64;
    let rows = ROWS as f64;
    let k1 = (cols - 2.0) / 2.0 / ((R1 + R2) / (K2 * K2 - (R1 + R2).powi(2)).sqrt());
    let m = (0.4f64 * 0.4 + 1.0 + 1.0).sqrt();
    let (lx, ly, lz) = (-0.4 / m, 1.0 / m, -1.0 / m);
    let mut out = TextFrame::blank(COLS, ROWS);
    let mut depth = vec![0f32; COLS * ROWS];

    let a = 1.0 + t * 0.8;
    let b = 1.0 + t * 0.35;
    let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
    let mut th = 0.0f64;
    while th < 6.283 {
        let (ct, st) = (th.cos(), th.sin());
        let mut ph = 0.0f64;
        while ph < 6.283 {
            let (cp, sp) = (ph.cos(), ph.sin());
            let h = R2 + R1 * ct;
            let x = h * (cb * cp + sa * sb * sp) - R1 * st * ca * sb;
            let y = h * (sb * cp - sa * cb * sp) + R1 * st * ca * cb;
            let ooz = 1.0 / (K2 + ca * h * sp + R1 * st * sa);
            let col = (cols / 2.0 + k1 * ooz * x).floor();
            let row = (rows / 2.0 - k1 * ASPECT * ooz * y).floor();
            if col >= 0.0 && col < cols && row >= 0.0 && row < rows {
                let k = col as usize + row as usize * COLS;
                if ooz > f64::from(depth[k]) {
                    depth[k] = ooz as f32;
                    // The surface normal is the same rotation applied to the
                    // tube's circle.
                    let nx = ct * (cb * cp + sa * sb * sp) - st * ca * sb;
                    let ny = ct * (sb * cp - sa * cb * sp) + st * ca * cb;
                    let nz = ca * ct * sp + st * sa;
                    let lit = (nx * lx + ny * ly + nz * lz).max(0.0);
                    let i = js_round(lit * (RAMP.len() - 1) as f64) as usize;
                    out.cells[k] = RAMP[i];
                }
            }
            ph += 0.03;
        }
        th += 0.07;
    }
    out
}

#[cfg(test)]
#[path = "donut_test.rs"]
mod donut_test;
