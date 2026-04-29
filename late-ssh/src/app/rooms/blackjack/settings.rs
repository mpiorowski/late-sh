use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const STAKE_OPTIONS: [i64; 4] = [10, 50, 100, 1000];
pub const PACE_OPTIONS: [BlackjackPace; 3] = [
    BlackjackPace::Quick,
    BlackjackPace::Standard,
    BlackjackPace::Chill,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlackjackPace {
    Quick,
    Standard,
    Chill,
}

impl BlackjackPace {
    pub fn label(self) -> &'static str {
        match self {
            Self::Quick => "Quick",
            Self::Standard => "Standard",
            Self::Chill => "Chill",
        }
    }

    pub fn table_label(self) -> &'static str {
        match self {
            Self::Quick => "~2m / round",
            Self::Standard => "~5m / round",
            Self::Chill => "~10m / round",
        }
    }

    pub fn betting_window_secs(self) -> u64 {
        match self {
            Self::Quick => 3,
            Self::Standard => 5,
            Self::Chill => 10,
        }
    }
}

impl Default for BlackjackPace {
    fn default() -> Self {
        Self::Standard
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlackjackTableSettings {
    pub pace: BlackjackPace,
    pub stake: i64,
}

impl BlackjackTableSettings {
    pub fn from_json(value: &Value) -> Self {
        serde_json::from_value::<Self>(value.clone())
            .unwrap_or_default()
            .normalized()
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self.clone().normalized()).unwrap_or_else(|_| serde_json::json!({}))
    }

    pub fn normalized(mut self) -> Self {
        if !STAKE_OPTIONS.contains(&self.stake) {
            self.stake = Self::default().stake;
        }
        self
    }

    pub fn min_bet(&self) -> i64 {
        self.normalized_ref().stake
    }

    pub fn max_bet(&self) -> i64 {
        self.min_bet() * 10
    }

    pub fn chip_denominations(&self) -> Vec<i64> {
        let stake = self.min_bet();
        vec![stake, stake * 2, stake * 5, stake * 10]
    }

    pub fn stake_label(&self) -> String {
        format!("{} chips", self.min_bet())
    }

    pub fn pace_label(&self) -> &'static str {
        self.pace.table_label()
    }

    pub fn betting_window_secs(&self) -> u64 {
        self.pace.betting_window_secs()
    }

    fn normalized_ref(&self) -> Self {
        self.clone().normalized()
    }
}

impl Default for BlackjackTableSettings {
    fn default() -> Self {
        Self {
            pace: BlackjackPace::Standard,
            stake: 10,
        }
    }
}
