//! alpine dawn: jagged snow peaks catch the first pink light on their east
//! faces while their flanks stay in blue shadow. Mist pools along the far
//! shore and a still lake mirrors it all. The light warms toward gold as the
//! sun clears the ridge, the mist drifts, and slow ripples cross the water.
//! Ported from ascii.rest's `alpine-dawn` (MIT, @bas3line).
//!
//! The range is a heightfield, raymarched once for the whole process from a
//! camera just above the water: each cell keeps its depth, height, sunlight
//! (with cast shadows) and snow cover, so a frame only re-tints it. The lake
//! looks up the picture above it along each cell's reflected ray. Shaded in
//! colour per cell on a square grid; [`dot`] is the original's halftone of
//! it, the renderer draws `Shade::ink` (`ui.rs`).

use std::sync::OnceLock;

use super::piece::{Shade, ShadedFrame, js_i32, js_round};

pub(crate) const COLS: usize = 200;
pub(crate) const ROWS: usize = 100;
const W: usize = COLS;
const H: usize = ROWS;
const N: usize = W * H;
/// Tangent of half the field of view, across the width.
const K: f64 = 0.62;
/// Eye level, in rows.
const HZ: f64 = 56.5;
/// Camera height above the water.
const CAM: f64 = 1.5;
/// Distance to the far shore.
const SHORE_Z: f64 = 46.0;
/// First row of open water.
const SHORE: usize = 62;
const SHORE_F: f64 = SHORE as f64;
const SUN: [f64; 2] = [151.0, 50.0];

/// The colour behind the dots.
pub(crate) const GROUND: [u8; 3] = [0x09, 0x0c, 0x18];
#[cfg(test)]
pub(crate) const PALETTE: [[u8; 3]; 46] = [
    [0x0e, 0x14, 0x30],
    [0x15, 0x1d, 0x40],
    [0x1d, 0x27, 0x52],
    [0x26, 0x33, 0x65],
    [0x31, 0x41, 0x79],
    [0x3e, 0x50, 0x8c],
    [0x4f, 0x62, 0xa0],
    [0x65, 0x77, 0xb3],
    [0x80, 0x90, 0xc4],
    [0x9e, 0xaa, 0xd3],
    [0xbe, 0xc6, 0xe2],
    [0x4b, 0x3e, 0x6c],
    [0x6a, 0x54, 0x82],
    [0x8c, 0x6a, 0x92],
    [0xb0, 0x82, 0x9c],
    [0xcf, 0x96, 0xa4],
    [0xe8, 0xa9, 0xa8],
    [0xf5, 0xbc, 0xaa],
    [0xff, 0xd0, 0xb0],
    [0xff, 0xe2, 0xc2],
    [0xff, 0xf1, 0xe0],
    [0xfd, 0xfa, 0xf6],
    [0xff, 0xc8, 0x87],
    [0xf7, 0xa9, 0x65],
    [0xf2, 0xa0, 0x8f],
    [0xe5, 0x8a, 0x87],
    [0xf8, 0xb5, 0x9d],
    [0xd9, 0x7b, 0x7e],
    [0x7a, 0x4c, 0x4a],
    [0xa5, 0x65, 0x4f],
    [0x52, 0x3a, 0x4a],
    [0x1b, 0x20, 0x34],
    [0x26, 0x2c, 0x45],
    [0x36, 0x3c, 0x59],
    [0x0a, 0x14, 0x18],
    [0x0f, 0x1f, 0x24],
    [0x16, 0x2a, 0x2f],
    [0x20, 0x3a, 0x3c],
    [0xa3, 0xa7, 0xc6],
    [0xc6, 0xc3, 0xd8],
    [0xe0, 0xd4, 0xdc],
    [0x5a, 0x4f, 0x7e],
    [0x7b, 0x6c, 0x9c],
    [0x9a, 0x8c, 0xb6],
    [0xb8, 0xa8, 0xc8],
    [0xd8, 0xbc, 0xcb],
];

