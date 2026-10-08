//! misty forest: morning in a pine forest. Ridge after ridge of pines
//! recedes into fog, each paler than the one in front, with mist lying in
//! sheets in the valleys between them. A low sun sits behind the farthest
//! trees and sends beams slanting down through the fog to a clearing on the
//! forest floor. The fog drifts, the beams shimmer, and motes of dust float
//! in the light. Ported from ascii.rest's `misty-forest` (MIT, @bas3line).
//!
//! Shaded in colour per cell on a square grid, layer by layer with the haze
//! of its depth. ascii.rest then draws it as a halftone, dot size for
//! brightness, ordered-dithered, in the palette colour nearest its hue;
//! [`dot`] is that step, kept for the golden frame, while the renderer
//! draws `Shade::ink` (`ui.rs`). The forest, the fog banks, the high cloud
//! and the beams are built once for the whole process; each frame drifts
//! the fog and the cloud across them, sways the beams and floats the motes.
//!
//! It is the slow piece (`piece::Cadence::Slow`): played at a crawl, a frame
//! moves a few cells of fog and beam, which is what makes it the
//! screensaver's default.

use std::f64::consts::PI;
use std::sync::OnceLock;

#[cfg(test)]
use super::piece::js_round;
use super::piece::{Shade, ShadedFrame, js_i32};

pub(crate) const COLS: usize = 200;
pub(crate) const ROWS: usize = 100;
const W: usize = COLS;
const H: usize = ROWS;
const WF: f64 = W as f64;
const HF: f64 = H as f64;
const N: usize = W * H;
const SUN: [f64; 2] = [146.0, 45.5];
const SUN_R: f64 = 3.4;

/// The colour behind the dots.
pub(crate) const GROUND: [u8; 3] = [0x09, 0x0f, 0x0e];
#[cfg(test)]
pub(crate) const PALETTE: [[u8; 3]; 32] = [
    // sunlight and the warm fog around it
    [0xff, 0xfb, 0xea],
    [0xff, 0xf0, 0xc8],
    [0xfb, 0xe2, 0xa6],
    [0xf2, 0xc9, 0x7e],
    [0xe0, 0xa9, 0x5e],
    [0xb9, 0x83, 0x4a],
    // cool fog, pale to sea-green
    [0xe4, 0xeb, 0xe6],
    [0xcb, 0xe0, 0xdc],
    [0xa8, 0xd2, 0xd0],
    [0x86, 0xc0, 0xc2],
    [0x68, 0xa7, 0xac],
    [0x4f, 0x8c, 0x93],
    [0x3b, 0x70, 0x78],
    // pines, from the haze to the dark in front of us
    [0x55, 0x75, 0x70],
    [0x45, 0x67, 0x61],
    [0x36, 0x59, 0x52],
    [0x2a, 0x4a, 0x44],
    [0x1f, 0x3c, 0x37],
    [0x17, 0x2f, 0x2b],
    [0x11, 0x23, 0x1f],
    // moss and bark where the light touches the floor
    [0xa4, 0xa3, 0x5a],
    [0x8a, 0x8f, 0x4c],
    [0x6d, 0x74, 0x40],
    [0x4d, 0x55, 0x30],
    [0x7a, 0x5a, 0x3a],
    [0x5b, 0x45, 0x32],
    // the clear sky above the fog, deepening overhead
    [0xa9, 0xc3, 0xcc],
    [0x8e, 0xad, 0xb8],
    [0x6f, 0x93, 0xa2],
    [0x53, 0x79, 0x8a],
    [0x3d, 0x61, 0x72],
    [0x2b, 0x4a, 0x5a],
];
#[cfg(test)]
const DOTS: [char; 4] = [' ', '·', '•', '●'];
#[cfg(test)]
const COVER: [f64; 4] = [0.0, 0.3, 0.6, 1.0];
#[cfg(test)]
const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// What a cell belongs to: the sky, one of the five ridges by index, the
/// forest floor under the nearest, or the giants in front of everything.
const SKY: i8 = -1;
const FLOOR: i8 = 5;
const GIANT: i8 = 6;

