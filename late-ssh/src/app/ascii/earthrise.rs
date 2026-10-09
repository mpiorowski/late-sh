//! earthrise: the Earth coming up over the lunar horizon. The sun is low on
//! the right, so every crater rim and boulder throws a long black shadow
//! across the grey ground, and the same light makes a gibbous Earth with a
//! clean line between day and night. The Earth turns, its clouds drift, it
//! climbs very slowly, and a few bright stars breathe. Ported from
//! ascii.rest's `earthrise` (MIT, @bas3line).
//!
//! The ground is a heightfield of craters, rendered once column by column
//! from a camera standing on it, with real shadows marched toward the sun;
//! the sky, its stars and the Earth's maps (surface colour, sea, cloud) are
//! built with it, once for the whole process. Each frame only the Earth's
//! disc and the few stars that twinkle are shaded again. ascii.rest then
//! draws every cell as a halftone dot sized by its brightness, ordered-
//! dithered, in the palette colour nearest its hue; [`dot`] is that step,
//! kept for the golden frame, while the renderer draws `Shade::ink`
//! (`ui.rs`).
//!
//! It is a slow piece (`piece::Cadence::Slow`, `piece::EARTH_FRAME_MS` and
//! `piece::EARTH_RATE`), and the screensaver's default: a frame every few
//! seconds turns the globe a visible notch, and the disc is all it moves.

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::piece::{ROUGH_TAU, Shade, ShadedFrame, js_i32, js_round};

pub(crate) const COLS: usize = 200;
pub(crate) const ROWS: usize = 100;
const W: usize = COLS;
const H: usize = ROWS;
const WF: f64 = W as f64;
const N: usize = W * H;
/// Screen row of eye level; the horizon dips below it.
const EYE: f64 = 37.0;
/// Focal length in cells.
const F: f64 = 112.0;
const CAM_H: f64 = 20.0;
/// The moon's radius, in ground units, for the falling horizon.
const RM: f64 = 1500.0;
const ZMAX: f64 = 520.0;
/// The Earth on screen (centre column and row, radius), its lower edge
/// still behind the horizon.
const EC: [f64; 2] = [141.0, 32.0];
const ER: f64 = 22.0;
/// Which face of the globe is turned to us at the start.
const LON0: f64 = 3.5;
/// It climbs `RISE` rows and settles back over `RISE_T` seconds.
const RISE: f64 = 5.0;
const RISE_T: f64 = 150.0;
/// The equirectangular maps' size.
const TW: usize = 192;
const TH: usize = 96;

/// The colour behind the dots.
pub(crate) const GROUND: [u8; 3] = [0x03, 0x04, 0x08];
#[cfg(test)]
pub(crate) const PALETTE: [[u8; 3]; 38] = [
    // greys, black to the sun on the rim
    [0x18, 0x18, 0x1b],
    [0x23, 0x23, 0x26],
    [0x30, 0x30, 0x33],
    [0x41, 0x41, 0x43],
    [0x55, 0x55, 0x56],
    [0x6b, 0x6a, 0x69],
    [0x83, 0x81, 0x7d],
    [0x9c, 0x99, 0x93],
    [0xb6, 0xb2, 0xaa],
    [0xcf, 0xca, 0xc1],
    [0xe6, 0xe1, 0xd8],
    [0xf7, 0xf4, 0xee],
    // the night sky
    [0x0c, 0x11, 0x20],
    [0x13, 0x1a, 0x2e],
    [0x1b, 0x25, 0x40],
    [0x26, 0x2f, 0x4a],
    // starlight
    [0xdf, 0xe9, 0xff],
    // the oceans
    [0x0a, 0x22, 0x59],
    [0x0f, 0x2f, 0x72],
    [0x15, 0x40, 0x8c],
    [0x1d, 0x53, 0xa6],
    [0x2a, 0x69, 0xbf],
    [0x43, 0x86, 0xd3],
    [0x6e, 0xae, 0xea],
    [0xa8, 0xd3, 0xf6],
    // cloud and ice
    [0xe8, 0xf0, 0xfa],
    [0xc2, 0xd0, 0xe3],
    [0x8b, 0x9f, 0xbc],
    // the land
    [0x2f, 0x4a, 0x26],
    [0x3b, 0x5a, 0x2c],
    [0x5b, 0x72, 0x38],
    [0x7a, 0x91, 0x50],
    [0x77, 0x78, 0x3f],
    [0x8f, 0x85, 0x50],
    [0xa8, 0x95, 0x5e],
    [0x6b, 0x56, 0x34],
    [0xb9, 0x77, 0x4a],
    [0x8a, 0x48, 0x38],
];
#[cfg(test)]
const DOTS: [char; 4] = [' ', '·', '•', '●'];
#[cfg(test)]
const COVER: [f64; 4] = [0.0, 0.3, 0.6, 1.0];
#[cfg(test)]
const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
/// The bright stars, which twinkle: [col, row, brightness, period in
/// seconds].
const BRIGHT: [[f64; 4]; 4] = [
    [24.0, 9.0, 1.0, 4.6],
    [67.0, 27.0, 0.85, 3.4],
    [99.0, 12.0, 0.95, 5.8],
    [189.0, 7.0, 0.8, 4.1],
];
/// A few big boulders in the near ground, [x, z, radius].
const ROCKS: [[f64; 3]; 6] = [
    [30.0, 50.0, 2.4],
    [62.0, 63.0, 1.8],
    [12.0, 70.0, 1.3],
    [-4.0, 45.0, 1.2],
    [90.0, 55.0, 1.6],
    [46.0, 90.0, 1.4],
];
/// The storms the clouds wind around: [longitude, latitude, spin].
const STORMS: [[f64; 3]; 5] = [
    [0.9, 0.8, 1.0],
    [3.1, -0.85, -1.0],
    [4.6, 0.62, 1.0],
    [1.9, 0.25, 1.0],
    [5.6, -0.55, -1.0],
];

