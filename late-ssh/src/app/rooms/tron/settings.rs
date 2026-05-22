use serde_json::{Value, json};

pub const SPEED_OPTIONS: [TronSpeed; 3] = [TronSpeed::Chill, TronSpeed::Standard, TronSpeed::Quick];

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
pub struct TronTableSettings {
    pub speed: TronSpeed,
}

impl TronTableSettings {
    pub fn to_json(self) -> Value {
        json!({
            "speed": self.speed.id(),
        })
    }

    pub fn from_json(value: &Value) -> Self {
        let speed = value
            .get("speed")
            .and_then(Value::as_str)
            .and_then(TronSpeed::from_id)
            .unwrap_or_default();
        Self { speed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_speed() {
        let settings = TronTableSettings {
            speed: TronSpeed::Quick,
        };
        assert_eq!(TronTableSettings::from_json(&settings.to_json()), settings);
    }

    #[test]
    fn unknown_speed_falls_back_to_default() {
        let settings = TronTableSettings::from_json(&json!({ "speed": "warp" }));
        assert_eq!(settings.speed, TronSpeed::Standard);
    }
}
