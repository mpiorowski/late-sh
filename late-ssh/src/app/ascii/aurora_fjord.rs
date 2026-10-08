//! aurora fjord: curtains of aurora ripple over a fjord between snowy
//! mountains. The still water holds a broken shimmer of them, and a red
//! cabin on the far shore keeps its lamps lit. Ported from ascii.rest's
//! `aurora-fjord` (MIT, @bas3line).
//!
//! Shaded in colour per cell on a square grid. ascii.rest then draws it as a
//! halftone, dot size for brightness, ordered-dithered, in the palette colour
//! nearest its hue; [`dot`] is that step, which the renderer runs at the
//! terminal's own resolution (`ui.rs`). The land is built once for the whole
//! process; each frame shades the sky, then mirrors it into the water.

use std::f64::consts::PI;
use std::sync::OnceLock;

use super::piece::{Shade, ShadedFrame, js_i32, js_round};

pub(crate) const COLS: usize = 200;
pub(crate) const ROWS: usize = 100;
const W: usize = COLS;
const H: usize = ROWS;
const WF: f64 = W as f64;
const HF: f64 = H as f64;
/// The waterline.
const WL: usize = 62;
const WLF: f64 = WL as f64;

/// The colour behind the dots.
pub(crate) const GROUND: [u8; 3] = [0x05, 0x08, 0x0f];
pub(crate) const PALETTE: [[u8; 3]; 43] = [
    [0x0b, 0x13, 0x22],
    [0x10, 0x1b, 0x30],
    [0x16, 0x24, 0x3f],
    [0x1e, 0x30, 0x50],
    [0x2a, 0x3f, 0x63],
    [0x0e, 0x3a, 0x33],
    [0x11, 0x57, 0x3f],
    [0x16, 0x7a, 0x4c],
    [0x22, 0xa0, 0x5c],
    [0x3c, 0xcb, 0x73],
    [0x7c, 0xf0, 0xa0],
    [0xc8, 0xff, 0xdc],
    [0x0f, 0x5f, 0x5c],
    [0x16, 0x87, 0x7f],
    [0x2c, 0xb5, 0xa6],
    [0x7f, 0xe6, 0xd6],
    [0x2a, 0x1f, 0x52],
    [0x43, 0x2b, 0x78],
    [0x6a, 0x3c, 0x9f],
    [0x95, 0x58, 0xc6],
    [0xc4, 0x8a, 0xe4],
    [0x36, 0x3a, 0x72],
    [0x4f, 0x55, 0x96],
    [0x1b, 0x4f, 0x63],
    [0x1a, 0x22, 0x36],
    [0x28, 0x33, 0x50],
    [0x3b, 0x4a, 0x6e],
    [0x56, 0x6a, 0x92],
    [0x7d, 0x91, 0xb8],
    [0xa9, 0xba, 0xd9],
    [0xd6, 0xe0, 0xf2],
    [0x9f, 0xc9, 0xcf],
    [0x0f, 0x1a, 0x1c],
    [0x17, 0x31, 0x28],
    [0x24, 0x49, 0x3a],
    [0x4a, 0x15, 0x17],
    [0x7c, 0x24, 0x20],
    [0xb2, 0x3a, 0x2a],
    [0xd6, 0x5a, 0xa8],
    [0xff, 0xd2, 0x7c],
    [0xff, 0xb0, 0x4a],
    [0xff, 0xf2, 0xc4],
    [0xee, 0xf3, 0xff],
];

#[cfg(test)]
const DOTS: [char; 4] = [' ', '·', '•', '●'];
const COVER: [f64; 4] = [0.0, 0.3, 0.6, 1.0];
const BAYER: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mat {
    Air,
    Near,
    Far,
    Shore,
    Wall,
    Roof,
    Pane,
    Tree,
    Door,
}

/// The cabin's walls, x from and to.
const CAB: [usize; 2] = [142, 161];
const PANES: [[usize; 2]; 2] = [[145, 148], [156, 159]];
const DOOR_X: [usize; 2] = [150, 152];
/// Pane centres and their strength.
const LAMPS: [[f64; 2]; 2] = [[147.0, 1.0], [158.0, 0.8]];

