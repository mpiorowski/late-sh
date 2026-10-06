use std::time::Duration;

use serde_json::{Value, json};

// ── Arena economy ──────────────────────────────────────────────
// The perpetual arena settles chips as they are earned, so these are the
// only numbers that decide what an hour of snake is worth. Nothing else in
// the game hard-codes a chip amount — tune the balance here.

/// Chips per food, multiplied by the number of snakes actually moving when
/// it is eaten. A lone farmer earns the base rate; a busy arena pays
/// everyone more, which is the whole draw of joining a crowded board.
pub const SSNAKE_FOOD_CHIPS: i64 = 5;
/// The pink food (the old extra-life pickup) pays this many times the
/// normal rate instead of granting a life.
pub const SSNAKE_BONUS_FOOD_MULTIPLIER: i64 = 3;
/// 1-in-N chance a spawned food is the pink one. The DOS original rolled
/// 1-in-35 for its extra life, which is rare enough that a player could sit
/// through several arenas without ever seeing one — as a payout it needs to
/// show up often enough to be worth chasing. Note this lifts the effective
/// chip rate by roughly `(SSNAKE_BONUS_FOOD_MULTIPLIER - 1) / N`.
pub const SSNAKE_BONUS_FOOD_ODDS: u32 = 10;
/// Chips for eating an arena's last food, also multiplied by the moving
/// snake count. Paid to the snake that closed it out; the arena then
/// reshuffles to a new random level for everyone.
pub const SSNAKE_CLEAR_CHIPS: i64 = 50;
/// Flat penalty per crash, deliberately independent of the snake count:
/// enough to sting, never enough to wipe out a good run.
pub const SSNAKE_CRASH_CHIPS: i64 = 10;
/// Extra chips per arena wall orthogonally touching the food, added to the
/// base before any multiplier. Food wedged against a wall is harder to take
/// cleanly and a corner pickup is harder still, so the risk pays: 5 in open
/// floor, 7 against one wall, 9 in a corner.
pub const SSNAKE_EDGE_BONUS_CHIPS: i64 = 2;

/// What a food is worth, start to finish. The wall-edge bonus lands on the
/// base, pink multiplies that, and the moving-snake count multiplies the lot.
/// Both the payout and the number on screen read this, so they cannot drift.
pub fn ssnake_food_chips(wall_edges: i64, bonus_food: bool, moving_snakes: i64) -> i64 {
    let base = SSNAKE_FOOD_CHIPS + SSNAKE_EDGE_BONUS_CHIPS * wall_edges.clamp(0, 4);
    let base = match bonus_food {
        true => base * SSNAKE_BONUS_FOOD_MULTIPLIER,
        false => base,
    };
    base * moving_snakes.max(1)
}

// ── Arena pacing ───────────────────────────────────────────────

/// How long the board holds still after it reshuffles. A steer pressed on the
/// arena that just ended would otherwise carry straight into a level the
/// player has not seen yet — finish a lap hugging an edge and the new board
/// would kill you before you could look at it. Four seconds so the on-board
/// countdown gets a clean second each for 3, 2, 1, GO.
pub const SSNAKE_ARENA_FREEZE: Duration = Duration::from_secs(4);

/// Tail segments dropped per tick during the death shrink. The original ate
/// one a tick, which on a long snake is a punishing wait for something the
/// player has already lost.
pub const SSNAKE_DEATH_SHRINK_PER_TICK: usize = 3;

/// Percent of its length a snake loses on each crash. The original came back
/// exactly as long as it died, which on a big arena means an unwieldy snake
/// stays unwieldy forever and the level becomes unclearable. Shedding a fifth
/// per crash turns a bad run into a recoverable one — the chip penalty is
/// what makes crashing cost something, not the length.
pub const SSNAKE_CRASH_LENGTH_PENALTY_PCT: i32 = 20;
/// Floor for that shrink, so a snake never respawns as a bare head.
pub const SSNAKE_MIN_RESPAWN_LENGTH: i32 = 3;

/// Length a snake regrows to after crashing at `died_at` total length.
pub fn ssnake_respawn_length(died_at: i32) -> i32 {
    let kept = died_at * (100 - SSNAKE_CRASH_LENGTH_PENALTY_PCT) / 100;
    kept.clamp(
        SSNAKE_MIN_RESPAWN_LENGTH,
        died_at.max(SSNAKE_MIN_RESPAWN_LENGTH),
    )
}

// ── Skip vote ──────────────────────────────────────────────────

/// Minimum gap between vote-skips. The consensus rule alone does not stop a
/// lone player from rerolling until they land the arena they farm fastest,
/// because on an empty table they *are* the consensus. The cooldown is what
/// makes the vote a way out of a level nobody enjoys rather than a way to
/// shop for one.
pub const SSNAKE_SKIP_COOLDOWN: Duration = Duration::from_secs(60);
/// A skip vote lapses this long after it is cast, so votes cannot quietly
/// accumulate across unrelated moments and hand a later voter an instant
/// skip they did not really have a table behind.
pub const SSNAKE_SKIP_VOTE_TTL: Duration = Duration::from_secs(30);

pub const SPEED_OPTIONS: [SsnakeSpeed; 3] = [
    SsnakeSpeed::Relaxed,
    SsnakeSpeed::Classic,
    SsnakeSpeed::Swift,
];

/// Pace multiplier over each level's own `tick-millis`. Levels carry their
/// original DOS pacing; the room setting only stretches or squeezes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SsnakeSpeed {
    Relaxed,
    #[default]
    Classic,
    Swift,
}

impl SsnakeSpeed {
    pub fn id(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Classic => "classic",
            Self::Swift => "swift",
        }
    }

    pub fn label(self) -> &'static str {
        self.id()
    }

    pub fn scale_tick(self, base_millis: u64) -> u64 {
        match self {
            Self::Relaxed => base_millis * 4 / 3,
            Self::Classic => base_millis,
            Self::Swift => base_millis * 3 / 4,
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        SPEED_OPTIONS
            .iter()
            .copied()
            .find(|option| option.id() == value)
    }
}

pub const MIN_TABLE_SEATS: usize = 2;
pub const MAX_TABLE_SEATS: usize = 5;

/// The arena has no level setting: it shuffles to a new random level every
/// time one is cleared, so there is nothing to pick before playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SsnakeTableSettings {
    pub speed: SsnakeSpeed,
    /// Table size, 2-5 snakes. Persisted as `"seats": <number>`.
    pub seats: usize,
}

impl Default for SsnakeTableSettings {
    fn default() -> Self {
        Self {
            speed: SsnakeSpeed::default(),
            seats: MIN_TABLE_SEATS,
        }
    }
}

impl SsnakeTableSettings {
    pub fn to_json(self) -> Value {
        json!({ "speed": self.speed.id(), "seats": self.seats })
    }

    pub fn from_json(value: &Value) -> Self {
        let speed = value
            .get("speed")
            .and_then(Value::as_str)
            .and_then(SsnakeSpeed::from_id)
            .unwrap_or_default();
        let seats = value
            .get("seats")
            .and_then(Value::as_u64)
            .map(|seats| seats as usize)
            .unwrap_or(MIN_TABLE_SEATS)
            .clamp(MIN_TABLE_SEATS, MAX_TABLE_SEATS);
        Self { speed, seats }
    }

    pub fn label(self) -> String {
        format!("{} · shuffling arena", self.speed.label())
    }
}