/// The ridges, far to near: [ridge row, its rise and fall, tree spacing,
/// tree heights from and to, haze, how fast its fog drifts, mist lying in
/// the valley below it, drifting fog in front of it, sunbeam in front of
/// it].
const LAYERS: [[f64; 10]; 5] = [
    [50.0, 6.0, 1.6, 1.0, 2.5, 0.48, 0.8, 1.0, 0.14, 0.6],
    [59.0, 6.0, 2.0, 3.0, 5.5, 0.3, 1.3, 0.8, 0.16, 0.8],
    [69.0, 7.0, 2.6, 4.0, 7.0, 0.13, 2.0, 0.66, 0.18, 0.9],
    [80.0, 6.0, 3.4, 5.0, 9.0, 0.03, 2.8, 0.44, 0.14, 1.0],
    [90.0, 1.5, 10.0, 9.0, 24.0, 0.03, 3.4, 0.0, 0.08, 0.6],
];
const NEAR: usize = LAYERS.len() - 1;

/// The fog banks wrap at this width so they can slide forever.
const FW: usize = 400;
/// Rows of high cloud, in the sky only.
const CH: usize = 46;
/// Angle bins round the sun, for the beams.
const RA: usize = 720;
/// Sunbeams: a handful of wide shafts through the gaps, [angle, half
/// width, strength].
const SHAFTS: [[f64; 3]; 7] = [
    [1.42, 0.05, 0.7],
    [1.66, 0.07, 1.0],
    [1.93, 0.05, 0.8],
    [2.18, 0.08, 1.0],
    [2.45, 0.05, 0.75],
    [2.7, 0.06, 0.9],
    [2.95, 0.04, 0.6],
];
const MOTES: usize = 110;