fn hash(x: f64, y: f64) -> f64 {
    let h = (js_i32(x) as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((js_i32(y) as u32).wrapping_mul(668_265_263));
    let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    f64::from(h ^ (h >> 16)) / 4_294_967_296.0
}

/// Value noise; `period` > 0 wraps it along x.
fn noise(x: f64, y: f64, period: f64) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let (mut x0, mut x1) = (xi, xi + 1.0);
    if period != 0.0 {
        x0 = ((xi % period) + period) % period;
        x1 = (x0 + 1.0) % period;
    }
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

/// Sharp crests where plain noise crosses its middle: rock ribs and couloirs.
fn ridged(x: f64, y: f64, octaves: u32) -> f64 {
    let (mut s, mut n, mut amp, mut f) = (0.0, 0.0, 0.5, 1.0);
    for i in 0..octaves {
        let v = 1.0 - (2.0 * noise(x * f + f64::from(i) * 17.3, y * f, 0.0) - 1.0).abs();
        s += amp * v * v;
        n += amp;
        amp *= 0.5;
        f *= 2.1;
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

/// A peak as a pyramid, turned a little: [x, z, height, spread, cos, sin].
type Peak = [f64; 6];

/// Peaks placed by where their summits should land on screen:
/// [column, row, distance, spread, turn].
const PEAK_SPOTS: [[f64; 5]; 7] = [
    [66.0, 11.0, 140.0, 0.95, 0.3],
    [38.0, 26.0, 115.0, 1.1, -0.15],
    [116.0, 22.0, 175.0, 0.9, 0.2],
    [92.0, 33.0, 150.0, 1.0, 0.1],
    [96.0, 25.0, 340.0, 1.1, 0.4],
    [180.0, 38.0, 200.0, 1.5, -0.1],
    [8.0, 30.0, 160.0, 1.2, 0.25],
];

fn peaks() -> [Peak; 7] {
    PEAK_SPOTS.map(|[sx, row, z, f, a]| {
        let h = CAM + ((HZ - row) / 100.0) * K * z - 1.5;
        [
            ((sx - 100.0) / 100.0) * K * z,
            z,
            h,
            h * f,
            a.cos(),
            a.sin(),
        ]
    })
}

fn terrain(peaks: &[Peak; 7], x: f64, z: f64) -> f64 {
    let mut h = 0.0;
    for &[px, pz, ph, pr, c, s] in peaks {
        let (dx, dz) = (x - px, z - pz);
        let rx = dx * c - dz * s;
        let rz = dx * s + dz * c;
        let v = ph * (1.0 - (rx.abs() + rz.abs()) / pr);
        if v > h {
            h = v;
        }
    }
    let hills = (1.0 + 3.0 * fbm(x * 0.04, z * 0.04, 3, 0.0)) * smooth(SHORE_Z, SHORE_Z + 15.0, z);
    if hills > h {
        h = hills;
    }
    // crags, deeper on the high ground
    h += ((ridged(x * 0.06, z * 0.06, 3) - 0.45) * 6.0
        + (ridged(x * 0.2, z * 0.2, 2) - 0.45) * 1.6)
        * smooth(4.0, 22.0, h);
    h
}

/// Distance along a ray from height `oy` with slope `v` and spread `u` to
/// the terrain beyond the shore, or 0 when it reaches the sky.
fn march(peaks: &[Peak; 7], u: f64, v: f64, oy: f64) -> f64 {
    let mut z = SHORE_Z;
    let mut prev = z;
    let mut i = 0;
    while i < 260 && z < 520.0 {
        let gap = oy + v * z - terrain(peaks, u * z, z);
        if gap < 0.0 {
            let (mut a, mut b) = (prev, z);
            for _ in 0..7 {
                let m = (a + b) / 2.0;
                if oy + v * m - terrain(peaks, u * m, m) < 0.0 {
                    b = m;
                } else {
                    a = m;
                }
            }
            return b;
        }
        prev = z;
        z += (gap * 0.45).max(0.35) + z * 0.002;
        i += 1;
    }
    0.0
}

/// Rows above the open water.
const SR: usize = SHORE;
/// Rows of open water.
const LR: usize = H - SHORE;
/// The mist sheet: a wrapping strip of `MW` columns from row `M0`.
const MW: usize = 480;
const M0: usize = 36;
const MR: usize = SHORE + 2 - M0;
/// The high cloud: a wrapping strip of `CW` columns over `CR` rows.
const CW: usize = 640;
const CR: usize = 34;

/// Near pines: [x, tip row, scale].
const PINES: [[f64; 3]; 6] = [
    [9.0, 5.0, 1.15],
    [20.0, 38.0, 0.85],
    [32.0, 66.0, 0.5],
    [192.0, 10.0, 1.15],
    [181.0, 40.0, 0.75],
    [204.0, 26.0, 1.0],
];

/// Everything that does not move, built once for the process. The
/// `Float32Array`s of the original are `f32` here so the port rounds where
/// it does.
struct Land {
    depth: Vec<f32>,
    alt: Vec<f32>,
    sun: Vec<f32>,
    snow: Vec<f32>,
    up: Vec<f32>,
    rim: Vec<u8>,
    tree_top: Vec<f32>,
    src: Vec<f32>,
    fg: Vec<u8>,
    fg_shade: Vec<f32>,
    fg_rim: Vec<f32>,
    mist: Vec<f32>,
    cloud: Vec<f32>,
    hz: Vec<f32>,
    sky_r: Vec<f32>,
    sky_g: Vec<f32>,
    sky_b: Vec<f32>,
    glow_a: Vec<f32>,
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
    let peaks = peaks();
    let light = {
        let v: [f64; 3] = [0.9, 0.3, 0.14];
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / n, v[1] / n, v[2] / n]
    };

    // --- the range, raymarched once ---------------------------------------
    let mut depth = vec![0.0f32; SR * W];
    let mut alt = vec![0.0f32; SR * W];
    let mut sun = vec![0.0f32; SR * W];
    let mut snow = vec![0.0f32; SR * W];
    let mut up = vec![0.0f32; SR * W];
    for r in 0..SR {
        let v = ((HZ - (r as f64 + 0.5)) / 100.0) * K;
        for x in 0..W {
            let u = ((x as f64 + 0.5 - 100.0) / 100.0) * K;
            let z = march(&peaks, u, v, CAM);
            let k = r * W + x;
            if z == 0.0 {
                continue;
            }
            let px = u * z;
            let py = CAM + v * z;
            let e = 0.35;
            let hx = (terrain(&peaks, px + e, z) - terrain(&peaks, px - e, z)) / (2.0 * e);
            let hzs = (terrain(&peaks, px, z + e) - terrain(&peaks, px, z - e)) / (2.0 * e);
            let nl = (hx * hx + 1.0 + hzs * hzs).sqrt();
            let nx = -hx / nl;
            let ny = 1.0 / nl;
            let nz = -hzs / nl;
            let mut lit = (nx * light[0] + ny * light[1] + nz * light[2]).max(0.0);
            if lit > 0.0 {
                // cast shadow: walk toward the sun
                let mut s = 0.8;
                while s < 160.0 {
                    let qx = px + light[0] * s;
                    let qy = py + light[1] * s + 0.15;
                    let qz = z + light[2] * s;
                    if qz < SHORE_Z {
                        break;
                    }
                    if qy < terrain(&peaks, qx, qz) {
                        lit = 0.0;
                        break;
                    }
                    s += 0.6 + s * 0.04;
                }
            }
            depth[k] = z as f32;
            alt[k] = py as f32;
            sun[k] = lit as f32;
            up[k] = ny as f32;
            let grain = fbm(px * 0.4, py * 0.25, 2, 0.0);
            // rock shows through in couloirs running down the fall line
            let gully = ridged(px * 0.22 + z * 0.05, py * 0.045, 2);
            snow[k] = (smooth(0.36, 0.52, ny + 0.3 * (grain - 0.5))
                * smooth(5.0, 11.0, py + 5.0 * grain)
                * (1.0 - 0.75 * smooth(0.62, 0.85, gully))) as f32;
        }
    }

    // sunlit terrain with open sky directly above: the crest line
    let mut rim = vec![0u8; SR * W];
    for k in W..SR * W {
        rim[k] = u8::from(depth[k] != 0.0 && depth[k - W] == 0.0 && sun[k] > 0.0);
    }

    // --- the far shore's treeline -----------------------------------------
    let mut tree_top = vec![0.0f32; W];
    for (x, top) in tree_top.iter_mut().enumerate() {
        *top = (SHORE_F - 0.6 - 1.2 * fbm(x as f64 * 0.06, 2.3, 2, 0.0)) as f32;
    }
    let mut tx = -2.0;
    while tx < W as f64 + 2.0 {
        let tip = SHORE_F - 2.6 - hash(tx * 3.0, 8.0) * 4.0 - 1.5 * smooth(60.0, 0.0, tx);
        let slope = 1.1 + hash(tx * 5.0, 9.0) * 0.5;
        let mut x = (tx - 6.0).floor().max(0.0) as usize;
        while x < W && (x as f64) < tx + 6.0 {
            let v = tip + (x as f64 + 0.5 - tx).abs() * slope;
            tree_top[x] = tree_top[x].min(v as f32);
            x += 1;
        }
        tx += 1.6 + hash(tx * 9.0, 7.0) * 2.2;
    }

    // --- the lake: where each cell's reflected ray lands in the picture above
    let mut src = vec![0.0f32; LR * W];
    for r in SHORE..H {
        let v = ((HZ - (r as f64 + 0.5)) / 100.0) * K;
        for x in 0..W {
            let u = ((x as f64 + 0.5 - 100.0) / 100.0) * K;
            let z = march(&peaks, u, -v, -CAM);
            let vs = -v - if z != 0.0 { (2.0 * CAM) / z } else { 0.0 };
            let mut row = HZ - (vs * 100.0) / K - 0.5;
            // the treeline stands on the shore
            let mirror = 2.0 * SHORE_F - 1.0 - r as f64;
            if mirror >= f64::from(tree_top[x]) {
                row = mirror;
            }
            src[(r - SHORE) * W + x] = row as f32;
        }
    }

    // --- the near pines and the bank they stand on -------------------------
    let mut fg = vec![0u8; N];
    let mut fg_shade = vec![0.0f32; N];
    let mut fg_rim = vec![0.0f32; N];
    for r in 0..H {
        let rf = r as f64;
        for x in 0..W {
            let xf = x as f64;
            let k = r * W + x;
            let y = rf + 0.5;
            let bank_l = 88.0 + 14.0 * smooth(0.0, 52.0, xf) + 2.0 * fbm(xf * 0.2, 1.0, 2, 0.0);
            let bank_r = 90.0
                + 12.0 * smooth(W as f64, W as f64 - 40.0, xf)
                + 2.0 * fbm(xf * 0.2, 4.0, 2, 0.0);
            if y > bank_l || y > bank_r {
                fg[k] = 1;
                fg_shade[k] = (0.15 * hash(xf, rf)) as f32;
            }
            for [px, tip, s] in PINES {
                let d = y - tip;
                if d < 0.0 {
                    continue;
                }
                let tier = 3.4 * s;
                let f = d / tier - (d / tier).floor();
                let hw = (0.4 + d * 0.2)
                    * (0.5 + 0.5 * f)
                    * (1.0 + 0.6 * (hash(rf, px.floor() * 7.0) - 0.5) * smooth(0.0, 8.0, d));
                let dx = xf + 0.5 - px;
                if dx.abs() <= hw {
                    fg[k] = 2;
                    // the outline catches the dawn sky: a warm rim on the side
                    // facing the sun, a dim cool one on the other, the inside
                    // stays black
                    let edge = hw - dx.abs() < 1.0;
                    let sunward = if px < SUN[0] { dx > 0.0 } else { dx < 0.0 };
                    fg_shade[k] = match (edge, sunward) {
                        (true, true) => 1.0,
                        (true, false) => 0.5,
                        (false, _) => (0.3 * hash(xf * 3.0, rf * 5.0)) as f32,
                    };
                    // the rim is light from the sky behind, so it fades out
                    // below the shore
                    fg_rim[k] = match edge {
                        true => (smooth(70.0, 44.0, y) * (0.75 + 0.25 * hash(xf, rf * 7.0))) as f32,
                        false => 0.0,
                    };
                }
            }
        }
    }

    // --- mist, a wrapping sheet that drifts along the valley ---------------
    let mut mist = vec![0.0f32; MW * MR];
    for r in 0..MR {
        for x in 0..MW {
            let xf = x as f64;
            let y = (r + M0) as f64;
            let q = fbm(xf * 0.0125, y * 0.1, 2, MW as f64 * 0.0125);
            mist[r * MW + x] = fbm(xf * 0.025 + q * 1.4, y * 0.22 + q, 4, MW as f64 * 0.025) as f32;
        }
    }

    // --- thin high cloud, streaked and lit from below by the sun -----------
    let mut cloud = vec![0.0f32; CW * CR];
    for r in 0..CR {
        for x in 0..CW {
            let xf = x as f64;
            let y = r as f64 + 0.5;
            let q = fbm(xf * 0.0125, y * 0.12, 2, 8.0);
            let c = fbm(xf * 0.025 + q * 2.0, y * 0.2 + q * 0.8, 4, 16.0);
            cloud[r * CW + x] = (smooth(0.52, 0.7, c - 0.06 * (y - 18.0).abs() / 10.0)
                * smooth(5.0, 13.0, y)
                * smooth(33.0, 24.0, y)) as f32;
        }
    }

    // a faint large-scale unevenness, so the open sky and the deep water are
    // never one flat halftone screen
    let mut hz = vec![0.0f32; N];
    for (k, cell) in hz.iter_mut().enumerate() {
        *cell = fbm((k % W) as f64 * 0.03, (k / W) as f64 * 0.06, 3, 0.0) as f32;
    }

    // the sky's colour at a point, for the sky itself and as haze on the peaks
    let mut sky_r = vec![0.0f32; SR * W];
    let mut sky_g = vec![0.0f32; SR * W];
    let mut sky_b = vec![0.0f32; SR * W];
    let mut glow_a = vec![0.0f32; SR * W];
    for r in 0..SR {
        for x in 0..W {
            let xf = x as f64;
            let y = r as f64 + 0.5;
            let v = clamp(y / 52.0);
            let east = smooth(20.0, 190.0, xf);
            let dx = xf + 0.5 - SUN[0];
            let dy = (y - SUN[1]) * 2.2;
            let ds = (dx * dx + dy * dy).sqrt();
            let low = v.powf(1.9);
            // indigo overhead, a soft unevenness in it, then a pale lilac and
            // rose band behind the range, lighter than the mountains' shadowed
            // flanks
            let veil = (f64::from(hz[r * W + x]) - 0.5) * 0.1 * (1.0 - v);
            sky_r[r * W + x] = (0.03 + veil + low * (0.56 + 0.2 * east)) as f32;
            sky_g[r * W + x] = (0.04 + veil + low * (0.48 + 0.02 * east)) as f32;
            sky_b[r * W + x] = (0.13 + veil * 1.6 + low * (0.62 - 0.12 * east)) as f32;
            // the sun's glow, kept apart so it can breathe
            glow_a[r * W + x] = ((-ds / 6.0).exp() * 0.65
                + (-ds / 15.0).exp() * 0.2
                + (-ds / 50.0).exp() * 0.1) as f32;
        }
    }

    Land {
        depth,
        alt,
        sun,
        snow,
        up,
        rim,
        tree_top,
        src,
        fg,
        fg_shade,
        fg_rim,
        mist,
        cloud,
        hz,
        sky_r,
        sky_g,
        sky_b,
        glow_a,
    }
}

/// The scene at play time `t` (seconds).
pub(crate) fn frame(t: f64) -> ShadedFrame {
    let land = land();
    let f = |v: f32| f64::from(v);

    // rose first light warming toward gold
    let warm = 0.75 - 0.5 * (-t / 60.0).exp();
    // the sunlit line creeps down the slopes
    let line = 6.0 + 4.0 * (-t / 70.0).exp();
    let drift = t * 1.1;
    // the sun's glow breathes
    let pulse = 1.0 + 0.06 * ((t / 8.0) * std::f64::consts::PI * 2.0).sin();
    let mut wisp = vec![0.0f32; W];
    for (x, w) in wisp.iter_mut().enumerate() {
        *w = noise((x as f64 + drift * 0.6) * 0.06, 3.7, 0.0) as f32;
    }

    // the warm light, from rose toward gold
    let lr = 1.0;
    let lg = mix(0.6, 0.8, warm);
    let lb = mix(0.55, 0.4, warm);

    // the picture above the water (`a_*`), and the whole frame (`f_*`)
    let mut a_r = vec![0.0f32; SR * W];
    let mut a_g = vec![0.0f32; SR * W];
    let mut a_b = vec![0.0f32; SR * W];
    let mut f_r = vec![0.0f32; N];
    let mut f_g = vec![0.0f32; N];
    let mut f_b = vec![0.0f32; N];
    let mut floor = vec![0.0f32; N];
    let mut fade = vec![1.0f32; N];

    for r in 0..SR {
        let rf = r as f64;
        let y = rf + 0.5;
        for x in 0..W {
            let xf = x as f64;
            let k = r * W + x;
            let gl = f(land.glow_a[k]) * pulse;
            // gold at the core, rose further out
            let gg = 0.62 + 0.28 * smooth(0.15, 0.7, gl);
            let mut cr = f(land.sky_r[k]) + gl;
            let mut cg = f(land.sky_g[k]) + gl * gg;
            let mut cb = f(land.sky_b[k]) + gl * (gg - 0.22);
            let mut fl = 0.21;
            let z = f(land.depth[k]);
            if z != 0.0 {
                let sn = f(land.snow[k]);
                let altk = f(land.alt[k]);
                let lit = f(land.sun[k]) * smooth(line, line + 7.0, altk);
                let amb = (0.55 + 0.45 * f(land.up[k])) * (0.55 + 0.5 * smooth(4.0, 34.0, altk));
                // snow: deep blue in shadow, rose to gold in the sun, ending
                // sharply
                let sl = smooth(0.08, 0.24, lit);
                // full on faces and summits glow gold, glancing light stays rose
                let gold = clamp(0.6 * smooth(0.25, 0.8, lit) + 0.5 * smooth(14.0, 36.0, altk));
                let br = 0.78 + 0.3 * lit;
                let sr = mix(0.13 * amb, lr * br, sl);
                let sg = mix(0.17 * amb, mix(lg - 0.12, lg + 0.14, gold) * br, sl);
                let sb = mix(0.36 * amb, mix(lb + 0.02, lb + 0.12, gold) * br, sl);
                // rock: slate in shadow, warm umber in the sun
                let rr = mix(0.06, 0.4, sl);
                let rg = mix(0.07, 0.2, sl);
                let rb = mix(0.14, 0.2, sl);
                cr = mix(rr, sr, sn);
                cg = mix(rg, sg, sn);
                cb = mix(rb, sb, sn);
                // forested foothills
                let wood = smooth(9.0, 4.0, altk) * smooth(110.0, 75.0, z);
                cr = mix(cr, 0.07, wood);
                cg = mix(cg, 0.09, wood);
                cb = mix(cb, 0.18, wood);
                // distance hazes toward the sky behind, and haze settles in the
                // far valleys so each ridge stands clear of the one behind it
                let fog = smooth(16.0, 3.0, altk) * smooth(70.0, 150.0, z) * 0.6;
                let haze = fog.max(clamp(1.0 - (-(z - SHORE_Z) / 260.0).exp()) * 0.45);
                cr = mix(cr, f(land.sky_r[k]) + gl, haze);
                cg = mix(cg, f(land.sky_g[k]) + gl * gg, haze);
                cb = mix(cb, f(land.sky_b[k]) + gl * (gg - 0.22), haze);
                // the first light catches the crest itself in a bright line
                if land.rim[k] != 0 && sl > 0.3 {
                    let a = f64::from(land.rim[k]) * sl;
                    cr = mix(cr, 1.0, a);
                    cg = mix(cg, mix(0.89, 0.95, warm), a);
                    cb = mix(cb, mix(0.76, 0.88, warm), a);
                }
                // the shadowed range still carries a dim blue screen; the
                // wooded foothills in front of it drop away to near black
                fl = mix(mix(0.42, 0.3, wood), 0.04, sl);
            } else {
                // a few stars still out in the west
                if y < 34.0 && hash(xf, rf * 3.0 + 11.0) > 0.985 {
                    let tw =
                        0.6 + 0.4 * (t * (1.5 + hash(xf, rf) * 3.0) + hash(rf, xf) * 6.28).sin();
                    let s = tw * smooth(150.0, 40.0, xf) * smooth(34.0, 6.0, y) * 0.75;
                    cr = cr.max(s * 0.9);
                    cg = cg.max(s * 0.92);
                    cb = cb.max(s);
                }
                // high cloud, rose-gold toward the sun and mauve away from it
                if r < CR {
                    let sx = xf + t * 0.8;
                    let ix = sx.floor();
                    let fx = sx - ix;
                    let ix = ix as usize;
                    let c0 = f(land.cloud[r * CW + (ix % CW)]);
                    let c1 = f(land.cloud[r * CW + ((ix + 1) % CW)]);
                    let c = (c0 + (c1 - c0) * fx) * (0.35 + 0.65 * smooth(40.0, 150.0, xf));
                    if c > 0.01 {
                        let dx = xf + 0.5 - SUN[0];
                        let dy = (y - SUN[1]) * 1.6;
                        let g = (-(dx * dx + dy * dy).sqrt() / 55.0).exp();
                        let b = clamp(0.25 + 0.9 * g);
                        let kr = mix(0.32, 1.0, b);
                        let kg = mix(0.24, mix(0.62, 0.74, warm), b);
                        let kb = mix(0.4, 0.5, b);
                        cr = mix(cr, kr, c * 0.75);
                        cg = mix(cg, kg, c * 0.75);
                        cb = mix(cb, kb, c * 0.75);
                    }
                }
                // the sun, just clearing the ridge
                let dx = xf + 0.5 - SUN[0];
                let dy = y - SUN[1];
                let ds = (dx * dx + dy * dy).sqrt();
                if ds < 4.5 {
                    let a = smooth(4.5, 3.3, ds);
                    cr = mix(cr, 1.0, a);
                    cg = mix(cg, 0.96, a);
                    cb = mix(cb, 0.86, a);
                }
            }
            // mist pooled in the valley behind the shore, lit rose toward the sun
            let e = smooth(20.0, 170.0, xf) * (0.6 + 0.4 * warm);
            let near = (-(xf + 0.5 - SUN[0]).abs() / 22.0).exp() * 0.3;
            let mr = mix(0.48, 0.9, e) + near;
            let mg = mix(0.46, 0.7, e) + near * 0.75;
            let mb = mix(0.7, 0.7, e) + near * 0.5;
            if r >= M0 {
                let m = f(land.mist[(r - M0) * MW + ((xf + drift).floor() as usize % MW)]);
                // a ragged top edge: the sheet heaves in long swells and small
                // tufts
                let edge = 5.0 * (m - 0.5) + 4.0 * (f(wisp[x]) - 0.5);
                let band = smooth(M0 as f64 + 14.0, SHORE_F - 3.0, y + edge);
                let a = (0.3 + 0.7 * smooth(0.32, 0.64, m)) * band * 0.6;
                cr = mix(cr, mr, a);
                cg = mix(cg, mg, a);
                cb = mix(cb, mb, a);
                if a > 0.05 {
                    fl = fl.max(0.2);
                }
            }
            if y >= f(land.tree_top[x]) {
                // the far shore's pines, dark against the mist
                let s = 0.4 + 0.6 * hash(xf * 7.0, rf * 3.0);
                cr = 0.04 + 0.03 * s;
                cg = 0.06 + 0.04 * s;
                cb = 0.1 + 0.05 * s;
                fl = 0.0;
                // and low wisps drifting across their feet
                let m = f(land.mist
                    [(r - M0) * MW + ((xf * 0.7 + drift * 1.9 + 211.0).floor() as usize % MW)]);
                let a = smooth(0.45, 0.72, m) * smooth(f(land.tree_top[x]) + 1.0, SHORE_F, y) * 0.6;
                cr = mix(cr, mr, a);
                cg = mix(cg, mg, a);
                cb = mix(cb, mb, a);
            }
            a_r[k] = cr as f32;
            a_g[k] = cg as f32;
            a_b[k] = cb as f32;
            f_r[k] = cr as f32;
            f_g[k] = cg as f32;
            f_b[k] = cb as f32;
            floor[k] = fl as f32;
        }
    }

    // the lake: the picture above, shaken a little by slow ripples
    for r in SHORE..H {
        let y = r as f64 + 0.5;
        let d = (y - SHORE_F) / LR as f64;
        for x in 0..W {
            let xf = x as f64;
            let k = r * W + x;
            let w1 = noise(xf * 0.045 + t * 0.06, y * 0.5 - t * 0.35, 0.0);
            let w2 = noise(xf * 0.12 - t * 0.1, y * 1.1 - t * 0.7, 0.0);
            let sway = (w1 - 0.5) * (0.4 + 1.4 * d) + (w2 - 0.5) * 0.5;
            let sx = js_round(xf + sway).clamp(0.0, W as f64 - 1.0) as usize;
            let sr = js_round(f(land.src[(r - SHORE) * W + x]) + (w2 - 0.5) * 0.6 * d)
                .clamp(0.0, SR as f64 - 1.0) as usize;
            let sk = sr * W + sx;
            let refl = 0.68 - 0.32 * d;
            let w3 = noise(xf * 0.03 + t * 0.04, y * 1.9 - t * 0.45, 0.0);
            // long, faint ripple lines
            let lift = 1.0 + (w3 - 0.5) * (0.4 + 0.5 * d);
            // the water gives back a little less colour than it was sent
            let (ar, ag, ab) = (f(a_r[sk]), f(a_g[sk]), f(a_b[sk]));
            let grey = (ar + ag + ab) / 3.0;
            // slow unevenness in the dark water
            let deep = (f(land.hz[k]) - 0.5) * 0.1 * d;
            let mut cr = 0.02 + deep + mix(grey, ar, 0.75) * refl * lift;
            let mut cg = 0.035 + deep + mix(grey, ag, 0.75) * refl * lift;
            let mut cb = 0.07 + deep * 1.6 + mix(grey, ab, 0.75) * refl * lift;
            // the sun's road
            let road_w = 1.5 + (y - SHORE_F) * 0.45;
            let road = (-((xf + 0.5 - SUN[0]) / road_w).powi(2)).exp();
            let glint = smooth(0.55, 0.85, w2) * road * (0.5 + 0.5 * warm);
            cr += glint;
            cg += glint * 0.8;
            cb += glint * 0.6;
            f_r[k] = cr as f32;
            f_g[k] = cg as f32;
            f_b[k] = cb as f32;
            floor[k] = 0.26;
            fade[k] = smooth(H as f64 + 2.0, H as f64 - 22.0, y) as f32;
            if r == SHORE {
                // a dark seam where the shore meets the water
                f_r[k] = 0.06;
                f_g[k] = 0.12;
                f_b[k] = 0.14;
                floor[k] = 0.0;
            }
        }
    }

    // the near pines and the bank, black against it all, rimmed on the sun side
    for k in 0..N {
        if land.fg[k] == 0 {
            continue;
        }
        let s = f(land.fg_shade[k]);
        f_r[k] = (0.02 + 0.05 * s) as f32;
        f_g[k] = (0.04 + 0.06 * s) as f32;
        f_b[k] = (0.05 + 0.06 * s) as f32;
        floor[k] = 0.0;
        let e = f(land.fg_rim[k]);
        if e > 0.0 && s == 1.0 {
            // warm on the side facing the sun
            f_r[k] = mix(f(f_r[k]), 0.62, e) as f32;
            f_g[k] = mix(f(f_g[k]), 0.4, e) as f32;
            f_b[k] = mix(f(f_b[k]), 0.38, e) as f32;
            floor[k] = (0.12 * e) as f32;
        } else if e > 0.0 && s == 0.5 {
            // cool lilac from the sky on the other
            f_r[k] = mix(f(f_r[k]), 0.2, e) as f32;
            f_g[k] = mix(f(f_g[k]), 0.22, e) as f32;
            f_b[k] = mix(f(f_b[k]), 0.36, e) as f32;
            floor[k] = (0.22 * e) as f32;
        }
        fade[k] = 1.0;
    }

    let mut cells = Vec::with_capacity(N);
    for k in 0..N {
        let rgb = [f(f_r[k]), f(f_g[k]), f(f_b[k])];
        let fl = f(floor[k]);
        let peak = rgb[0].max(rgb[1]).max(rgb[2]).max(1e-4);
        let level = clamp(fl + (1.0 - fl) * peak.powf(1.1)) * f(fade[k]);
        // the largest dot's ink: dim cells keep some of their darkness in the
        // colour too, so the shadows sit back in deep blues rather than as a
        // bright fine screen
        let s = mix(0.5, 1.0, smooth(0.08, 0.5, peak)) / peak;
        cells.push(Shade {
            level,
            rgb,
            ink: [clamp(rgb[0] * s), clamp(rgb[1] * s), clamp(rgb[2] * s)],
        });
    }
    ShadedFrame {
        cols: W,
        rows: H,
        ground: GROUND,
        cells,
    }
}

#[cfg(test)]
const DOTS: [char; 4] = [' ', '·', '•', '●'];
#[cfg(test)]
const COVER: [f64; 4] = [0.0, 0.3, 0.6, 1.0];
#[cfg(test)]
const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

/// One cell of the halftone as the original draws it: its dot and its
/// palette colour, dithered by where the cell sits (`x`, `y`). What the
/// golden frame is compared against.
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
    let s = ((0.3 + 0.7 * want) * mix(0.5, 1.0, smooth(0.08, 0.5, peak))) / peak;
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

#[cfg(test)]
#[path = "alpine_dawn_test.rs"]
mod alpine_dawn_test;
