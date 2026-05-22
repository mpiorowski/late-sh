use serde_json::{Value, json};

pub const TIME_CONTROL_OPTIONS: [ChessTimeControl; 3] = [
    ChessTimeControl::Blitz,
    ChessTimeControl::Rapid,
    ChessTimeControl::Daily,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChessTimeControl {
    #[default]
    Rapid,
    Blitz,
    Daily,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChessClockMode {
    Countdown { base_secs: u64, increment_secs: u64 },
    Daily { move_secs: u64 },
}

impl ChessTimeControl {
    pub fn id(self) -> &'static str {
        match self {
            Self::Blitz => "blitz",
            Self::Rapid => "rapid",
            Self::Daily => "daily",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Blitz => "Blitz (5+3)",
            Self::Rapid => "Rapid (15+10)",
            Self::Daily => "Daily (1d/move)",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            Self::Blitz => "blitz 5+3",
            Self::Rapid => "rapid 15+10",
            Self::Daily => "daily 1d",
        }
    }

    pub fn mode(self) -> ChessClockMode {
        match self {
            Self::Blitz => ChessClockMode::Countdown {
                base_secs: 5 * 60,
                increment_secs: 3,
            },
            Self::Rapid => ChessClockMode::Countdown {
                base_secs: 15 * 60,
                increment_secs: 10,
            },
            Self::Daily => ChessClockMode::Daily {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
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
        let id = value
            .get("time_control")
            .and_then(Value::as_str)
            .expect("chess settings require a time_control");
        let time_control = ChessTimeControl::from_id(id)
            .expect("chess time_control must be one of: blitz, rapid, daily");
        Self { time_control }
    }
}
