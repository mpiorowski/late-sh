//! Building an opening rack, and the only randomness in the kernel.
//!
//! The shuffle uses a SplitMix64 written out here rather than `rand`. That is
//! deliberate: the replay contract needs the same seed to produce the same
//! rack for the lifetime of a match, and `rand`'s generators carry no
//! cross-version stability guarantee. Ten lines of arithmetic that will never
//! change is a better trade than a dependency that might.
//!
//! Balls carry their printed numbers: 0 is the cue ball, 1..=7 solids, 8 the
//! black, 9..=15 stripes. Nine-ball uses 1..=9 out of the same numbering.

use crate::app::games::pool_core::{
    ball::{Ball, CUE},
    rules_snooker::{BLACK, BLUE, BROWN, GREEN, PINK, RED_FIRST, YELLOW},
    shot::RackState,
    table::TableSpec,
};

/// Balls are racked a hair apart rather than exactly touching. A rack built
/// with zero clearance starts every ball in contact with its neighbours,
/// which the simulator would spend its first steps pushing apart.
const RACK_CLEARANCE: f64 = 1.002;

/// Where the apex ball sits, as a fraction of table length from the head end.
const FOOT_SPOT_X: f64 = 0.75;
/// Where the cue ball is placed to break, as a fraction of table length.
const BREAK_SPOT_X: f64 = 0.22;
/// The black's spot, as a fraction of table length back from the top cushion.
/// On a full table that is 324mm of 3569mm.
const BLACK_SPOT_BACK: f64 = 0.0908;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackKind {
    /// Fifteen balls in a triangle: apex on the spot, the 8 in the middle of
    /// the third row, one solid and one stripe in the back corners.
    EightBall,
    /// Nine balls in a diamond: the 1 on the spot, the 9 in the centre.
    NineBall,
    /// Twenty-two: fifteen reds in a triangle behind the pink, the six colours
    /// on their own spots, and the cue ball in the D.
    Snooker,
}

/// A deterministic, version-stable PRNG. SplitMix64.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    /// Fisher-Yates, walking downwards so the sequence of draws is fixed.
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

/// The head spot, where the cue ball is placed for the break.
pub fn break_spot(spec: &TableSpec) -> [f64; 2] {
    [spec.length * BREAK_SPOT_X, spec.width / 2.0]
}

/// The foot spot, where the apex ball is racked.
pub fn foot_spot(spec: &TableSpec) -> [f64; 2] {
    [spec.length * FOOT_SPOT_X, spec.width / 2.0]
}

/// Row/slot layout for a triangle or diamond, in row-major order starting at
/// the apex. Row `k` sits `k` rows back from the spot; within a row the slots
/// run from one side to the other.
fn slots(spec: &TableSpec, rows: &[usize]) -> Vec<[f64; 2]> {
    let r = spec.ball_radius;
    let spot = foot_spot(spec);
    let row_gap = r * 3f64.sqrt() * RACK_CLEARANCE;
    let slot_gap = 2.0 * r * RACK_CLEARANCE;

    let mut out = Vec::new();
    for (k, count) in rows.iter().copied().enumerate() {
        let x = spot[0] + k as f64 * row_gap;
        for j in 0..count {
            let offset = j as f64 - (count as f64 - 1.0) / 2.0;
            out.push([x, spot[1] + offset * slot_gap]);
        }
    }
    out
}

