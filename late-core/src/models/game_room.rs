//! The house-table game roster. The rooms-era `GameRoom` model (user-created
//! `game_rooms` rows) is gone; what survives is the `GameKind` tag stored in
//! `chat_rooms.game_kind` for the permanent house-table chat rooms.

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameKind {
    Asterion,
    Blackjack,
    Poker,
    Tron,
}

impl GameKind {
    pub const ALL: [Self; 4] = [Self::Asterion, Self::Blackjack, Self::Poker, Self::Tron];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Asterion => "asterion",
            Self::Blackjack => "blackjack",
            Self::Poker => "poker",
            Self::Tron => "tron",
        }
    }
}

impl std::fmt::Display for GameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl TryFrom<&str> for GameKind {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "asterion" => Ok(Self::Asterion),
            "blackjack" => Ok(Self::Blackjack),
            "poker" => Ok(Self::Poker),
            "tron" => Ok(Self::Tron),
            _ => Err(anyhow::anyhow!("unknown game kind: {}", value)),
        }
    }
}
