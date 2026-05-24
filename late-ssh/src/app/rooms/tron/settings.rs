use serde_json::{Value, json};

pub const SPEED_OPTIONS: [TronSpeed; 3] = [TronSpeed::Chill, TronSpeed::Standard, TronSpeed::Quick];
pub const MODE_OPTIONS: [TronMode; 3] = [TronMode::Classic, TronMode::Gaps, TronMode::Glitch];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TronSpeed {
    Chill,
    #[default]
    Standard,
    Quick,
}

impl TronSpeed {
    pub fn id(self) -> &'static str {
        match self {
            Self::Chill => "chill",
            Self::Standard => "standard",
            Self::Quick => "quick",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Chill => "chill",
            Self::Standard => "standard",
            Self::Quick => "quick",
        }
    }

    pub fn tick_millis(self) -> u64 {
        match self {
            Self::Chill => 700,
            Self::Standard => 450,
            Self::Quick => 275,
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        SPEED_OPTIONS
            .iter()
            .copied()
            .find(|option| option.id() == value)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TronMode {
    Classic,
    Gaps,
    #[default]
    Glitch,
}

impl TronMode {
    pub fn id(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Gaps => "gaps",
            Self::Glitch => "glitch",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Gaps => "gaps",
            Self::Glitch => "glitch",
        }
    }

    pub fn has_gaps(self) -> bool {
        matches!(self, Self::Gaps | Self::Glitch)
    }

    pub fn has_pickups(self) -> bool {
        matches!(self, Self::Glitch)
    }

    pub fn from_id(value: &str) -> Option<Self> {
        MODE_OPTIONS
            .iter()
            .copied()
            .find(|option| option.id() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TronTableSettings {
    pub speed: TronSpeed,
    pub mode: TronMode,
}

impl Default for TronTableSettings {
    fn default() -> Self {
        Self {
            speed: TronSpeed::Standard,
            mode: TronMode::Glitch,
        }
    }
}

impl TronTableSettings {
    pub fn to_json(self) -> Value {
        json!({
            "speed": self.speed.id(),
            "mode": self.mode.id(),
        })
    }

    pub fn from_json(value: &Value) -> Self {
        let speed = value
            .get("speed")
            .and_then(Value::as_str)
            .and_then(TronSpeed::from_id)
            .unwrap_or_default();
        let mode = value
            .get("mode")
            .and_then(Value::as_str)
            .and_then(TronMode::from_id)
            .unwrap_or(TronMode::Classic);
        Self { speed, mode }
    }

    pub fn label(self) -> String {
        format!("{} · {}", self.speed.label(), self.mode.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_speed() {
        let settings = TronTableSettings {
            speed: TronSpeed::Quick,
            mode: TronMode::Gaps,
        };
        assert_eq!(TronTableSettings::from_json(&settings.to_json()), settings);
    }

    #[test]
    fn unknown_values_fall_back_to_safe_defaults() {
        let settings =
            TronTableSettings::from_json(&json!({ "speed": "warp", "mode": "overdrive" }));
        assert_eq!(settings.speed, TronSpeed::Standard);
        assert_eq!(settings.mode, TronMode::Classic);
    }

    #[test]
    fn missing_mode_preserves_legacy_classic_rooms() {
        let settings = TronTableSettings::from_json(&json!({ "speed": "quick" }));
        assert_eq!(settings.speed, TronSpeed::Quick);
        assert_eq!(settings.mode, TronMode::Classic);
    }
}
