use serde::{Deserialize, Serialize};

/// Wire values are shared with late-ssh's Zork client. No filesystem paths are
/// accepted by this protocol, and exec request text is never shell-evaluated.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edition {
    Zork1,
    Zork2,
    Zork3,
}

impl Edition {
    pub const ALL: [Self; 3] = [Self::Zork1, Self::Zork2, Self::Zork3];

    pub fn key(self) -> &'static str {
        match self {
            Self::Zork1 => "zork1",
            Self::Zork2 => "zork2",
            Self::Zork3 => "zork3",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|edition| edition.key() == value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Auto,
    Manual,
    New,
}

impl Action {
    pub fn mode(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::New => "new",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Availability {
    Missing,
    Ready,
    Invalid,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Slot {
    pub status: Availability,
    #[serde(default)]
    pub saved_at: Option<u64>,
    #[serde(default)]
    pub description: String,
}

impl Slot {
    pub fn missing() -> Self {
        Self {
            status: Availability::Missing,
            saved_at: None,
            description: String::new(),
        }
    }
    pub fn unavailable() -> Self {
        Self {
            status: Availability::Unavailable,
            saved_at: None,
            description: String::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EditionSlots {
    pub edition: Edition,
    pub autosave: Slot,
    pub manual: Slot,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Catalogue {
    pub editions: Vec<EditionSlots>,
    pub active: Option<Edition>,
}
