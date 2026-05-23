use ratatui::style::Color;
use uuid::Uuid;

pub const WATER_HEIGHT: u16 = 2;
pub const POS_SCALE: i64 = 1000;

const FISH_MIN: usize = 5;
const FISH_MAX: usize = 8;
const SPEED_MIN: i32 = 60;
const SPEED_MAX: i32 = 200;

#[derive(Clone, Copy, Debug)]
pub enum Species {
    Common,
    Long,
}

#[derive(Clone, Debug)]
pub struct Fish {
    pub x_milli: i64,
    pub speed_milli: i32,
    pub species: Species,
    pub colour: Color,
}

#[derive(Clone, Debug)]
pub struct WaterState {
    pub fish: Vec<Fish>,
    pub ticks: u64,
}

impl WaterState {
    pub fn new_for_user(user_id: Uuid) -> Self {
        let mut rng = Xor64::from_uuid(user_id);
        let count = FISH_MIN + (rng.next() as usize % (FISH_MAX - FISH_MIN + 1));
        let palette: &[Color] = &[
            Color::LightCyan,
            Color::Yellow,
            Color::LightMagenta,
            Color::LightGreen,
            Color::Cyan,
            Color::LightYellow,
            Color::LightRed,
            Color::LightBlue,
        ];
        let mut fish = Vec::with_capacity(count);
        for _ in 0..count {
            let speed_mag = SPEED_MIN
                + (rng.next() as i32).rem_euclid(SPEED_MAX - SPEED_MIN + 1);
            let going_right = rng.next() & 1 == 0;
            let speed_milli = if going_right { speed_mag } else { -speed_mag };
            let x_milli = (rng.next() % 120) as i64 * POS_SCALE;
            let species = if rng.next().is_multiple_of(4) {
                Species::Long
            } else {
                Species::Common
            };
            let colour = palette[(rng.next() as usize) % palette.len()];
            fish.push(Fish {
                x_milli,
                speed_milli,
                species,
                colour,
            });
        }
        Self { fish, ticks: 0 }
    }

    pub fn tick(&mut self) {
        self.ticks = self.ticks.wrapping_add(1);
        for f in &mut self.fish {
            f.x_milli = f.x_milli.wrapping_add(f.speed_milli as i64);
        }
    }
}

/// Tiny deterministic PRNG. Stable per-user without pulling SeedableRng plumbing.
struct Xor64(u64);

impl Xor64 {
    fn from_uuid(user_id: Uuid) -> Self {
        let bytes = user_id.as_bytes();
        let mut seed: u64 = 0xcbf29ce484222325; // FNV offset basis as a non-zero start
        for b in bytes {
            seed = seed.wrapping_mul(0x100000001b3);
            seed ^= *b as u64;
        }
        if seed == 0 {
            seed = 0xdeadbeefcafebabe;
        }
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_per_user() {
        let id = Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
        let a = WaterState::new_for_user(id);
        let b = WaterState::new_for_user(id);
        assert_eq!(a.fish.len(), b.fish.len());
        for (fa, fb) in a.fish.iter().zip(b.fish.iter()) {
            assert_eq!(fa.x_milli, fb.x_milli);
            assert_eq!(fa.speed_milli, fb.speed_milli);
        }
    }

    #[test]
    fn different_users_get_different_shoals() {
        let a = WaterState::new_for_user(Uuid::from_u128(1));
        let b = WaterState::new_for_user(Uuid::from_u128(2));
        let positions_a: Vec<_> = a.fish.iter().map(|f| (f.x_milli, f.speed_milli)).collect();
        let positions_b: Vec<_> = b.fish.iter().map(|f| (f.x_milli, f.speed_milli)).collect();
        assert_ne!(positions_a, positions_b);
    }

    #[test]
    fn tick_advances_positions() {
        let mut s = WaterState::new_for_user(Uuid::from_u128(42));
        let before: Vec<i64> = s.fish.iter().map(|f| f.x_milli).collect();
        s.tick();
        let after: Vec<i64> = s.fish.iter().map(|f| f.x_milli).collect();
        assert_ne!(before, after);
        assert_eq!(s.ticks, 1);
    }
}
