use serde::{Deserialize, Serialize};

/// Wire values are shared with late-ssh's Zork client. No filesystem paths are
/// accepted by this protocol, and exec request text is never shell-evaluated.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Edition {
    Zork1,
    Zork2,
    Zork3,
}

impl Edition {
    pub(crate) const ALL: [Self; 3] = [Self::Zork1, Self::Zork2, Self::Zork3];

    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Zork1 => "zork1",
            Self::Zork2 => "zork2",
            Self::Zork3 => "zork3",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|edition| edition.key() == value)
    }

    /// The edition's name in the watch roster's status.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Zork1 => "Zork I",
            Self::Zork2 => "Zork II",
            Self::Zork3 => "Zork III",
        }
    }
}

/// Arcade handle bounds (`late_core::models::arcade_handle`), duplicated
/// here because the host shares no library with late-ssh.
const PLAYNAME_MIN_LEN: usize = 3;
const PLAYNAME_MAX_LEN: usize = 20;

/// A player's arcade handle as it travels with `play` and a watch request:
/// a letter, then letters, digits, or underscores, within the handle bounds.
/// It names the game in the watch roster and nothing else.
pub(crate) fn parse_playname(value: &str) -> Option<String> {
    if !(PLAYNAME_MIN_LEN..=PLAYNAME_MAX_LEN).contains(&value.len()) {
        return None;
    }
    let mut bytes = value.bytes();
    let first = bytes.next()?;
    let valid =
        first.is_ascii_alphabetic() && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
    valid.then(|| value.to_string())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Auto,
    Manual,
    New,
}

impl Action {
    pub(crate) fn mode(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::New => "new",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Request {
    List,
    /// The edition, what to load, and the player's arcade handle, which
    /// lists the game in the watch roster.
    Play(Edition, Action, String),
}

pub(crate) fn parse_request(data: &[u8]) -> Option<Request> {
    if data.len() > 64 {
        return None;
    }
    let parts: Vec<_> = std::str::from_utf8(data)
        .ok()?
        .split_ascii_whitespace()
        .collect();
    match parts.as_slice() {
        ["list"] => Some(Request::List),
        ["play", edition, action, playname] => {
            let edition = Edition::parse(edition)?;
            let action = match *action {
                "auto" => Action::Auto,
                "manual" => Action::Manual,
                "new" => Action::New,
                _ => return None,
            };
            Some(Request::Play(edition, action, parse_playname(playname)?))
        }
        _ => None,
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Availability {
    Missing,
    Ready,
    Invalid,
    Unavailable,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Slot {
    pub(crate) status: Availability,
    #[serde(default)]
    pub(crate) saved_at: Option<u64>,
    #[serde(default)]
    pub(crate) description: String,
}

impl Slot {
    pub(crate) fn missing() -> Self {
        Self {
            status: Availability::Missing,
            saved_at: None,
            description: String::new(),
        }
    }
    pub(crate) fn unavailable() -> Self {
        Self {
            status: Availability::Unavailable,
            saved_at: None,
            description: String::new(),
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct EditionSlots {
    pub(crate) edition: Edition,
    pub(crate) autosave: Slot,
    pub(crate) manual: Slot,
}

#[derive(Debug, Serialize)]
pub(crate) struct Catalogue {
    pub(crate) editions: Vec<EditionSlots>,
    pub(crate) active: Option<Edition>,
}

#[cfg(test)]
#[path = "protocol_test.rs"]
mod protocol_test;