/// Toward the sun: low, from the right and a little behind us (z is
/// forward).
fn sun() -> [f64; 3] {
    normalized([0.94, 0.14, -0.3])
}

fn normalized(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

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

/// A bowl with a raised rim, `r` in ground units, `q` its distance in
/// radii.
fn bowl(q: f64, r: f64) -> f64 {
    let dish = match q < 1.0 {
        true => -0.36 * (1.0 - q * q),
        false => 0.0,
    };
    r * (dish + 0.14 * (-((q - 1.0) / 0.25).powi(2)).exp())
}

/// Craters scattered one to a cell.
fn craters(x: f64, z: f64, cell: f64, salt: f64, r0: f64, r1: f64, p: f64) -> f64 {
    let (ci, cj) = ((x / cell).floor(), (z / cell).floor());
    let mut h = 0.0;
    for j in [cj - 1.0, cj, cj + 1.0] {
        for i in [ci - 1.0, ci, ci + 1.0] {
            if hash(i + salt, j - salt) > p {
                continue;
            }
            let cx = (i + hash(i, j + salt * 3.0)) * cell;
            let cz = (j + hash(i + salt * 5.0, j)) * cell;
            let e = hash(j + salt, i - salt * 7.0);
            let r = cell * (r0 + (r1 - r0) * e * e);
            let (dx, dz) = (x - cx, z - cz);
            let d2 = dx * dx + dz * dz;
            if d2 > 4.0 * r * r {
                continue;
            }
            h += bowl(d2.sqrt() / r, r) * 0.95;
        }
    }
    h
}

fn height(x: f64, z: f64) -> f64 {
    let mut h = 6.0 * fbm(x * 0.006 + 50.0, z * 0.006 + 50.0, 3, 0.0)
        + 0.6 * fbm(x * 0.04, z * 0.04, 2, 0.0);
    // old, worn highlands at the edge of sight, rising to the left, and a
    // lower ridge in front of them
    if z > 150.0 {
        let lift = smooth(150.0, 300.0, z);
        // rounded massifs: folded noise, worn smooth
        let m = 1.0 - (2.0 * fbm(x * 0.006 + 7.0, z * 0.006, 4, 0.0) - 1.0).abs();
        h += lift * (28.0 * (-((x + 260.0) / 230.0).powi(2)).exp() + 45.0 * (m * m - 0.35));
        let ridge = (-((z - 205.0) / 20.0).powi(2)).exp() * (-((x + 140.0) / 130.0).powi(2)).exp();
        h += ridge * 22.0 * (0.35 + fbm(x * 0.018 + 3.0, z * 0.01, 3, 0.0));
    }
    // big basins only out in the middle distance, so we do not stand in one
    if z > 90.0 {
        h += craters(x, z, 110.0, 11.0, 0.14, 0.34, 0.55) * smooth(90.0, 150.0, z);
    }
    if z < 300.0 {
        h += craters(x, z, 34.0, 23.0, 0.12, 0.36, 0.85);
    }
    if z < 170.0 {
        h += craters(x, z, 10.0, 37.0, 0.12, 0.34, 0.85) * smooth(170.0, 110.0, z);
    }
    // one big crater in the near ground, off to the left
    {
        let (dx, dz) = (x + 24.0, z - 58.0);
        let q = (dx * dx + dz * dz).sqrt() / 16.0;
        if q < 2.0 {
            h += bowl(q, 16.0);
        }
    }
    // boulders strewn close by, and a few big ones
    if z < 90.0 {
        let c = 5.0;
        let (ci, cj) = ((x / c).floor(), (z / c).floor());
        if hash(ci + 91.0, cj) < 0.14 {
            let bx = (ci + 0.2 + 0.6 * hash(ci, cj + 92.0)) * c;
            let bz = (cj + 0.2 + 0.6 * hash(ci + 93.0, cj)) * c;
            let br = 0.3 + 0.5 * hash(ci + 94.0, cj + 95.0).powi(2);
            let d2 = ((x - bx).powi(2) + (z - bz).powi(2)) / (br * br);
            if d2 < 4.0 {
                h += br * 0.9 * (-d2 * 1.4).exp();
            }
        }
        for [bx, bz, br] in ROCKS {
            let d2 = ((x - bx).powi(2) + (z - bz).powi(2)) / (br * br);
            // a squat, lumpy dome
            if d2 < 1.0 {
                h += br * (0.85 + 0.3 * noise(x * 1.3, z * 1.3, 0.0)) * (1.0 - d2).sqrt();
            }
        }
    }
    h
}

/// One of the bright stars' cells: the cell, its place, its brightness and
/// period, its phase, and whether it is the star itself or a ray of the
/// faint cross it carries.
struct Twinkle {
    k: usize,
    s: f64,
    p: f64,
    ph: f64,
    core: bool,
}

/// Everything that does not move: the ground and the sky shaded once, the
/// Earth's maps, the sky cells the Earth and its air can reach, and the
/// stars that twinkle. The originals are `Float32Array`s, so these hold
/// `f32` and the frame reads them back as `f64`.
struct Land {
    /// The static light of every cell: the ground and the sky, everything
    /// but the Earth's disc.
    sr: Vec<f32>,
    sg: Vec<f32>,
    sb: Vec<f32>,
    /// How bright an ink a cell may draw in: grey stays grey, dim ground
    /// draws in darker ink.
    scap: Vec<f32>,
    /// The sky cells the Earth and its air can reach, as it rises and
    /// settles.
    reach: Vec<usize>,
    twinkle: Vec<Twinkle>,
    /// Equirectangular maps, wrapping in longitude: surface colour, how
    /// much of a cell is open sea, and cloud cover.
    tr: Vec<f32>,
    tg: Vec<f32>,
    tb: Vec<f32>,
    sea: Vec<f32>,
    cloud: Vec<f32>,
    /// The sun in screen space (z toward us), and the half vector between
    /// it and the eye, for the glint on the sea.
    light: [f64; 3],
    half: [f64; 3],
}

fn land() -> &'static Land {
    static LAND: OnceLock<Land> = OnceLock::new();
    LAND.get_or_init(build_land)
}

