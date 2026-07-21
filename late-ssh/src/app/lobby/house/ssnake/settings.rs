use serde_json::{Value, json};

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
pub const MAX_TABLE_SEATS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SsnakeTableSettings {
    pub speed: SsnakeSpeed,
    /// 0-based index into `levels::LEVELS`; `None` draws a random arena each
    /// match. Persisted as `"level": <number>`; absent or invalid = random.
    pub level: Option<usize>,
    /// Table size, 2-4 snakes. Persisted as `"seats": <number>`.
    pub seats: usize,
}

impl Default for SsnakeTableSettings {
    fn default() -> Self {
        Self {
            speed: SsnakeSpeed::default(),
            level: None,
            seats: MIN_TABLE_SEATS,
        }
    }
}

impl SsnakeTableSettings {
    pub fn to_json(self) -> Value {
        match self.level {
            Some(level) => {
                json!({ "speed": self.speed.id(), "level": level, "seats": self.seats })
            }
            None => json!({ "speed": self.speed.id(), "seats": self.seats }),
        }
    }

    pub fn from_json(value: &Value) -> Self {
        let speed = value
            .get("speed")
            .and_then(Value::as_str)
            .and_then(SsnakeSpeed::from_id)
            .unwrap_or_default();
        let level = value
            .get("level")
            .and_then(Value::as_u64)
            .map(|level| level as usize)
            .filter(|level| *level < crate::app::lobby::house::ssnake::levels::LEVELS.len());
        let seats = value
            .get("seats")
            .and_then(Value::as_u64)
            .map(|seats| seats as usize)
            .unwrap_or(MIN_TABLE_SEATS)
            .clamp(MIN_TABLE_SEATS, MAX_TABLE_SEATS);
        Self {
            speed,
            level,
            seats,
        }
    }

    pub fn label(self) -> String {
        format!("{} · {}", self.speed.label(), self.level_label())
    }

    pub fn level_label(self) -> String {
        self.level
            .and_then(|level| crate::app::lobby::house::ssnake::levels::LEVELS.get(level))
            .map(|level| level.name.clone())
            .unwrap_or_else(|| "random arena".to_string())
    }
}
