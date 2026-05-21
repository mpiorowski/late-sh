use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SMALL_BLIND_OPTIONS: [i64; 4] = [10, 25, 50, 100];
pub const STARTING_STACK_OPTIONS: [i64; 5] = [100, 500, 1_000, 2_000, 5_000];
pub const PACE_OPTIONS: [PokerPace; 3] = [PokerPace::Quick, PokerPace::Standard, PokerPace::Chill];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PokerPace {
    Quick,
    #[default]
    Standard,
    Chill,
}

impl PokerPace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quick => "Quick",
            Self::Standard => "Standard",
            Self::Chill => "Chill",
        }
    }

    pub fn table_label(self) -> &'static str {
        match self {
            Self::Quick => "20s action timer",
            Self::Standard => "45s action timer",
            Self::Chill => "90s action timer",
        }
    }

    pub fn action_timeout_secs(self) -> u64 {
        match self {
            Self::Quick => 20,
            Self::Standard => 45,
            Self::Chill => 90,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PokerTableSettings {
    pub pace: PokerPace,
    pub small_blind: i64,
    pub starting_stack: i64,
}

impl PokerTableSettings {
    pub fn from_json(value: &Value) -> Self {
        let default = Self::default();
        let pace = value
            .get("pace")
            .and_then(|value| serde_json::from_value::<PokerPace>(value.clone()).ok())
            .unwrap_or(default.pace);
        let small_blind = value
            .get("small_blind")
            .and_then(Value::as_i64)
            .unwrap_or(default.small_blind);
        let starting_stack = value
            .get("starting_stack")
            .and_then(Value::as_i64)
            .unwrap_or(default.starting_stack);

        Self {
            pace,
            small_blind,
            starting_stack,
        }
        .normalized()
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.clone().normalized()).unwrap_or_else(|_| serde_json::json!({}))
    }

    pub fn normalized(mut self) -> Self {
        if !SMALL_BLIND_OPTIONS.contains(&self.small_blind) {
            self.small_blind = Self::default().small_blind;
        }
        if !STARTING_STACK_OPTIONS.contains(&self.starting_stack) {
            self.starting_stack = Self::default().starting_stack;
        }
        self
    }

    pub fn small_blind(&self) -> i64 {
        self.normalized_ref().small_blind
    }

    pub fn big_blind(&self) -> i64 {
        self.small_blind() * 2
    }

    pub fn starting_stack(&self) -> i64 {
        self.normalized_ref().starting_stack
    }

    pub fn stake_label(&self) -> String {
        format!("{} stack", self.starting_stack())
    }

    pub fn blind_label(&self) -> String {
        format!("{}/{} blinds", self.small_blind(), self.big_blind())
    }

    pub fn pace_label(&self) -> &'static str {
        self.pace.table_label()
    }

    pub fn action_timeout_secs(&self) -> u64 {
        self.pace.action_timeout_secs()
    }

    /// Compact one-liner shown in the chat seat-joined card.
    pub fn meta_label(&self) -> String {
        format!(
            "{} · {} · {}s/turn",
            self.stake_label(),
            self.blind_label(),
            self.action_timeout_secs()
        )
    }

    fn normalized_ref(&self) -> Self {
        self.clone().normalized()
    }
}

impl Default for PokerTableSettings {
    fn default() -> Self {
        Self {
            pace: PokerPace::Standard,
            small_blind: 10,
            starting_stack: 1_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_small_blind_falls_back_to_default() {
        let settings = PokerTableSettings::from_json(&serde_json::json!({
            "pace": "standard",
            "small_blind": 999
        }));

        assert_eq!(settings.small_blind(), 10);
        assert_eq!(settings.big_blind(), 20);
        assert_eq!(settings.starting_stack(), 1_000);
    }

    #[test]
    fn invalid_values_fall_back_independently() {
        let settings = PokerTableSettings::from_json(&serde_json::json!({
            "pace": "typo",
            "small_blind": 50,
            "starting_stack": 123
        }));

        assert_eq!(settings.pace, PokerPace::Standard);
        assert_eq!(settings.small_blind(), 50);
        assert_eq!(settings.big_blind(), 100);
        assert_eq!(settings.starting_stack(), 1_000);
    }

    #[test]
    fn labels_include_stack_and_blinds() {
        let settings = PokerTableSettings {
            pace: PokerPace::Quick,
            small_blind: 50,
            starting_stack: 5_000,
        };

        assert_eq!(settings.stake_label(), "5000 stack");
        assert_eq!(settings.blind_label(), "50/100 blinds");
        assert_eq!(settings.action_timeout_secs(), 20);
        assert_eq!(
            settings.meta_label(),
            "5000 stack · 50/100 blinds · 20s/turn"
        );
    }
}