/// Build the land ahead of the first frame (`piece::warm`).
pub(crate) fn warm() {
    land();
}

fn build_land() -> Land {
    let sun = sun();

    // --- the ground: march each column from near to far -------------------
    let mut ground = vec![false; N];
    let mut top = vec![H as i64; W];
    let mut gx = vec![0f32; N];
    let mut gz = vec![0f32; N];
    let mut gy = vec![0f32; N];
    let cam_y = height(0.0, 7.0) + CAM_H;
    for (c, top_c) in top.iter_mut().enumerate() {
        let dir = (c as f64 + 0.5 - WF / 2.0) / F;
        let mut top_r = H as i64;
        let mut z = 26.0;
        let (mut prev_y, mut prev_z, mut prev_f) = (0.0, 0.0, 1e9);
        while z < ZMAX && top_r > 0 {
            let x = dir * z;
            let hy = height(x, z);
            let y = hy - (z * z) / (2.0 * RM);
            let yf = EYE - (F * (y - cam_y)) / z;
            let r0 = ((yf - 0.5).ceil() as i64).max(0);
            for r in r0..top_r {
                // place the cell between this sample and the last, by where
                // its row falls
                let a = match prev_f > yf + 1e-6 {
                    true => clamp((prev_f - (r as f64 + 0.5)) / (prev_f - yf)),
                    false => 1.0,
                };
                let k = r as usize * W + c;
                ground[k] = true;
                gz[k] = match prev_z != 0.0 {
                    true => mix(prev_z, z, a),
                    false => z,
                } as f32;
                gx[k] = (dir * f64::from(gz[k])) as f32;
                gy[k] = match prev_z != 0.0 {
                    true => mix(prev_y, hy, a),
                    false => hy,
                } as f32;
            }
            if r0 < top_r {
                top_r = r0;
            }
            prev_f = yf;
            prev_y = hy;
            prev_z = z;
            z += 0.03 + z * 0.012;
        }
        *top_c = top_r;
    }

    // Static light: the ground and the sky, everything but the Earth's disc.
    let mut sr = vec![0f32; N];
    let mut sg = vec![0f32; N];
    let mut sb = vec![0f32; N];
    let mut scap = vec![1f32; N];
    for r in 0..H {
        for (x, &top_x) in top.iter().enumerate() {
            let k = r * W + x;
            if !ground[k] {
                continue;
            }
            let (px, pz, py) = (f64::from(gx[k]), f64::from(gz[k]), f64::from(gy[k]));
            let e = 0.1 + pz * 0.004;
            let hx = (height(px + e, pz) - height(px - e, pz)) / (2.0 * e);
            let hz = (height(px, pz + e) - height(px, pz - e)) / (2.0 * e);
            let nl = (hx * hx + 1.0 + hz * hz).sqrt();
            let lam = (-hx * sun[0] + sun[1] - hz * sun[2]) / nl;
            // toward the eye, for the moon's own way of reflecting
            // (Lommel-Seeliger)
            let (vx, vy, vz) = (-px, cam_y - py, -pz);
            let vl = (vx * vx + vy * vy + vz * vz).sqrt();
            let mu = ((-hx * vx + vy - hz * vz) / (nl * vl)).max(0.02);
            let mut lit = 0.0;
            if lam > 0.0 {
                // march toward the sun; a soft edge for the sun's own width
                lit = 1.0;
                let mut s = 0.1 + pz * 0.003;
                while s < 120.0 {
                    let (qx, qz, qy) = (px + sun[0] * s, pz + sun[2] * s, py + sun[1] * s);
                    let d = qy - height(qx, qz);
                    if d < 0.0 {
                        lit = 0.0;
                        break;
                    }
                    lit = f64::min(lit, (d * 30.0) / s);
                    s += 0.05 + s * 0.2;
                }
                lit = smooth(0.0, 1.0, lit);
            }
            // regolith: patchy, the maria darker
            let albedo = 0.5
                + 0.9 * fbm(px * 0.025 + 3.0, pz * 0.025, 3, 0.0)
                + 0.3 * (fbm(px * 0.35, pz * 0.35, 2, 0.0) - 0.5)
                - 0.22 * smooth(0.46, 0.64, fbm(px * 0.0035, pz * 0.0035 + 20.0, 3, 0.0));
            let ls = match lam > 0.0 {
                true => ((0.2 * lam) / (lam + mu) + 2.6 * lam) * lit,
                false => 0.0,
            };
            // the near corners fall off a little, to frame the view
            let vig = 1.0
                - 0.3
                    * smooth(84.0, 102.0, r as f64)
                    * smooth(30.0, 100.0, (x as f64 - 100.0).abs());
            let mut b = (1.0 - (-ls * 1.5).exp()) * albedo * vig;
            // the far crest catches the sun along its whole length
            let crest = r as i64 - top_x;
            if crest < 2 && lit > 0.2 {
                let catch = match crest {
                    0 => 0.85,
                    _ => 0.55,
                };
                b = b.max(catch * albedo);
            }
            // shadow is black, but for a breath of earthshine on what faces us
            let fill = match lit > 0.02 || b > 0.03 {
                true => 0.008 + 0.007 * clamp(-hz / nl + 0.5),
                false => 0.0,
            };
            sr[k] = (b + fill * 0.75) as f32;
            sg[k] = (b * 0.95 + fill * 0.85) as f32;
            sb[k] = (b * 0.86 + fill * 1.2) as f32;
            scap[k] = (0.42 + 0.62 * b) as f32; // grey stays grey: dim ground draws in darker ink
        }
    }

    // the sky: a soft band of the galaxy, and stars that hold still
    let near = |x: usize, r: usize| {
        let (dx, dy) = (
            x as f64 + 0.5 - EC[0],
            r as f64 + 0.5 - (EC[1] - RISE / 2.0),
        );
        (dx * dx + dy * dy).sqrt() < 2.0 * ER
    };
    for r in 0..H {
        for x in 0..W {
            let k = r * W + x;
            if ground[k] {
                continue;
            }
            let (xf, rf) = (x as f64, r as f64);
            // a black sky. Faint stars, thicker along a diagonal where the
            // galaxy runs, none round the Earth
            let band_d = (rf - (4.0 + xf * 0.32)) / 1.05;
            let band = (-(band_d / 10.0).powi(2)).exp()
                * smooth(0.35, 0.65, fbm(xf * 0.05, rf * 0.08, 3, 0.0))
                * smooth(120.0, 70.0, xf);
            let (mut cr, mut cg, mut cb) = (0.0, 0.0, 0.0);
            let hs = hash(xf * 3.0 + 1.0, rf * 7.0 + 2.0);
            if hs > 0.994 - 0.05 * band && !near(x, r) {
                let m = hash(xf + 17.0, rf + 29.0).powf(3.0);
                let s = 0.2 + 0.5 * m;
                let tint = hash(xf + 5.0, rf + 77.0);
                let [tr, tg, tb] = match tint {
                    t if t < 0.3 => [0.84, 0.9, 1.0],
                    t if t > 0.88 => [1.0, 0.93, 0.84],
                    _ => [0.96, 0.96, 0.98],
                };
                cr = s * tr;
                cg = s * tg;
                cb = s * tb;
            }
            sr[k] = cr as f32;
            sg[k] = cg as f32;
            sb[k] = cb as f32;
            scap[k] = 1.0;
        }
    }

    // --- the Earth ---------------------------------------------------------
    // Equirectangular maps, wrapping in longitude: surface colour and cloud.
    let mut tr = vec![0f32; TW * TH];
    let mut tg = vec![0f32; TW * TH];
    let mut tb = vec![0f32; TW * TH];
    let mut sea = vec![0f32; TW * TH];
    let mut cloud = vec![0f32; TW * TH];
    let mut elev = vec![0f32; TW * TH];
    let (twf, thf) = (TW as f64, TH as f64);
    for j in 0..TH {
        let v = (j as f64 + 0.5) / thf;
        let lat = (0.5 - v) * PI;
        let al = lat.abs();
        for i in 0..TW {
            let u = i as f64 / twf;
            let lon = u * PI * 2.0;
            let t = j * TW + i;
            let wx = fbm(u * 6.0, v * 3.0 + 9.0, 3, 6.0);
            elev[t] =
                (fbm(u * 8.0 + 1.6 * wx, v * 4.0, 5, 8.0) - 0.04 * smooth(1.2, 1.5, al)) as f32;
            // clouds: warped noise, wound into spirals around a few storms
            let (mut cx, mut cy) = (u * 14.0, v * 7.0);
            for [slon, slat, spin] in STORMS {
                let mut dl = lon - slon;
                dl -= js_round(dl / (PI * 2.0)) * PI * 2.0;
                let (lx, ly) = (dl * lat.cos(), lat - slat);
                let dd = (lx * lx + ly * ly).sqrt();
                let a = spin * 5.0 * (-dd / 0.2).exp();
                if a * spin > 0.02 {
                    let (ca, sa) = (a.cos(), a.sin());
                    cx += ((lx * ca - ly * sa - lx) / (PI * 2.0)) * 14.0;
                    cy -= ((lx * sa + ly * ca - ly) / PI) * 7.0;
                }
            }
            // streaked along the winds, east to west
            let q = fbm(cx * 0.5 + 3.0, cy * 1.2, 3, 7.0);
            let n0 = fbm(cx + 1.6 * q, cy * 1.3 + 0.5 * q, 5, 14.0);
            // folded into filaments, the way weather fronts string out
            let n = 0.4 * n0
                + 0.6
                    * (1.0 - (2.0 * fbm(cx * 1.5 + 2.2 * q, cy * 1.6 + 9.0, 4, 21.0) - 1.0).abs());
            // cloudy at the equator and in the storm belts, clearer in the
            // subtropics
            let belt = 0.05 * (-(lat / 0.12).powi(2)).exp()
                - 0.07 * (-((al - 0.42) / 0.16).powi(2)).exp()
                + 0.05 * (-((al - 0.95) / 0.25).powi(2)).exp();
            cloud[t] = (n + belt) as f32;
        }
    }
    // thresholds by share of the globe: about three tenths land, a third
    // cloud
    let quantile = |a: &[f32], p: f64| -> f64 {
        let mut s = a.to_vec();
        s.sort_by(f32::total_cmp);
        f64::from(s[(p * (s.len() - 1) as f64).floor() as usize])
    };
    let shore = quantile(&elev, 0.7);
    let c0 = quantile(&cloud, 0.6);
    let c1 = quantile(&cloud, 0.86);
    for j in 0..TH {
        let v = (j as f64 + 0.5) / thf;
        let lat = (0.5 - v) * PI;
        let al = lat.abs();
        for i in 0..TW {
            let u = i as f64 / twf;
            let t = j * TW + i;
            let e = f64::from(elev[t]);
            let land = smooth(shore - 0.004, shore + 0.006, e);
            let ice = smooth(1.22, 1.32, al + 0.12 * fbm(u * 12.0, v * 6.0, 2, 12.0));
            // desert in the subtropics and on high ground, forest and scrub
            // elsewhere, broken up finely so a continent is never one flat
            // tone
            let grain = fbm(u * 36.0 + 2.0, v * 18.0, 3, 36.0) - 0.5;
            let arid = clamp(
                (-((al - 0.4) / 0.22).powi(2)).exp()
                    * (0.1 + 1.2 * fbm(u * 10.0 + 4.0, v * 5.0, 3, 10.0))
                    + (e - shore - 0.04) * 4.0
                    + 0.8 * grain,
            );
            let relief = 1.0 + 1.2 * grain - 2.5 * (e - shore - 0.08).max(0.0);
            let mut r = mix(0.13, 0.4, arid) * relief;
            let mut g = mix(0.25, 0.34, arid) * relief;
            let mut b = mix(0.08, 0.19, arid) * relief;
            // ocean, lighter over the shelves near the coasts
            let shelf = smooth(shore - 0.06, shore, e);
            let (or, og, ob) = (
                mix(0.025, 0.07, shelf),
                mix(0.11, 0.3, shelf),
                mix(0.38, 0.62, shelf),
            );
            r = mix(or, r, land);
            g = mix(og, g, land);
            b = mix(ob, b, land);
            r = mix(r, 0.92, ice);
            g = mix(g, 0.95, ice);
            b = mix(b, 0.99, ice);
            tr[t] = r as f32;
            tg[t] = g as f32;
            tb[t] = b as f32;
            sea[t] = ((1.0 - land) * (1.0 - ice)) as f32;
            cloud[t] = smooth(c0, c1, f64::from(cloud[t])) as f32;
        }
    }

    // the sky cells the Earth and its air can reach, as it rises and
    // settles
    let mut reach = Vec::new();
    let r_lo = ((EC[1] - RISE - ER - 13.0) as i64).max(0) as usize;
    let r_hi = ((EC[1] + ER + 2.0) as usize).min(H - 1);
    let x_lo = (EC[0] - ER - 13.0) as usize;
    let x_hi = (EC[0] + ER + 13.0) as usize;
    for r in r_lo..=r_hi {
        for x in x_lo..=x_hi {
            if !ground[r * W + x] {
                reach.push(r * W + x);
            }
        }
    }
    // the bright stars and the faint cross each one carries
    let mut twinkle = Vec::new();
    for [x, r, s, p] in BRIGHT {
        for [ox, oy, w] in [
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.2],
            [1.0, 0.0, 0.2],
            [0.0, -1.0, 0.2],
            [0.0, 1.0, 0.2],
        ] {
            let k = (r + oy) as usize * W + (x + ox) as usize;
            if !ground[k] {
                twinkle.push(Twinkle {
                    k,
                    s: s * w,
                    p,
                    ph: hash(x, r) * ROUGH_TAU,
                    core: w == 1.0,
                });
            }
        }
    }

    // into screen space, z toward us
    let light = [sun[0], sun[1], -sun[2]];
    let half = normalized([light[0], light[1], light[2] + 1.0]);
    Land {
        sr,
        sg,
        sb,
        scap,
        reach,
        twinkle,
        tr,
        tg,
        tb,
        sea,
        cloud,
        light,
        half,
    }
}