/// Build the opening rack. The same `seed` always produces the same rack.
pub fn build(spec: &TableSpec, kind: RackKind, seed: u64) -> RackState {
    let mut rng = Rng::new(seed);
    let mut balls = vec![Ball::resting(CUE, break_spot(spec))];

    match kind {
        RackKind::EightBall => {
            let positions = slots(spec, &[1, 2, 3, 4, 5]);
            let mut order = [0u8; 15];
            // Apex is the 1; the 8 takes the middle of the third row.
            order[0] = 1;
            const EIGHT_SLOT: usize = 4; // rows 1 + 2 before it, then j = 1
            order[EIGHT_SLOT] = 8;

            let mut pool: Vec<u8> = (2..=7).chain(9..=15).collect();
            rng.shuffle(&mut pool);
            let mut it = pool.into_iter();
            for (slot, entry) in order.iter_mut().enumerate() {
                if slot == 0 || slot == EIGHT_SLOT {
                    continue;
                }
                *entry = it.next().expect("14 balls fill 13 free slots");
            }
            fix_back_corners(&mut order, &mut rng);

            for (id, pos) in order.into_iter().zip(positions) {
                balls.push(Ball::resting(id, pos));
            }
        }
        RackKind::NineBall => {
            let positions = slots(spec, &[1, 2, 3, 2, 1]);
            let mut order = [0u8; 9];
            order[0] = 1;
            const NINE_SLOT: usize = 4; // rows 1 + 2 before it, then j = 1
            order[NINE_SLOT] = 9;

            let mut pool: Vec<u8> = (2..=8).collect();
            rng.shuffle(&mut pool);
            let mut it = pool.into_iter();
            for (slot, entry) in order.iter_mut().enumerate() {
                if slot == 0 || slot == NINE_SLOT {
                    continue;
                }
                *entry = it.next().expect("7 balls fill 7 free slots");
            }

            for (id, pos) in order.into_iter().zip(positions) {
                balls.push(Ball::resting(id, pos));
            }
        }
        RackKind::Snooker => {
            // Nothing is shuffled: a snooker rack is the same every frame, and
            // the six colours have named spots of their own.
            for (id, at) in colour_spots(spec) {
                balls.push(Ball::resting(id, at));
            }
            // The reds sit behind the pink, apex first, on the same triangle
            // the pool games use — the pink stands on its foot spot, so the
            // whole thing shifts back by a ball to clear it.
            let clearance = 2.0 * spec.ball_radius * RACK_CLEARANCE;
            for (index, mut at) in slots(spec, &[1, 2, 3, 4, 5]).into_iter().enumerate() {
                at[0] += clearance;
                balls.push(Ball::resting(RED_FIRST + index as u8, at));
            }
            // And the cue ball breaks from the D rather than from the head
            // spot the pool games use.
            balls[0].pos = d_spot(spec);
        }
    }

    RackState { balls }
}

/// The six colours on their spots, in ascending value.
///
/// Yellow and green sit at the ends of the D, brown on the baulk line between
/// them, blue on the centre spot, pink midway between centre and the top
/// cushion, and black on its own spot near it.
pub fn colour_spots(spec: &TableSpec) -> [(u8, [f64; 2]); 6] {
    let mid = spec.width / 2.0;
    let head = spec.head_string;
    [
        (YELLOW, [head, mid - spec.d_radius]),
        (GREEN, [head, mid + spec.d_radius]),
        (BROWN, [head, mid]),
        (BLUE, [spec.length / 2.0, mid]),
        (PINK, foot_spot(spec)),
        (BLACK, [spec.length * (1.0 - BLACK_SPOT_BACK), mid]),
    ]
}

/// Where the cue ball starts a snooker frame: inside the D, on the centre
/// line, a little behind the baulk line.
pub fn d_spot(spec: &TableSpec) -> [f64; 2] {
    [spec.head_string - spec.d_radius * 0.45, spec.width / 2.0]
}

pub fn is_solid(id: u8) -> bool {
    (1..=7).contains(&id)
}

pub fn is_stripe(id: u8) -> bool {
    (9..=15).contains(&id)
}

/// The back row's two corners must be one solid and one stripe. The shuffle
/// does not know that, so this repairs it afterwards by trading one corner
/// with an interior ball of the group that is missing.
fn fix_back_corners(order: &mut [u8; 15], rng: &mut Rng) {
    const LEFT: usize = 10;
    const RIGHT: usize = 14;
    if is_solid(order[LEFT]) != is_solid(order[RIGHT]) {
        return;
    }
    let want_stripe = is_solid(order[LEFT]);
    // Trade the right-hand corner. Interior slots exclude the apex, the 8,
    // and both corners.
    let candidates: Vec<usize> = (1..15)
        .filter(|s| *s != 4 && *s != LEFT && *s != RIGHT)
        .filter(|s| {
            if want_stripe {
                is_stripe(order[*s])
            } else {
                is_solid(order[*s])
            }
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    let pick = candidates[rng.below(candidates.len())];
    order.swap(RIGHT, pick);
}