fn hash(x: f64, y: f64) -> f64 {
    let h = (js_i32(x) as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((js_i32(y) as u32).wrapping_mul(668_265_263));
    let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    f64::from(h ^ (h >> 16)) / 4_294_967_296.0
}

/// Value noise; a `period` other than zero wraps it along x.
fn noise(x: f64, y: f64, period: f64) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let (x0, x1) = match period != 0.0 {
        true => {
            let x0 = ((xi % period) + period) % period;
            (x0, (x0 + 1.0) % period)
        }
        false => (xi, xi + 1.0),
    };
    let a = hash(x0, yi);
    let b = hash(x1, yi);
    let c = hash(x0, yi + 1.0);
    let d = hash(x1, yi + 1.0);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f64, y: f64, octaves: u32, period: f64) -> f64 {
    let (mut s, mut n, mut amp, mut f) = (0.0, 0.0, 0.5, 1.0);
    for _ in 0..octaves {
        s += amp * noise(x * f, y * f, period * f);
        n += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    s / n
}

fn clamp(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

fn smooth(a: f64, b: f64, v: f64) -> f64 {
    let k = clamp((v - a) / (b - a));
    k * k * (3.0 - 2.0 * k)
}

fn mix(a: f64, b: f64, k: f64) -> f64 {
    a + (b - a) * k
}

// The colour of the fog: cool and green-grey, warming to cream by the sun.
fn fog_r(s: f64) -> f64 {
    mix(0.7, 1.05, s)
}
fn fog_g(s: f64) -> f64 {
    mix(0.92, 0.92, s)
}
fn fog_b(s: f64) -> f64 {
    mix(0.9, 0.6, s)
}

/// Everything that does not move: the forest shaded in its own light, and
/// per cell what the frame lays over it. The originals are `Float32Array`s,
/// so these hold `f32` and the frame reads them back as `f64`.
struct Land {
    sr: Vec<f32>,
    sg: Vec<f32>,
    sb: Vec<f32>,
    /// How much drifting fog shows in front of a cell, and how fast.
    fog_amt: Vec<f32>,
    fog_speed: Vec<f32>,
    /// How much of a sunbeam the air in front of a cell holds.
    ray_amt: Vec<f32>,
    /// Nearness to the sun, for the fog's warmth.
    sun_s: Vec<f32>,
    /// Where a beam that reaches the floor lights it.
    floor_lit: Vec<f32>,
    /// The dot floor, so the darkest air still shows.
    lift: Vec<f32>,
    /// Where high cloud can show, in the sky only.
    cloud_amt: Vec<f32>,
    /// A cell's angle bin round the sun, and its distance from it.
    abin: Vec<f32>,
    dist: Vec<f32>,
    /// Drifting fog banks, `FW` wide by `H`, wrapping.
    fog: Vec<f32>,
    /// Thin high cloud, `FW` wide by `CH`, wrapping.
    cloud: Vec<f32>,
    /// The shafts by angle bin, with a grain along each, and the slower
    /// pattern that slides across them.
    ray_a: Vec<f32>,
    ray_b: Vec<f32>,
    /// Dust in the air: [x, y, drift speed, bob phase, size].
    motes: Vec<[f64; 5]>,
}

fn land() -> &'static Land {
    static LAND: OnceLock<Land> = OnceLock::new();
    LAND.get_or_init(build_land)
}

/// Build the land now (`piece::warm`), so no frame has to.
pub(crate) fn warm() {
    land();
}

fn build_land() -> Land {
    // --- the layers, rasterised far to near so nearer ones cover farther --
    let mut layer = vec![SKY; N];
    // how much a cell sits on a silhouette's sunward rim
    let mut edge = vec![0f32; N];
    // rows below the layer's tree line
    let mut below = vec![0f32; N];
    // the lower edge of a tier of boughs, near trees only
    let mut tex = vec![0f32; N];
    // the row a cell's ridge rises from, for its valley mist
    let mut ground = vec![0f32; N];
    for (i, [y_row, amp, gap, h0, h1, ..]) in LAYERS.iter().enumerate() {
        let (y_row, amp, gap, h0, h1) = (*y_row, *amp, *gap, *h0, *h1);
        let fi = i as f64;
        // nearer ridges dip toward the sun, a valley opening onto the light
        let ridge = |x: f64| -> f64 {
            let sway = if i < 3 { 4.0 } else { 3.0 };
            let dip = match i > 0 && i < NEAR {
                true => (2.0 + fi * 2.0) * (-((x - SUN[0] - 4.0) / 38.0).powi(2)).exp(),
                false => 0.0,
            };
            y_row + amp * (fbm(x * (0.018 + fi * 0.003), fi * 13.0 + 2.0, 3, 0.0) - 0.5) * sway + dip
        };
        let base: Vec<f32> = (0..W).map(|x| ridge(x as f64) as f32).collect();
        let mut fill = vec![0u8; N];
        let mut spire = vec![0u8; N];
        for x in 0..W {
            let start = ridge(x as f64).floor().max(0.0) as usize;
            for r in start..H {
                fill[r * W + x] = 1;
            }
        }
        // pines: a spire of tiers, each tier flaring out and stepping back in
        let mut tx = -2.0 + hash(fi, 1.0) * gap;
        while tx < WF + 2.0 {
            let next = tx + gap * (0.7 + hash(js_i32(tx) as f64, fi + 3.0) * 0.7);
            // the nearest trees leave a clearing under the sun for the
            // light to land in
            if !(i == NEAR && tx > 92.0 && tx < 140.0) {
                let th = h0 + (h1 - h0) * hash(js_i32(tx * 7.0) as f64, fi + 5.0);
                let tip = ridge(tx) - th;
                let tier = 2.0 + th * 0.12;
                let r_end = HF.min(ridge(tx) + 2.0);
                let mut r = tip.floor().max(0.0);
                while r < r_end {
                    let d = r + 0.5 - tip;
                    if d >= 0.0 {
                        let saw = (d % tier) / tier;
                        let half = d * 0.3 * (0.6 + 0.5 * saw) + 0.35;
                        let x0 = (tx - half).floor().max(0.0) as i64;
                        let x1 = (tx + half).ceil().min(WF - 1.0) as i64;
                        for x in x0..=x1 {
                            let xf = x as f64;
                            let dx = (xf + 0.5 - tx).abs();
                            if dx > half {
                                continue;
                            }
                            let k = r as usize * W + x as usize;
                            fill[k] = 1;
                            spire[k] = 1;
                            if i == NEAR {
                                tex[k] = (smooth(0.62, 0.95, saw)
                                    * smooth(0.3, 0.85, dx / half)
                                    * (0.55 + 0.45 * hash(xf, r * 5.0)))
                                    as f32;
                            }
                        }
                    }
                    r += 1.0;
                }
            }
            tx = next;
        }
        // where the silhouette starts in each column, smoothed a little so
        // the mist line follows the forest rather than every single spire
        let mut top = vec![HF as f32; W];
        for x in 0..W {
            for r in 0..H {
                if fill[r * W + x] != 0 {
                    top[x] = r as f32;
                    break;
                }
            }
        }
        let line: Vec<f32> = (0..W)
            .map(|x| {
                let mut s = 0.0;
                for d in -3i64..=3 {
                    let xx = (x as i64 + d).clamp(0, W as i64 - 1) as usize;
                    s += f64::from(top[xx]);
                }
                (s / 7.0).max(f64::from(top[x])) as f32
            })
            .collect();
        let open = |xx: i64, rr: i64| {
            xx >= 0 && xx < W as i64 && (rr < 0 || fill[rr as usize * W + xx as usize] == 0)
        };
        for r in 0..H {
            for x in 0..W {
                let k = r * W + x;
                if fill[k] == 0 {
                    continue;
                }
                layer[k] = match i == NEAR && spire[k] == 0 {
                    true => FLOOR,
                    false => i as i8,
                };
                if i != NEAR {
                    tex[k] = 0.0;
                }
                below[k] = (r as f64 + 0.5 - f64::from(line[x])) as f32;
                ground[k] = base[x];
                // a rim where the sky (or a farther layer) shows beside or
                // above
                let (xi, ri) = (x as i64, r as i64);
                let toward: i64 = if (x as f64) < SUN[0] { 1 } else { -1 };
                edge[k] = if open(xi + toward, ri) || open(xi, ri - 1) {
                    1.0
                } else if open(xi + 2 * toward, ri) {
                    0.5
                } else {
                    0.0
                };
            }
        }
    }

    // --- what stands in front: a giant pine cut by the left of the frame,
    // and a smaller one at the right edge, so the frame is not a symmetric
    // curtain ---
    let mut giant = vec![0u8; N];
    const GIANTS: [[f64; 4]; 2] = [[9.0, -12.0, 15.0, 7.0], [192.0, 12.0, 7.0, 5.0]];
    for [gx, tip, spread, tier] in GIANTS {
        let mut r = tip.floor().max(0.0);
        while r < HF {
            let d = r + 0.5 - tip;
            let saw = (d % tier) / tier;
            // each tier of boughs sweeps out and droops, so the outline is
            // a stack of points rather than a straight edge where it meets
            // the frame
            let half = (spread * (0.5 + 0.5 * saw)).min(d * 0.34 * (0.45 + 0.7 * saw) + 0.5);
            let x0 = (gx - half).floor().max(0.0) as i64;
            let x1 = (gx + half).ceil().min(WF - 1.0) as i64;
            for x in x0..=x1 {
                let xf = x as f64;
                let dx = (xf + 0.5 - gx).abs();
                // the side toward the frame stays full, so no sliver of sky
                // shows there
                let outer = if (xf + 0.5 - gx) * (gx - WF / 2.0) > 0.0 { 1.6 } else { 1.0 };
                let ragged = half * outer * (0.82 + 0.3 * noise(xf * 0.5, r * 0.4, 0.0));
                if dx <= ragged || dx < 0.9 {
                    let k = r as usize * W + x as usize;
                    giant[k] = 1;
                    tex[k] = (smooth(0.66, 0.96, saw)
                        * smooth(0.25, 0.8, dx / ragged)
                        * (0.5 + 0.5 * hash(xf * 3.0, r))) as f32;
                }
            }
            r += 1.0;
        }
    }
    for k in 0..N {
        if giant[k] != 0 {
            layer[k] = GIANT;
            below[k] = 0.0;
        }
    }
    // a rim two cells deep on the side that faces the sun
    {
        let open = |xx: i64, rr: i64| {
            xx >= 0 && xx < W as i64 && (rr < 0 || giant[rr as usize * W + xx as usize] == 0)
        };
        for r in 0..H {
            for x in 0..W {
                let k = r * W + x;
                if giant[k] == 0 {
                    continue;
                }
                let (xi, ri) = (x as i64, r as i64);
                let toward: i64 = if (x as f64) < SUN[0] { 1 } else { -1 };
                edge[k] = if open(xi + toward, ri) || open(xi, ri - 1) {
                    1.0
                } else if open(xi + 2 * toward, ri) {
                    0.6
                } else {
                    0.0
                };
            }
        }
    }

    // --- static colour ------------------------------------------------------
    let mut sr = vec![0f32; N];
    let mut sg = vec![0f32; N];
    let mut sb = vec![0f32; N];
    let mut fog_amt = vec![0f32; N];
    let mut fog_speed = vec![0f32; N];
    let mut ray_amt = vec![0f32; N];
    let mut sun_s = vec![0f32; N];
    let mut floor_lit = vec![0f32; N];
    let mut lift = vec![0f32; N];
    let mut cloud_amt = vec![0f32; N];
    let mut abin = vec![0f32; N];
    let mut dist = vec![0f32; N];
    let raf = RA as f64;
    for r in 0..H {
        for x in 0..W {
            let k = r * W + x;
            let xf = x as f64;
            let y = r as f64 + 0.5;
            let l = layer[k];
            let dx = xf + 0.5 - SUN[0];
            let dy = y - SUN[1];
            let ang = dy.atan2(dx);
            abin[k] = (((ang / (PI * 2.0)) * raf + raf) % raf) as f32;
            let d = (dx * dx + dy * dy).sqrt();
            dist[k] = d as f32;
            let s = (-(dx * dx + dy * dy * 1.96).sqrt() / 30.0).exp();
            sun_s[k] = s as f32;
            // a soft warm bloom over a wide radius, in the air and the fog
            let bloom = (-d / 16.0).exp() * 0.25 + (-d / 6.0).exp() * 0.1;
            let (mut cr, mut cg, mut cb);
            let mut ray;
            if l == SKY {
                // sky: deep overhead, paling to the fog band low down, warm
                // by the sun
                let v = clamp(y / 52.0);
                let p = v.powf(1.6);
                let veil = 0.95 + 0.1 * fbm(xf * 0.04, y * 0.08, 3, 0.0);
                cr = mix(0.065, 0.36, p) * veil;
                cg = mix(0.14, 0.58, p) * veil;
                cb = mix(0.17, 0.62, p) * veil;
                // the fog band the far ridge stands in
                let band =
                    0.75 * smooth(28.0, 50.0, y) * (0.55 + 0.75 * fbm(xf * 0.025, y * 0.16, 3, 0.0));
                cr = mix(cr, fog_r(0.0), band);
                cg = mix(cg, fog_g(0.0), band);
                cb = mix(cb, fog_b(0.0), band);
                // warm in hue round the sun, but falling off in brightness,
                // so the disc stands in a halo rather than a flat blaze
                let kw = s * 0.55;
                let kb = 0.5 + 0.5 * (-d / 9.0).exp();
                cr = mix(cr, fog_r(s) * kb, kw);
                cg = mix(cg, fog_g(s) * kb, kw);
                cb = mix(cb, fog_b(s) * kb, kw);
                let glow = (-d / 3.0).exp() * 0.3 + (-d / 10.0).exp() * 0.18;
                cr += glow + bloom;
                cg += glow * 0.9 + bloom * 0.78;
                cb += glow * 0.66 + bloom * 0.45;
                // the disc itself, a little brighter in the middle
                let disc = smooth(SUN_R + 0.6, SUN_R - 0.4, d);
                cr = mix(cr, 1.3, disc);
                cg = mix(cg, 1.24, disc);
                cb = mix(cb, 1.08, disc);
                fog_amt[k] = (0.3 * smooth(30.0, 50.0, y)) as f32;
                fog_speed[k] = 0.4;
                cloud_amt[k] = (0.75
                    * smooth(5.0, 15.0, y)
                    * smooth(44.0, 28.0, y)
                    * smooth(SUN_R + 2.0, SUN_R + 8.0, d)) as f32;
                ray = 0.35 * smooth(26.0, 46.0, y);
                lift[k] = 0.04;
            } else if l <= NEAR as i8 - 1 {
                let lf = f64::from(l);
                let [.., haze, speed, m, drift, beam] = LAYERS[l as usize];
                // pine: dark teal, paling with distance; mist pooled just
                // under the tree line, and a sheet of it lying along the
                // valley floor below
                let below = f64::from(below[k]);
                let pool = smooth(2.5, 8.0 + lf * 1.5, below) * smooth(20.0, 11.0, below);
                // the sheet follows the lie of the land, smoothly, so it
                // never streaks
                let sheet = (-((y - (f64::from(ground[k]) + 5.0)) / 3.0).powi(2)).exp()
                    * (0.75 + 0.5 * fbm(xf * 0.035, lf * 7.3, 3, 0.0));
                let mist = clamp((pool * 0.5).max(sheet) * m);
                let h = clamp(haze + (1.0 - haze) * mist);
                let veil = 0.9 + 0.2 * fbm(xf * 0.06, y * 0.12, 3, 0.0);
                let (pr, pg, pb) = (0.02, 0.075, 0.07);
                cr = mix(pr, fog_r(s) * veil, h);
                cg = mix(pg, fog_g(s) * veil, h);
                cb = mix(pb, fog_b(s) * veil, h);
                cr += bloom * h;
                cg += bloom * 0.78 * h;
                cb += bloom * 0.45 * h;
                let rim = f64::from(edge[k]) * s * (0.2 + 0.6 * (1.0 - haze));
                cr += rim * 0.95;
                cg += rim * 0.8;
                cb += rim * 0.45;
                fog_amt[k] = drift as f32;
                fog_speed[k] = speed as f32;
                // the trees stop most of the light; the beams show in the
                // mist between (far off there is more air in front of them
                // to hold the light)
                ray = beam
                    * match l < 3 {
                        true => 0.45 + 0.55 * clamp(mist / m),
                        false => 0.25 + 0.75 * clamp(mist / m),
                    };
                lift[k] = 0.03;
            } else if l == NEAR as i8 {
                // the nearest pines: near-black, a faint teal on each tier's
                // lower edge (the body itself stays black, or the dither
                // would screen it evenly)
                let g = f64::from(tex[k]) * 0.12;
                cr = 0.001 + g * 0.35;
                cg = 0.004 + g;
                cb = 0.004 + g * 0.9;
                let rim = f64::from(edge[k]) * (0.12 + 0.6 * s);
                cr += rim * 0.9;
                cg += rim * 0.7;
                cb += rim * 0.38;
                fog_amt[k] = LAYERS[NEAR][8] as f32;
                fog_speed[k] = LAYERS[NEAR][6] as f32;
                ray = 0.12;
                lift[k] = 0.0;
            } else if l == FLOOR {
                // the forest floor: moss in clumps, sparse, fading out
                // toward the frame
                let clump = smooth(0.42, 0.72, fbm(xf * 0.09, y * 0.4, 3, 0.0));
                let speck = if hash(xf * 7.0, r as f64 * 11.0) > 0.55 { 1.0 } else { 0.35 };
                let m = (0.06 + 0.3 * clump) * speck * smooth(HF + 2.0, HF - 8.0, y);
                cr = 0.02 + m * 0.6;
                cg = 0.03 + m * 0.62;
                cb = 0.02 + m * 0.3;
                fog_amt[k] = 0.06;
                fog_speed[k] = 3.4;
                ray = 0.0;
                // dappled patches the beams can land on
                floor_lit[k] = (smooth(0.32, 0.56, fbm(xf * 0.06 + 3.0, y * 0.24, 3, 0.0))
                    * smooth(HF + 3.0, HF - 6.0, y)) as f32;
                lift[k] = 0.0;
            } else {
                // the giants: near-black needles, the tiers drawn by a faint
                // teal edge, a warm rim two cells deep toward the light
                let g = f64::from(tex[k]) * 0.15;
                cr = 0.001 + g * 0.35;
                cg = 0.004 + g;
                cb = 0.004 + g * 0.9;
                // warm on the side near the sun, a cool fog-lit edge far
                // from it
                let warm = (-d / 50.0).exp();
                let rim = f64::from(edge[k]) * (0.13 + 0.55 * warm);
                cr += rim * mix(0.4, 0.95, warm);
                cg += rim * mix(0.75, 0.66, warm);
                cb += rim * mix(0.72, 0.32, warm);
                fog_amt[k] = 0.04;
                fog_speed[k] = 4.0;
                ray = 0.04;
                lift[k] = 0.0;
            }
            // the beams fan out mostly down and to the west, the way the
            // gaps face
            ray *= 0.2 + 0.8 * smooth(1.25, 1.75, ang) * smooth(3.1, 2.6, ang);
            ray_amt[k] = ray as f32;
            sr[k] = cr as f32;
            sg[k] = cg as f32;
            sb[k] = cb as f32;
        }
    }

    // drifting fog banks: wide soft noise that wraps so it can slide forever
    let fwf = FW as f64;
    let mut fog = vec![0f32; FW * H];
    for r in 0..H {
        for u in 0..FW {
            fog[r * FW + u] =
                smooth(0.42, 0.75, fbm(u as f64 * 0.022, r as f64 * 0.09, 4, fwf * 0.022)) as f32;
        }
    }

    // thin high cloud, long and flat, lit from below by the low sun
    let mut cloud = vec![0f32; FW * CH];
    for r in 0..CH {
        for u in 0..FW {
            let (uf, rf) = (u as f64, r as f64);
            let q = fbm(uf * 0.01, rf * 0.05, 2, fwf * 0.01);
            cloud[r * FW + u] =
                smooth(0.44, 0.68, fbm(uf * 0.016 + q * 1.5, rf * 0.17, 4, fwf * 0.016)) as f32;
        }
    }

    // sunbeams: the shafts, with a faint grain along each, and a slower
    // pattern sliding across them so they brighten and fade
    let mut ray_a = vec![0f32; RA];
    let mut ray_b = vec![0f32; RA];
    for i in 0..RA {
        let fi = i as f64;
        let a = (fi / raf) * PI * 2.0;
        let mut v: f64 = 0.0;
        for [c, w, st] in SHAFTS {
            v = v.max(st * smooth(w, w * 0.35, (a - c).abs()));
        }
        ray_a[i] = (v * (0.8 + 0.2 * fbm(fi * 0.4, 3.1, 2, raf * 0.4))) as f32;
        ray_b[i] = smooth(0.4, 0.7, fbm(fi * 0.03, 8.7, 2, raf * 0.03)) as f32;
    }

    // dust in the air
    let motes = (0..MOTES)
        .map(|i| {
            let fi = i as f64;
            [
                hash(fi, 1.0) * WF,
                50.0 + hash(fi, 2.0) * 44.0,
                0.3 + hash(fi, 3.0) * 0.8,
                hash(fi, 4.0) * 6.28,
                hash(fi, 5.0),
            ]
        })
        .collect();

    Land {
        sr,
        sg,
        sb,
        fog_amt,
        fog_speed,
        ray_amt,
        sun_s,
        floor_lit,
        lift,
        cloud_amt,
        abin,
        dist,
        fog,
        cloud,
        ray_a,
        ray_b,
        motes,
    }
}

/// A wrapping read of a `FW`-wide row at `u`, interpolated.
fn drift(bank: &[f32], row: usize, u: f64) -> f64 {
    let ui = u.floor();
    let uf = u - ui;
    let ui = ui as i64;
    let at = |i: i64| f64::from(bank[row * FW + i.rem_euclid(FW as i64) as usize]);
    let (f0, f1) = (at(ui), at(ui + 1));
    f0 + (f1 - f0) * uf
}

/// The scene at `t` seconds of play time, shaded per square cell, as the
/// original plays it: one clock for everything.
pub(crate) fn frame(t: f64) -> ShadedFrame {
    shade(t, t)
}

/// The scene with the air (fog, cloud, motes) at `t` and the sun's beams at
/// `beams` seconds of play time.
pub(crate) fn shade(t: f64, beams: f64) -> ShadedFrame {
    let land = land();
    let mut mote = vec![0f32; N];
    for [mx, my, sp, ph, sz] in &land.motes {
        let x = ((((mx + t * sp + 2.5 * (t * 0.4 + ph).sin()) % WF) + WF) % WF).floor();
        let y = (my + 2.0 * (t * 0.3 + ph * 1.7).sin() - ((t * sp * 0.2) % 6.0)).floor();
        if y < 0.0 || y >= HF {
            continue;
        }
        mote[y as usize * W + x as usize] = (0.5 + 0.5 * sz) as f32;
    }
    // the beams hold their places (the gaps in the trees do not move) and
    // only sway a hair; a second, slower pattern drifts across them
    let shift_a = 2.0 * (beams * 0.35).sin();
    let shift_b = -beams * 1.6;
    let pulse = 0.88 + 0.12 * (beams * 0.7).sin();
    let ray = |arr: &[f32], bin: f64| f64::from(arr[(bin.floor() as i64).rem_euclid(RA as i64) as usize]);

    let mut cells = Vec::with_capacity(N);
    for r in 0..H {
        for x in 0..W {
            let k = r * W + x;
            let (mut cr, mut cg, mut cb) = (
                f64::from(land.sr[k]),
                f64::from(land.sg[k]),
                f64::from(land.sb[k]),
            );
            let s = f64::from(land.sun_s[k]);
            let dist = f64::from(land.dist[k]);

            // the fog banks drift, nearer ones faster
            let fa = f64::from(land.fog_amt[k]);
            if fa > 0.01 {
                let a = drift(&land.fog, r, x as f64 + t * f64::from(land.fog_speed[k])) * fa;
                cr = mix(cr, fog_r(s), a);
                cg = mix(cg, fog_g(s), a);
                cb = mix(cb, fog_b(s), a);
            }

            let ca = f64::from(land.cloud_amt[k]);
            if ca > 0.01 {
                let a = drift(&land.cloud, r, x as f64 + t * 0.6) * ca;
                if a > 0.005 {
                    // cool grey-teal, warming to gold on the undersides
                    // near the sun
                    let w = (-dist / 40.0).exp();
                    cr = mix(cr, mix(0.26, 0.95, w), a);
                    cg = mix(cg, mix(0.38, 0.72, w), a);
                    cb = mix(cb, mix(0.42, 0.45, w), a);
                }
            }

            // the beams: brightest near the sun, fading with distance
            let ra = f64::from(land.ray_amt[k]);
            let fl = f64::from(land.floor_lit[k]);
            let mut beam = 0.0;
            if ra > 0.01 || fl > 0.01 {
                let ai = f64::from(land.abin[k]);
                beam = ray(&land.ray_a, ai + shift_a)
                    * (0.6 + 0.4 * ray(&land.ray_b, ai + shift_b))
                    * pulse;
            }
            if ra > 0.01 && dist > SUN_R {
                // lit shafts brighten the air, the shadows between them dim
                // it
                let fall = (-dist / 80.0).exp() * smooth(SUN_R, 12.0, dist);
                let b = (beam - 0.3) * fall * ra * 2.2;
                if b > 0.0 {
                    cr += b * 1.05;
                    cg += b * 0.8;
                    cb += b * 0.42;
                } else {
                    let dim = 1.0 + b;
                    cr *= dim;
                    cg *= dim;
                    cb *= dim;
                }
            }
            if fl > 0.01 {
                // a patch of sun on the moss where a beam lands
                let b = beam * fl * 1.1;
                cr += b;
                cg += b * 0.78;
                cb += b * 0.4;
            }

            let mut floor = f64::from(land.lift[k]);
            if mote[k] != 0.0 && dist > 6.0 {
                // a mote shows up where a beam catches it
                let lit = ray(&land.ray_a, f64::from(land.abin[k]) + shift_a) * (-dist / 90.0).exp();
                let v = f64::from(mote[k]) * (0.1 + 1.6 * lit) * (ra * 2.0).min(1.0);
                if v > 0.18 {
                    cr = cr.max(v * 1.05);
                    cg = cg.max(v * 0.97);
                    cb = cb.max(v * 0.7);
                    floor = 0.3;
                }
            }

            let peak = cr.max(cg).max(cb).max(1e-4);
            let level = clamp(floor + (1.0 - floor) * peak.powf(0.9) * 0.95);
            // the largest dot's ink: the colour at full brightness
            cells.push(Shade {
                level,
                rgb: [cr, cg, cb],
                ink: [clamp(cr / peak), clamp(cg / peak), clamp(cb / peak)],
            });
        }
    }
    ShadedFrame {
        cols: W,
        rows: H,
        ground: GROUND,
        cells,
    }
}

/// One cell of the halftone as the original draws it: its dot and its
/// palette colour, dithered by where the cell sits (`x`, `y`). What the
/// golden frame is compared against; the terminal draws `Shade::ink`.
#[cfg(test)]
pub(crate) fn dot(shade: Shade, x: usize, y: usize) -> (char, u8) {
    let bayer = f64::from(BAYER[(y & 3) * 4 + (x & 3)]) / 16.0 - 0.47;
    let step = js_round(shade.level * 3.0 + bayer).clamp(0.0, 3.0) as usize;
    let [cr, cg, cb] = shade.rgb;
    let peak = cr.max(cg).max(cb).max(1e-4);
    let want = match step {
        0 => 0.0,
        _ => ((shade.level + 0.06) / COVER[step]).min(1.0),
    };
    let s = (0.3 + 0.7 * want) / peak;
    (
        DOTS[step],
        nearest(clamp(cr * s), clamp(cg * s), clamp(cb * s)),
    )
}

/// The palette colour nearest a colour, weighted toward green the way the
/// eye is.
#[cfg(test)]
pub(crate) fn nearest(r: f64, g: f64, b: f64) -> u8 {
    let mut best = 0usize;
    let mut best_d = f64::MAX;
    for (i, [pr, pg, pb]) in PALETTE.iter().enumerate() {
        let dr = f64::from(*pr) / 255.0 - r;
        let dg = f64::from(*pg) / 255.0 - g;
        let db = f64::from(*pb) / 255.0 - b;
        let d = 0.3 * dr * dr + 0.5 * dg * dg + 0.2 * db * db;
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    best as u8
}