/// A map read at a longitude and a map row, bilinear, wrapping in
/// longitude.
fn sample(a: &[f32], lon: f64, vy: f64) -> f64 {
    let fx = (((lon / (PI * 2.0)) % 1.0) + 1.0) % 1.0 * TW as f64;
    let x0 = fx.floor();
    let ax = fx - x0;
    let x0 = x0 as usize;
    let x1 = (x0 + 1) % TW;
    let y0 = (vy.floor().max(0.0) as usize).min(TH - 2);
    let ay = clamp(vy - y0 as f64);
    let at = |x: usize, y: usize| f64::from(a[y * TW + x]);
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y0 + 1), at(x1, y0 + 1));
    a + (b - a) * ax + (c - a) * ay + (a - b - c + d) * ax * ay
}

/// One cell as the original lights it, before its halftone: the colour,
/// the dot floor (the day side of the Earth never falls below a round
/// dot), and the ink cap (grey ground draws in darker ink).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lit {
    pub rgb: [f64; 3],
    pub floor: f64,
    pub cap: f64,
}

/// The frame at `t` seconds of play.
pub(crate) fn frame(t: f64) -> ShadedFrame {
    let cells = light(t)
        .into_iter()
        .map(|lit| {
            let [cr, cg, cb] = lit.rgb;
            let peak = cr.max(cg).max(cb).max(1e-4);
            let level = clamp(lit.floor + (1.0 - lit.floor) * peak.powf(0.85) * 0.95);
            // the largest dot's ink: the colour at full brightness, held
            // under the cell's cap
            let s = lit.cap.min(1.0) / peak;
            Shade {
                level,
                rgb: lit.rgb,
                ink: [clamp(cr * s), clamp(cg * s), clamp(cb * s)],
            }
        })
        .collect();
    ShadedFrame {
        cols: W,
        rows: H,
        ground: GROUND,
        cells,
    }
}