fn hash(x: f64, y: f64) -> f64 {
    let h = (js_i32(x) as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((js_i32(y) as u32).wrapping_mul(668_265_263));
    let h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    f64::from(h ^ (h >> 16)) / 4_294_967_296.0
}

fn noise(x: f64, y: f64) -> f64 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let a = hash(xi, yi);
    let b = hash(xi + 1.0, yi);
    let c = hash(xi, yi + 1.0);
    let d = hash(xi + 1.0, yi + 1.0);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn fbm(x: f64, y: f64, octaves: u32) -> f64 {
    let (mut s, mut n, mut amp, mut f) = (0.0, 0.0, 0.5, 1.0);
    for _ in 0..octaves {
        s += amp * noise(x * f, y * f);
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

// Ranges as tent peaks [x, height, slope], roughened.
const LEFT: [[f64; 3]; 4] = [
    [25.0, 38.0, 1.3],
    [6.0, 29.0, 0.9],
    [50.0, 25.0, 1.0],
    [72.0, 13.0, 0.6],
];
const RIGHT: [[f64; 3]; 4] = [
    [172.0, 30.0, 1.15],
    [194.0, 24.0, 0.85],
    [151.0, 16.0, 1.0],
    [212.0, 22.0, 0.6],
];
const DISTANT: [[f64; 3]; 4] = [
    [100.0, 10.0, 0.5],
    [119.0, 12.0, 0.55],
    [86.0, 7.0, 0.45],
    [134.0, 8.0, 0.5],
];

/// A range's height at `x` and the peak that sets it.
fn range(x: f64, peaks: &[[f64; 3]], seed: f64, rough: f64) -> (f64, f64) {
    let (mut m, mut px) = (-99.0, 0.0);
    for &[cx, h, s] in peaks {
        let v = h - (x - cx).abs() * s;
        if v > m {
            m = v;
            px = cx;
        }
    }
    let j =
        rough * (fbm(x * 0.09, seed, 3) - 0.5) + rough * 0.45 * (noise(x * 0.45, seed + 5.0) - 0.5);
    (m + j * (m.max(0.0) / 6.0).min(1.0), px)
}

fn shore_top(x: f64) -> f64 {
    WLF - 2.2 * smooth(134.0, 140.0, x) * smooth(178.0, 168.0, x) - 0.6 * noise(x * 0.3, 2.0)
}

const CAB_BASE: f64 = WLF - 2.2;
const EAVE: f64 = CAB_BASE - 7.0;

/// Everything that does not move: the land's material, base colour, how
/// much aurora light it picks up, the lamplight on it, the sky's haze and
/// its stars.
struct Land {
    mat: Vec<Mat>,
    sr: Vec<f32>,
    sg: Vec<f32>,
    sb: Vec<f32>,
    /// How much aurora light a cell picks up.
    rec: Vec<f32>,
    /// Aurora light caught on ridgelines and treetops.
    rim: Vec<f32>,
    lamp_land: Vec<f32>,
    haze: Vec<f32>,
    star: Vec<f32>,
    /// Rows of the water that break the reflection into strips.
    gap: Vec<bool>,
}

#[derive(Clone, Copy)]
enum Guard {
    None,
    /// The cabin, its shelf and its lit panes.
    Solid,
    /// The cabin alone.
    Cabin,
}

impl Land {
    fn blocked(&self, guard: Guard, k: usize) -> bool {
        let m = self.mat[k];
        match guard {
            Guard::None => false,
            Guard::Solid => matches!(
                m,
                Mat::Wall | Mat::Roof | Mat::Pane | Mat::Door | Mat::Shore
            ),
            Guard::Cabin => matches!(m, Mat::Wall | Mat::Roof | Mat::Pane | Mat::Door),
        }
    }

    /// A spruce: tiers widening down from its tip at `tb - th`.
    fn spruce(&mut self, tx: f64, th: f64, tw: f64, tb: f64, guard: Guard) {
        let r0 = (tb - th).floor().max(0.0) as usize;
        for r in r0..WL {
            let y = r as f64 + 0.5;
            let dy = y - (tb - th);
            if dy < 0.0 || y >= tb + 0.5 {
                continue;
            }
            let tier = (dy + th * 0.3) / 1.8;
            let w = (dy / th) * tw * (0.7 + 0.45 * (tier - tier.floor())) + 0.35;
            let x0 = (tx - tw - 1.0).floor().max(0.0) as usize;
            let x1 = (WF - 1.0).min(tx + tw + 1.0);
            let mut x = x0;
            while (x as f64) <= x1 {
                let k = r * W + x;
                let ex = x as f64 + 0.5 - tx;
                if ex.abs() <= w && !self.blocked(guard, k) {
                    self.mat[k] = Mat::Tree;
                    let h = hash((x * 13 + r) as f64, 5.0);
                    let s = if ex < 0.0 { 0.7 } else { 0.3 }; // the aurora is up and left
                    self.sr[k] = (0.02 + 0.03 * s * h) as f32;
                    self.sg[k] = (0.04 + 0.05 * s) as f32;
                    self.sb[k] = (0.055 + 0.045 * s) as f32;
                    self.rec[k] = 0.05;
                    let edge = if ex < 0.0 && ex < -w + 1.0 {
                        0.45 * smooth(th, 0.0, dy)
                    } else {
                        0.0
                    };
                    self.rim[k] = smooth(1.6, 0.2, dy).max(edge) as f32;
                }
                x += 1;
            }
        }
    }

    fn build() -> Self {
        let n = W * H;
        let mut land = Land {
            mat: vec![Mat::Air; n],
            sr: vec![0.0; n],
            sg: vec![0.0; n],
            sb: vec![0.0; n],
            rec: vec![0.0; n],
            rim: vec![0.0; n],
            lamp_land: vec![0.0; WL * W],
            haze: vec![0.0; WL * W],
            star: vec![0.0; WL * W],
            gap: vec![false; H],
        };
        let mut near_top = vec![0f32; W];
        let mut far_top = vec![0f32; W];
        let mut peak_x = vec![0f32; W];
        for x in 0..W {
            let xc = x as f64 + 0.5;
            let (hl, pl) = range(xc, &LEFT, 3.1, 4.0);
            let (hr, pr) = range(xc, &RIGHT, 7.7, 4.0);
            let (hd, _) = range(xc, &DISTANT, 11.3, 2.2);
            near_top[x] = (WLF - hl.max(hr).max(0.0)) as f32;
            peak_x[x] = (if hl > hr { pl } else { pr }) as f32;
            far_top[x] = (WLF - hd.max(0.0)) as f32;
        }

        for r in 0..WL {
            for x in 0..W {
                let k = r * W + x;
                let y = r as f64 + 0.5;
                let xf = x as f64;
                let near = f64::from(near_top[x]);
                let far = f64::from(far_top[x]);
                if y >= near {
                    land.mat[k] = Mat::Near;
                    let px = f64::from(peak_x[x]);
                    let side = xf + 0.5 - px;
                    let depth = y - near;
                    let height = WLF - near;
                    // Faces turned toward the fjord catch the aurora; the
                    // ridge between the faces wanders as it comes down from
                    // the peak.
                    let ridge = px + (depth + 1.0) * 0.6 * (noise(y * 0.1, px) - 0.5);
                    let inward = if px < 100.0 { 1.0 } else { -1.0 };
                    let face = smooth(-4.0, 4.0, (xf + 0.5 - ridge) * inward);
                    // ribs and couloirs running down the fall line, each with
                    // a lit side
                    let sign = if side == 0.0 { 1.0 } else { side.signum() };
                    let u = xf + depth * 0.45 * sign;
                    let rib = |v: f64| fbm(v * 0.13, px * 0.37, 3);
                    let grad = (rib(u + 1.0) - rib(u - 1.0)) * 7.0 * inward;
                    // the snow reaches further down the ribs than the
                    // couloirs, in fingers
                    let reach = 3.0 + height * (0.62 + 0.42 * rib(u + 40.0));
                    let streak = smooth(0.56, 0.64, fbm(u * 0.32, y * 0.035 + px, 3))
                        * smooth(1.0, 5.0, depth);
                    // a stand of spruce climbing the slope behind the cabin,
                    // jagged on top
                    let cx = xf + 0.5 - 152.0;
                    let odd = if x & 1 == 1 { 1.0 } else { 0.3 };
                    let wood =
                        WLF - 18.5 + 9.0 * (cx / 19.0).powi(2) + 1.5 * (noise(xf * 0.7, 9.0) - 0.5)
                            - 2.6 * hash(xf, 77.0) * odd;
                    let snow = smooth(reach + 1.6, reach - 1.6, depth)
                        * (1.0 - smooth(WLF - 3.0, WLF - 0.5, y))
                        * (1.0 - 0.5 * streak);
                    let lit = clamp(0.35 + 0.55 * face + grad * 0.5);
                    // blue-grey rock below the snow, a little lighter on the
                    // lit faces
                    let rv = 0.75 + 0.5 * fbm(xf * 0.4, y * 0.4, 2);
                    let rr = (0.05 + 0.03 * lit) * rv;
                    let rg = (0.065 + 0.035 * lit) * rv;
                    let rb = (0.11 + 0.05 * lit) * rv;
                    let s = 0.34 + 0.66 * lit.powf(1.5);
                    land.sr[k] = mix(rr, 0.62 * s + 0.04, snow) as f32;
                    land.sg[k] = mix(rg, 0.7 * s + 0.05, snow) as f32;
                    land.sb[k] = mix(rb, 0.86 * s + 0.09, snow) as f32;
                    land.rec[k] = (snow * (0.25 + 0.75 * lit) + 0.06) as f32;
                    land.rim[k] = (smooth(2.2, 0.3, depth) * 0.5) as f32;
                    let rf = r as f64;
                    if y > wood
                        && cx > -14.0 - 2.0 * hash(rf, 3.0)
                        && cx < 13.0 + 2.0 * hash(rf, 4.0)
                    {
                        land.mat[k] = Mat::Tree;
                        let h = hash((x * 13 + r) as f64, 5.0);
                        land.sr[k] = (0.02 + 0.03 * h) as f32;
                        land.sg[k] = (0.045 + 0.045 * h) as f32;
                        land.sb[k] = (0.06 + 0.04 * h) as f32;
                        land.rec[k] = 0.05;
                        land.rim[k] = (smooth(wood + 1.6, wood + 0.2, y) * (0.7 + 0.3 * h)) as f32;
                    }
                } else if y >= far {
                    land.mat[k] = Mat::Far;
                    let depth = y - far;
                    let snow = smooth(
                        0.5,
                        0.62,
                        fbm(xf * 0.18, y * 0.25, 3) * 0.6 + (1.0 - depth / 7.0) * 0.55,
                    );
                    let lit = 0.5 + 0.3 * (noise(xf * 0.25, y * 0.1) - 0.5);
                    land.sr[k] = mix(0.075, 0.22 * lit + 0.12, snow) as f32;
                    land.sg[k] = mix(0.1, 0.27 * lit + 0.15, snow) as f32;
                    land.sb[k] = mix(0.17, 0.36 * lit + 0.24, snow) as f32;
                    land.rec[k] = (0.2 + 0.3 * snow) as f32;
                }
                // the shelf the cabin stands on, snowed over
                if x > 132 && x < 180 && y >= shore_top(xf) {
                    land.mat[k] = Mat::Shore;
                    let f = 0.6 + 0.3 * fbm(xf * 0.3, y * 0.5, 2);
                    land.sr[k] = (0.26 * f) as f32;
                    land.sg[k] = (0.3 * f) as f32;
                    land.sb[k] = (0.42 * f) as f32;
                    land.rec[k] = 0.4;
                    land.rim[k] = 0.0;
                }
            }
        }

        // a spruce treeline along the foot of both ranges, open where the
        // fjord runs in
        let mut x = -1.0f64;
        while x < WF + 2.0 {
            let h = hash((x * 7.0).floor(), 21.0);
            let open = smooth(96.0, 80.0, x) + smooth(122.0, 136.0, x);
            let on_shelf = x > 132.0 && x < 180.0;
            if open > 0.05 && !(x > 136.0 && x < 166.0) {
                let tall = if hash((x * 5.0).floor(), 8.0) > 0.7 {
                    2.4
                } else {
                    0.0
                };
                let shelf = if on_shelf { 0.7 } else { 1.0 };
                let th = (3.2 + hash((x * 3.0).floor(), 5.0) * 3.0 + tall) * open.min(1.0) * shelf;
                if th > 1.5 {
                    let (tb, guard) = match on_shelf {
                        true => (shore_top(x) - 0.4, Guard::Solid),
                        false => (WLF + 0.3, Guard::None),
                    };
                    land.spruce(
                        x + hash(x.floor(), 2.0),
                        th,
                        1.0 + hash(x.floor(), 4.0) * 0.6,
                        tb,
                        guard,
                    );
                }
            }
            x += 2.0 + 2.2 * h;
        }

        // The cabin: red boards, a snowed roof, two warm panes, a door, a
        // chimney.
        let roof_top = EAVE - 8.0;
        let chim = CAB[1] - 5;
        let chim_top = EAVE - 8.5;
        let cx = (CAB[0] + CAB[1] + 1) as f64 / 2.0;
        for r in 0..WL {
            for x in CAB[0] - 3..=CAB[1] + 3 {
                let k = r * W + x;
                let y = r as f64 + 0.5;
                let xf = x as f64;
                if x >= CAB[0] && x <= CAB[1] && y >= EAVE && y < CAB_BASE + 0.5 {
                    land.mat[k] = Mat::Wall;
                    let boards = if r & 1 == 1 { 0.82 } else { 1.0 };
                    let s = (0.5 + 0.5 * ((CAB[1] - x) as f64 / (CAB[1] - CAB[0]) as f64)) * boards;
                    land.sr[k] = (0.7 * s) as f32;
                    land.sg[k] = (0.14 * s) as f32;
                    land.sb[k] = (0.11 * s) as f32;
                    land.rim[k] = 0.0;
                    for [a, b] in PANES {
                        if x >= a && x <= b && y >= EAVE + 1.5 && y < CAB_BASE - 2.0 {
                            land.mat[k] = Mat::Pane;
                        }
                    }
                    if x >= DOOR_X[0] && x <= DOOR_X[1] && y >= EAVE + 1.5 {
                        land.mat[k] = Mat::Door;
                        land.sr[k] = 0.16;
                        land.sg[k] = 0.06;
                        land.sb[k] = 0.05;
                    }
                }
                let half = 12.5 - (EAVE - y) * 1.45;
                if y >= roof_top && y < EAVE && (xf + 0.5 - cx).abs() <= half {
                    land.mat[k] = Mat::Roof;
                    let snowy = y < EAVE - 1.0;
                    let s = 0.78 + 0.22 * ((cx - xf - 0.5) / 12.0);
                    match snowy {
                        true => {
                            land.sr[k] = (0.78 * s) as f32;
                            land.sg[k] = (0.84 * s) as f32;
                            land.sb[k] = (0.96 * s) as f32;
                        }
                        false => {
                            land.sr[k] = 0.12;
                            land.sg[k] = 0.05;
                            land.sb[k] = 0.06;
                        }
                    }
                    land.rec[k] = if snowy { 0.5 } else { 0.0 };
                    land.rim[k] = 0.0;
                }
                if x >= chim && x <= chim + 1 && y >= chim_top && y < EAVE - 4.0 {
                    land.mat[k] = Mat::Wall;
                    let s = if x == chim { 1.0 } else { 0.55 };
                    land.sr[k] = (0.2 * s) as f32;
                    land.sg[k] = (0.21 * s) as f32;
                    land.sb[k] = (0.27 * s) as f32;
                    land.rim[k] = 0.0;
                }
            }
        }

        // a few spruce on the shelf, beside the cabin
        for [tx, th] in [
            [134.0, 7.0],
            [137.8, 10.0],
            [166.5, 9.0],
            [170.0, 6.0],
            [174.5, 8.0],
        ] {
            land.spruce(tx, th, 2.2, shore_top(tx) + 0.5, Guard::Cabin);
        }

        // the warm light the panes throw on the snow and the air around them
        for r in 0..WL {
            for x in 0..W {
                let k = r * W + x;
                let m = land.mat[k];
                if matches!(m, Mat::Pane | Mat::Wall | Mat::Roof | Mat::Door) {
                    continue;
                }
                let mut g = 0.0;
                for [lx, s] in LAMPS {
                    let wx = x as f64 + 0.5 - lx;
                    let wy = r as f64 + 0.5 - (CAB_BASE - 2.5);
                    let d = (wx * wx * 0.6 + wy * wy * 2.2).sqrt();
                    g += s * (-d / 5.5).exp() * 0.6;
                }
                let take = match m {
                    Mat::Air => 0.3,
                    Mat::Tree => 0.4,
                    Mat::Near | Mat::Far | Mat::Shore => {
                        0.75 + 0.5 * hash(x as f64, (r + 31) as f64)
                    }
                    Mat::Pane | Mat::Wall | Mat::Roof | Mat::Door => {
                        unreachable!("the cabin itself is skipped above")
                    }
                };
                land.lamp_land[k] = (g * take) as f32;
            }
        }

        // the sky's unevenness and its stars
        for k in 0..WL * W {
            let (x, r) = ((k % W) as f64, (k / W) as f64);
            land.haze[k] = fbm(x * 0.04, r * 0.07, 3) as f32;
            let h = hash(x, r + 101.0);
            if h > 0.986 {
                land.star[k] = (0.35 + (h - 0.986) * 45.0) as f32;
            }
        }
        for r in WL..H {
            land.gap[r] = hash(r as f64, 404.0) < 0.3;
        }
        land
    }
}

fn land() -> &'static Land {
    static LAND: OnceLock<Land> = OnceLock::new();
    LAND.get_or_init(Land::build)
}

const RAYS: usize = 4 * W;

fn ray(arr: &[f32], u: f64) -> f64 {
    let s = u * 4.0;
    let i = s.floor();
    let f = s - i;
    let i = (i as i64).rem_euclid(RAYS as i64) as usize;
    f64::from(arr[i]) + (f64::from(arr[i + 1]) - f64::from(arr[i])) * f
}

/// The scene at `t` seconds of play time, shaded per square cell.
pub(crate) fn frame(t: f64) -> ShadedFrame {
    let land = land();
    let mat = &land.mat;

    // --- the curtains -------------------------------------------------------
    let mut base_a = vec![0f32; W];
    let mut tall_a = vec![0f32; W];
    let mut env_a = vec![0f32; W];
    let mut base_b = vec![0f32; W];
    let mut env_b = vec![0f32; W];
    for x in 0..W {
        let xf = x as f64;
        let u = xf / WF;
        // the main curtain sweeps down from the upper left, low over the
        // fjord, and lifts again to the right; ripples travel along it and
        // fold it
        base_a[x] = (5.0 + 40.0 * ((u / 0.6).min(1.0) * PI / 2.0).sin().powf(1.5)
            - 13.0 * smooth(0.6, 0.95, u)
            + 2.6 * (xf * 0.07 - t * 0.55).sin()
            + 1.4 * (xf * 0.17 + t * 0.9 + 1.3).sin()
            + 2.2 * (xf * 0.22 + t * 1.2).sin()
            + 4.0 * (fbm(xf * 0.015 + t * 0.03, 4.2, 2) - 0.5)) as f32;
        tall_a[x] = (11.0 + 8.0 * fbm(xf * 0.03 - t * 0.05, 1.7, 2)) as f32;
        env_a[x] = (smooth(0.0, 0.2, u)
            * smooth(0.98, 0.72, u)
            * (0.4 + 0.8 * fbm(xf * 0.022 - t * 0.07, 8.8, 3))) as f32;
        // a fainter curtain behind, higher up, on the right
        base_b[x] = (14.0
            + 4.0 * (xf * 0.035 + t * 0.3 + 2.0).sin()
            + 1.6 * (xf * 0.11 - t * 0.7).sin()) as f32;
        env_b[x] = (smooth(0.45, 0.7, u)
            * smooth(1.05, 0.85, u)
            * (0.25 + 0.5 * fbm(xf * 0.03 + t * 0.05, 3.3, 2))) as f32;
    }
    let mut rays_a = vec![0f32; RAYS + 2];
    let mut rays_b = vec![0f32; RAYS + 2];
    for i in 0..=RAYS + 1 {
        let u = i as f64 / 4.0;
        rays_a[i] = (0.14 + fbm(u * 0.6 + t * 0.35, t * 0.12, 3).powf(2.4) * 2.5) as f32;
        rays_b[i] = (0.1 + fbm(u * 0.45 - t * 0.2, 5.0 + t * 0.1, 3).powf(2.2) * 2.0) as f32;
    }
    // the aurora's light falling on the land below
    let mut light_x = vec![0f32; W];
    for x in 0..W {
        let mut s = 0.0;
        let mut d = -24i64;
        while d <= 24 {
            let xx = (x as i64 + d).clamp(0, W as i64 - 1) as usize;
            s += f64::from(env_a[xx]) + 0.4 * f64::from(env_b[xx]);
            d += 6;
        }
        light_x[x] = (s / 9.0) as f32;
    }
    let flick = 0.92 + 0.05 * (t * 2.3).sin() + 0.03 * (t * 7.1).sin();

    // --- sky and land -------------------------------------------------------
    let mut sky_r = vec![0f32; WL * W];
    let mut sky_g = vec![0f32; WL * W];
    let mut sky_b = vec![0f32; WL * W];
    for r in 0..WL {
        let y = r as f64 + 0.5;
        let v = y / WLF;
        for x in 0..W {
            let k = r * W + x;
            let m = mat[k];
            let base_ax = f64::from(base_a[x]);
            let env_ax = f64::from(env_a[x]);
            let light = f64::from(light_x[x]);
            let mut cr: f64;
            let mut cg: f64;
            let mut cb: f64;
            match m {
                Mat::Air => {
                    let hz = 0.85 + 0.3 * f64::from(land.haze[k]);
                    cr = (0.025 + 0.03 * v * v) * hz;
                    cg = (0.04 + 0.07 * v * v) * hz;
                    cb = (0.1 + 0.11 * v * v) * hz;
                    // curtain A: a sharp lower hem, rays rising and fading to
                    // violet
                    let mut a = 0.0;
                    let d = base_ax - y;
                    if d > -4.0 && d < 46.0 {
                        let bend = (f64::from(base_a[(x + 1).min(W - 1)])
                            - f64::from(base_a[x.saturating_sub(1)]))
                        .abs();
                        let hc = f64::from(tall_a[x]);
                        let lean = ray(&rays_a, x as f64 + d * 0.22);
                        let prof = if d < 0.0 {
                            (-d * d * 0.9).exp()
                        } else {
                            (1.0 - (-(d + 0.4) * 1.1).exp()) * (-d / hc).exp()
                        };
                        // the rays, over a continuous bright band along the hem
                        let band = if d < 0.0 {
                            (-d * d).exp()
                        } else {
                            (-d / 3.0).exp()
                        };
                        a = (prof * lean + 0.3 * band * (0.55 + 0.45 * lean.min(1.4)))
                            * env_ax
                            * (1.15 + 0.35 * bend);
                        let up = clamp(d / (hc * 1.6));
                        let gk = 1.0 - smooth(0.0, 0.5, up);
                        let vk = smooth(0.4, 0.9, up);
                        let tk = 1.0 - gk - vk;
                        cr += a * (0.18 * gk + 0.06 * tk + 0.42 * vk);
                        cg += a * (1.0 * gk + 0.78 * tk + 0.16 * vk);
                        cb += a * (0.48 * gk + 0.7 * tk + 0.75 * vk);
                        // pink at the very hem where it is brightest
                        let hem = (-((d + 0.6).powi(2)) * 1.2).exp()
                            * smooth(0.3, 0.8, a)
                            * 0.8
                            * (0.45 + 0.55 * lean.min(1.0));
                        cr += hem * 0.95;
                        cg *= 1.0 - 0.55 * (hem * 1.6).min(1.0);
                        cb += hem * 0.4;
                    }
                    // curtain B, further away, thinning out toward the top of
                    // the frame
                    let db = f64::from(base_b[x]) - y;
                    if db > -3.0 && db < 30.0 {
                        let prof = if db < 0.0 {
                            (-db * db).exp()
                        } else {
                            (1.0 - (-(db + 0.4)).exp()) * (-db / 9.0).exp()
                        };
                        let b = prof
                            * ray(&rays_b, x as f64 + db * 0.18)
                            * f64::from(env_b[x])
                            * 0.85
                            * smooth(0.0, 8.0, y);
                        let up = clamp(db / 12.0);
                        cr += b * (0.1 + 0.4 * up);
                        cg += b * (0.75 - 0.5 * up);
                        cb += b * (0.7 + 0.2 * up);
                        a += b;
                    }
                    // a little glow round the curtain, kept tight under the
                    // hem so the hem reads as an edge over dark sky
                    let below = y - base_ax;
                    let gl = env_ax
                        * if below > 0.0 {
                            (-below / 6.0).exp() * 0.1
                        } else {
                            (below / 8.0).exp() * 0.18
                        };
                    cr += gl * 0.12;
                    cg += gl * 0.62;
                    cb += gl * 0.52;
                    // airglow on the horizon
                    let ag = (-(WLF - y) / 10.0).exp() * (0.1 + 0.16 * light);
                    cg += ag * 0.7;
                    cb += ag * 0.75;
                    cr += ag * 0.2;
                    let st = f64::from(land.star[k]);
                    if st > 0.0 {
                        let (xf, rf) = (x as f64, r as f64);
                        let tw = 0.7
                            + 0.3 * (t * (1.3 + 3.0 * hash(xf, rf)) + 6.28 * hash(rf, xf)).sin();
                        let s = st * tw * clamp(1.0 - a * 1.6) * smooth(WLF - 2.0, 30.0, y);
                        cr = cr.max(s * 0.9);
                        cg = cg.max(s * 0.94);
                        cb = cb.max(s);
                    }
                }
                Mat::Near
                | Mat::Far
                | Mat::Shore
                | Mat::Wall
                | Mat::Roof
                | Mat::Pane
                | Mat::Tree
                | Mat::Door => {
                    cr = f64::from(land.sr[k]);
                    cg = f64::from(land.sg[k]);
                    cb = f64::from(land.sb[k]);
                    let l = light * f64::from(land.rec[k]);
                    cr += l * 0.03;
                    cg += l * 0.14;
                    cb += l * 0.1;
                    let e = f64::from(land.rim[k]);
                    if e > 0.0 {
                        let q = e * if m == Mat::Tree {
                            0.2 + 0.45 * light
                        } else {
                            0.05 + 0.22 * light
                        };
                        cr += q * 0.25;
                        cg += q * 0.95;
                        cb += q * 0.7;
                    }
                    if m == Mat::Pane {
                        let f = flick + 0.04 * (t * 5.3 + x as f64).sin();
                        cr = f;
                        cg = 0.76 * f;
                        cb = 0.38 * f;
                    } else if m == Mat::Door && x == DOOR_X[1] && r as f64 > EAVE + 1.0 {
                        // light through the crack of the door
                        cr = 0.55 * flick;
                        cg = 0.36 * flick;
                        cb = 0.14 * flick;
                    }
                }
            }
            let lg = f64::from(land.lamp_land[k]) * flick;
            if lg > 0.0 {
                cr += lg;
                cg += lg * 0.62;
                cb += lg * 0.26;
            }
            sky_r[k] = cr as f32;
            sky_g[k] = cg as f32;
            sky_b[k] = cb as f32;
        }
    }

    let mut cells = Vec::with_capacity(W * H);
    for r in 0..H {
        let rf = r as f64;
        let y = rf + 0.5;
        let dw = y - WLF;
        let deep = if r >= WL {
            mix(1.0, 0.45, dw / (HF - WLF))
        } else {
            1.0
        };
        let ry = WL as i64
            - 1
            - (r as i64 - WL as i64)
            - js_round(0.4 * (rf * 0.9 + t * 0.6).sin()) as i64;
        let r0 = ry.max(0) as usize * W;
        let r1 = (ry - 1).max(0) as usize * W;
        let r2 = (ry - 2).max(0) as usize * W;
        for x in 0..W {
            let k = r * W + x;
            let xf = x as f64;
            let cr: f64;
            let cg: f64;
            let cb: f64;
            let floor: f64;
            let fade: f64;
            if r < WL {
                cr = f64::from(sky_r[k]);
                cg = f64::from(sky_g[k]);
                cb = f64::from(sky_b[k]);
                floor = match mat[k] {
                    Mat::Air => 0.1,
                    Mat::Near => 0.05,
                    Mat::Tree => 0.04,
                    Mat::Far | Mat::Shore | Mat::Wall | Mat::Roof | Mat::Pane | Mat::Door => 0.06,
                };
                fade = 1.0;
            } else {
                // still water: the mirror image, stretched and broken by slow
                // ripples
                let wave = noise(xf * 0.04 + t * 0.05, rf * 0.55 - t * 0.35);
                let sx = xf + (0.5 + dw * 0.12) * (rf * 1.3 + t * 1.6 + wave * 4.0).sin();
                let ix = sx.floor();
                let fx = sx - ix;
                let ix = (ix.max(0.0) as usize).min(W - 2);
                let (a0, a1, a2) = (r0 + ix, r1 + ix, r2 + ix);
                let sky = mat[a0] == Mat::Air;
                let mut kr = (if sky { 0.55 } else { 0.64 }) * (0.82 + 0.3 * wave) * deep;
                if sky && land.gap[r] && wave < 0.45 {
                    kr *= 0.12;
                }
                // the cabin's own image is soft; the lamplight road below
                // carries it
                let ms = mat[a0];
                match ms {
                    Mat::Pane => kr *= 0.4,
                    Mat::Wall | Mat::Roof | Mat::Door => kr *= 0.7,
                    Mat::Air | Mat::Near | Mat::Far | Mat::Shore | Mat::Tree => {}
                }
                let gx = 1.0 - fx;
                let mirror = |c: &[f32]| {
                    let at = |i: usize| f64::from(c[i]);
                    ((at(a0) * gx + at(a0 + 1) * fx) * 0.5
                        + (at(a1) * gx + at(a1 + 1) * fx) * 0.3
                        + (at(a2) * gx + at(a2 + 1) * fx) * 0.2)
                        * kr
                };
                let (mut wr, mut wg, mut wb) = (mirror(&sky_r), mirror(&sky_g), mirror(&sky_b));
                if ms != Mat::Wall && ms != Mat::Door {
                    wr += 0.02;
                    wg += 0.035;
                    wb += 0.065;
                }
                // a pale line where the water meets the shore
                if r == WL {
                    let e = 0.3 * (0.3 + 0.7 * smooth(0.25, 0.75, noise(xf * 0.3, t * 0.4)));
                    wr += e * 0.6;
                    wg += e * 0.85;
                    wb += e;
                }
                // the lamplight laid on the water as a broken golden road
                if dw < 14.0 && x > 138 && x < 166 {
                    let rip = noise(xf * 0.5 - t * 0.2, rf * 1.4 - t * 1.5);
                    for [lx, s] in LAMPS {
                        let lw = 1.0 + dw * 0.1;
                        let q = (xf + 0.5 - lx) / lw;
                        let g = (-q * q).exp()
                            * (-dw / 8.0).exp()
                            * smooth(0.3, 0.65, rip)
                            * s
                            * 1.6
                            * flick;
                        wr += g;
                        wg += g * 0.68;
                        wb += g * 0.28;
                    }
                }
                cr = wr;
                cg = wg;
                cb = wb;
                floor = if sky { 0.07 * deep } else { 0.03 };
                fade = smooth(HF + 3.0, HF - 12.0, y);
            }
            let peak = cr.max(cg).max(cb).max(1e-4);
            let level = clamp(floor + (1.0 - floor) * peak.powf(0.85) * 0.95) * fade;
            cells.push(Shade {
                level,
                rgb: [cr, cg, cb],
            });
        }
    }
    ShadedFrame {
        cols: W,
        rows: H,
        cells,
    }
}

/// One cell of the halftone as the original draws it: its dot and its
/// palette colour, dithered by where the cell sits (`x`, `y`). What the
/// golden frame is compared against; the terminal draws `pixel`.
#[cfg(test)]
pub(crate) fn dot(shade: Shade, x: usize, y: usize) -> (char, u8) {
    let (step, index) = halftone(shade, x, y);
    (DOTS[step], index)
}

/// The same cell as one solid pixel: its dot's colour spread over the cell,
/// so the dot's size becomes brightness over the ground. What the terminal
/// draws (`ui::draw_shaded`), where a cell is two of these stacked and a
/// glyph cannot reach into the next row the way the original's canvas lets
/// its dots touch.
pub(crate) fn pixel(shade: Shade, x: usize, y: usize) -> [u8; 3] {
    let (step, index) = halftone(shade, x, y);
    let cover = COVER[step];
    let ink = PALETTE[index as usize];
    let mut out = [0u8; 3];
    for c in 0..3 {
        let ground = f64::from(GROUND[c]);
        out[c] = js_round(ground + (f64::from(ink[c]) - ground) * cover) as u8;
    }
    out
}

/// The halftone step (0 for no dot, 3 for the largest) and palette colour
/// of a cell.
fn halftone(shade: Shade, x: usize, y: usize) -> (usize, u8) {
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
        step,
        nearest(clamp(cr * s), clamp(cg * s), clamp(cb * s)),
    )
}

/// The palette colour nearest a colour, weighted toward green the way the
/// eye is.
fn nearest(r: f64, g: f64, b: f64) -> u8 {
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
#[path = "aurora_fjord_test.rs"]
mod aurora_fjord_test;
