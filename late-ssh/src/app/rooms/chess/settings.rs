use serde_json::{Value, json};

pub const TIME_CONTROL_OPTIONS: [ChessTimeControl; 7] = [
    ChessTimeControl::Blitz3Plus2,
    ChessTimeControl::Blitz5Plus0,
    ChessTimeControl::Blitz5Plus3,
    ChessTimeControl::Rapid10Plus0,
    ChessTimeControl::Rapid15Plus10,
    ChessTimeControl::Rapid30Plus0,
    ChessTimeControl::Daily1Move,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChessTimeControl {
    Blitz3Plus2,
    Blitz5Plus0,
    Blitz5Plus3,
    #[default]
    Rapid10Plus0,
    Rapid15Plus10,
    Rapid30Plus0,
    Daily1Move,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChessClockMode {
    Countdown { base_secs: u64, increment_secs: u64 },
    Daily { move_secs: u64 },
}

impl ChessTimeControl {
    pub fn id(self) -> &'static str {
        match self {
            Self::Blitz3Plus2 => "blitz_3_2",
            Self::Blitz5Plus0 => "blitz_5_0",
            Self::Blitz5Plus3 => "blitz_5_3",
            Self::Rapid10Plus0 => "rapid_10_0",
            Self::Rapid15Plus10 => "rapid_15_10",
            Self::Rapid30Plus0 => "rapid_30_0",
            Self::Daily1Move => "daily_1d",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Blitz3Plus2 => "3+2 blitz",
            Self::Blitz5Plus0 => "5+0 blitz",
            Self::Blitz5Plus3 => "5+3 blitz",
            Self::Rapid10Plus0 => "10+0 rapid",
            Self::Rapid15Plus10 => "15+10 rapid",
            Self::Rapid30Plus0 => "30+0 rapid",
            Self::Daily1Move => "1d/move daily",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Blitz3Plus2 => "3+2",
            Self::Blitz5Plus0 => "5+0",
            Self::Blitz5Plus3 => "5+3",
            Self::Rapid10Plus0 => "10+0",
            Self::Rapid15Plus10 => "15+10",
            Self::Rapid30Plus0 => "30+0",
            Self::Daily1Move => "1d/move",
        }
    }

    pub fn mode(self) -> ChessClockMode {
        match self {
            Self::Blitz3Plus2 => ChessClockMode::Countdown {
                base_secs: 3 * 60,
                increment_secs: 2,
            },
            Self::Blitz5Plus0 => ChessClockMode::Countdown {
                base_secs: 5 * 60,
                increment_secs: 0,
            },
            Self::Blitz5Plus3 => ChessClockMode::Countdown {
                base_secs: 5 * 60,
                increment_secs: 3,
            },
            Self::Rapid10Plus0 => ChessClockMode::Countdown {
                base_secs: 10 * 60,
                increment_secs: 0,
            },
            Self::Rapid15Plus10 => ChessClockMode::Countdown {
                base_secs: 15 * 60,
                increment_secs: 10,
            },
            Self::Rapid30Plus0 => ChessClockMode::Countdown {
                base_secs: 30 * 60,
                increment_secs: 0,
            },
            Self::Daily1Move => ChessClockMode::Daily {
                move_secs: 24 * 60 * 60,
            },
        }
    }

    pub fn from_id(value: &str) -> Option<Self> {
        TIME_CONTROL_OPTIONS
            .iter()
            .copied()
            .find(|option| option.id() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChessTableSettings {
    pub time_control: ChessTimeControl,
}

impl ChessTableSettings {
    pub fn to_json(self) -> Value {
        json!({
            "time_control": self.time_control.id(),
        })
    }

    pub fn from_json(value: &Value) -> Self {
        let time_control = value
            .get("time_control")
            .and_then(Value::as_str)
            .and_then(ChessTimeControl::from_id)
            .unwrap_or_default();
        Self { time_control }
    }
}

impl Default for ChessTableSettings {
    fn default() -> Self {
        Self {
            time_control: ChessTimeControl::default(),
        }
    }
}
