use super::svc::GameKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderKind {
    Poker,
    Chess,
    Battleship,
    Tron,
}

impl PlaceholderKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Poker => "Poker",
            Self::Chess => "Chess",
            Self::Battleship => "Battleship",
            Self::Tron => "Tron",
        }
    }

    pub fn meta(self) -> GameMeta {
        match self {
            Self::Poker => GameMeta {
                short: "PK",
                seats: 6,
                pace: "~10m / hand",
            },
            Self::Chess => GameMeta {
                short: "CH",
                seats: 2,
                pace: "async, days",
            },
            Self::Battleship => GameMeta {
                short: "BS",
                seats: 2,
                pace: "async",
            },
            Self::Tron => GameMeta {
                short: "TR",
                seats: 4,
                pace: "real-time, ~2m",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GameMeta {
    pub short: &'static str,
    pub seats: u8,
    pub pace: &'static str,
}

pub fn meta_for_real(kind: GameKind) -> GameMeta {
    match kind {
        GameKind::Blackjack => GameMeta {
            short: "BJ",
            seats: 4,
            pace: "5m action timer",
        },
    }
}

pub const PLACEHOLDERS: &[PlaceholderKind] = &[
    PlaceholderKind::Poker,
    PlaceholderKind::Chess,
    PlaceholderKind::Battleship,
    PlaceholderKind::Tron,
];