/// Every cell as the original lights it at `t`: the static light, with the
/// Earth's disc and the twinkling stars shaded over it.
pub(crate) fn light(t: f64) -> Vec<Lit> {
    let land = land();
    let l = land.light;
    let hv = land.half;
    let tilt: f64 = 0.4;
    let nod: f64 = 0.22;
    let (ct, st, cn, sn) = (tilt.cos(), tilt.sin(), nod.cos(), nod.sin());
    let thf = TH as f64;

    let mut cells: Vec<Lit> = (0..N)
        .map(|k| Lit {
            rgb: [
                f64::from(land.sr[k]),
                f64::from(land.sg[k]),
                f64::from(land.sb[k]),
            ],
            floor: 0.0,
            cap: f64::from(land.scap[k]),
        })
        .collect();

    let spin = t * 0.045;
    let drift = t * 0.012; // clouds run a little ahead of the ground
    let ey = EC[1] - RISE * (0.5 - 0.5 * ((t / RISE_T) * PI * 2.0).cos());
    for &k in &land.reach {
        let (x, r) = (k % W, k / W);
        let Lit {
            rgb: [mut cr, mut cg, mut cb],
            mut cap,
            ..
        } = cells[k];
        let mut floor = 0.0;
        let (dx, dy) = (x as f64 + 0.5 - EC[0], r as f64 + 0.5 - ey);
        let d = (dx * dx + dy * dy).sqrt();
        // the Earth's air, a thin blue rim on its sunlit side
        if (ER - 1.0..ER + 12.0).contains(&d) {
            let side = smooth(-0.3, 0.75, (dx * l[0] - dy * l[1]) / d);
            let mut g = (-(d - ER).max(0.0) / 1.2).exp() * 0.55 * side;
            if g < 0.05 {
                g = 0.0; // no stray haze dots out in space
            }
            cr += 0.25 * g;
            cg += 0.52 * g;
            cb += 1.0 * g;
            if g > 0.0 {
                cap = 1.0;
            }
        }
        if d < ER {
            let (nx, ny) = (dx / ER, -dy / ER);
            let q2 = nx * nx + ny * ny;
            let nz = (1.0 - q2).sqrt();
            // into the globe's own frame: tip the pole toward us, then lean it
            let ax = nx * ct + ny * st;
            let ay0 = -nx * st + ny * ct;
            let ay = ay0 * cn - nz * sn;
            let az = ay0 * sn + nz * cn;
            let lat = ay.clamp(-1.0, 1.0).asin();
            let lon = ax.atan2(az) + LON0 + spin;
            let vy = (0.5 - lat / PI) * thf - 0.5;
            let ndl = nx * l[0] + ny * l[1] + nz * l[2];
            // full sun at the right limb, dimming toward the terminator, so
            // the disc reads as a ball
            let day = smooth(-0.005, 0.06, ndl) * (0.45 + 0.8 * clamp(ndl).sqrt());
            let dusk = (-((ndl - 0.02) / 0.035).powi(2)).exp();
            let cl = sample(&land.cloud, lon + drift, vy);
            // the cloud's own shadow, offset away from the sun
            let sh = sample(&land.cloud, lon + drift - 0.03, vy + 0.4);
            let sw = sample(&land.sea, lon, vy);
            let mut er = sample(&land.tr, lon, vy);
            let mut eg = sample(&land.tg, lon, vy);
            let mut eb = sample(&land.tb, lon, vy);
            let shade = 1.0 - 0.45 * sh * (1.0 - cl);
            er *= shade;
            eg *= shade;
            eb *= shade;
            er = mix(er, 0.95, cl);
            eg = mix(eg, 0.97, cl);
            eb = mix(eb, 1.0, cl);
            er *= day * (1.0 + 0.06 * dusk);
            eg *= day * (1.0 - 0.03 * dusk);
            eb *= day * (1.0 - 0.1 * dusk);
            let glint =
                (nx * hv[0] + ny * hv[1] + nz * hv[2]).max(0.0).powf(70.0) * 0.8 * sw * (1.0 - cl);
            let rim = (1.0 - nz).powf(2.2) * smooth(0.0, 0.3, ndl);
            er += glint * 0.95 + rim * 0.2;
            eg += glint * 0.92 + rim * 0.42;
            eb += glint * 0.85 + rim * 0.85;
            // a crisp edge where the disc meets space; the night side is a
            // void
            let edge = smooth(1.0, 0.95, q2.sqrt());
            cr = mix(cr, er, edge);
            cg = mix(cg, eg, edge);
            cb = mix(cb, eb, edge);
            // land keeps its earth tones instead of washing out to cream
            cap = mix(
                1.0,
                0.66,
                (1.0 - sw) * (1.0 - cl) * smooth(0.95, 0.85, ay.abs()),
            );
            // the day side is the brightest thing in the sky: full, round dots
            floor = 0.15 * day.min(1.0) * edge;
        }
        cells[k] = Lit {
            rgb: [cr, cg, cb],
            floor,
            cap,
        };
    }
    for star in &land.twinkle {
        let k = star.k;
        let v = star.s * (0.86 + 0.14 * ((t / star.p) * PI * 2.0 + star.ph).sin());
        cells[k] = match star.core {
            true => Lit {
                rgb: [v * 0.97, v * 0.98, v],
                floor: 0.0,
                cap: 1.0,
            },
            false => Lit {
                rgb: [
                    f64::from(land.sr[k]).max(v * 0.95),
                    f64::from(land.sg[k]).max(v),
                    f64::from(land.sb[k]).max(v * 1.15),
                ],
                floor: 0.0,
                cap: 0.6,
            },
        };
    }
    cells
}

/// The original's halftone of one lit cell at `x`, `y`: its glyph and
/// palette index. Dot size from brightness, colour from hue, with the
/// colour making up what the dot size could not, held under the cap.
#[cfg(test)]
pub(crate) fn dot(lit: Lit, x: usize, y: usize) -> (char, u8) {
    let bayer = f64::from(BAYER[(y & 3) * 4 + (x & 3)]) / 16.0 - 0.47;
    let [cr, cg, cb] = lit.rgb;
    let peak = cr.max(cg).max(cb).max(1e-4);
    let level = clamp(lit.floor + (1.0 - lit.floor) * peak.powf(0.85) * 0.95);
    let step = js_round(level * 3.0 + bayer).clamp(0.0, 3.0) as usize;
    let want = match step {
        0 => 0.0,
        _ => ((level + 0.06) / COVER[step]).min(1.0),
    };
    let s = lit.cap.min(0.3 + 0.7 * want) / peak;
    (
        DOTS[step],
        nearest(clamp(cr * s), clamp(cg * s), clamp(cb * s)),
    )
}

/// The palette colour nearest an rgb, weighted toward green as the eye is.
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
#[path = "earthrise_test.rs"]
mod earthrise_test;
