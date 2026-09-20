//! The daily-games roster. One enum owns every per-game fact; there is no
//! trait object or registry behind it. Adding a game is: add a variant here,
//! let the compiler walk you through the exhaustive matches (name, prize,
//! reward key, initial state, move handling, board surface), and seed its
//! win-payout reward template in a migration.

use late_core::models::{
    chips::ChipMove,
    daily_match::DailyMatch,
    reward::{
        DAILY_BACKGAMMON_WIN_REWARD_KEY, DAILY_BATTLESHIP_WIN_REWARD_KEY,
        DAILY_BRISCOLA_WIN_REWARD_KEY, DAILY_CHECKERS_WIN_REWARD_KEY, DAILY_CHESS_WIN_REWARD_KEY,
        DAILY_CHESS960_WIN_REWARD_KEY, DAILY_CONNECT4_WIN_REWARD_KEY,
        DAILY_EIGHTBALL_WIN_REWARD_KEY, DAILY_NINEBALL_WIN_REWARD_KEY,
        DAILY_REVERSI_WIN_REWARD_KEY, DAILY_SNOOKER_WIN_REWARD_KEY,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DailyGame {
    Chess,
    Chess960,
    Battleship,
    ConnectFour,
    Reversi,
    Checkers,
    Backgammon,
    Briscola,
    EightBall,
    NineBall,
    Snooker,
}

impl DailyGame {
    /// Roster order: pickers, help copy, and usage strings follow it.
    pub const ALL: [Self; 11] = [
        Self::Chess,
        Self::Chess960,
        Self::Battleship,
        Self::ConnectFour,
        Self::Reversi,
        Self::Checkers,
        Self::Backgammon,
        Self::Briscola,
        Self::EightBall,
        Self::NineBall,
        Self::Snooker,
    ];

    /// The persisted `daily_matches.game_kind` value.
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Chess => DailyMatch::GAME_KIND_CHESS,
            Self::Chess960 => DailyMatch::GAME_KIND_CHESS960,
            Self::Battleship => DailyMatch::GAME_KIND_BATTLESHIP,
            Self::ConnectFour => DailyMatch::GAME_KIND_CONNECTFOUR,
            Self::Reversi => DailyMatch::GAME_KIND_REVERSI,
            Self::Checkers => DailyMatch::GAME_KIND_CHECKERS,
            Self::Backgammon => DailyMatch::GAME_KIND_BACKGAMMON,
            Self::Briscola => DailyMatch::GAME_KIND_BRISCOLA,
            Self::EightBall => DailyMatch::GAME_KIND_EIGHTBALL,
            Self::NineBall => DailyMatch::GAME_KIND_NINEBALL,
            Self::Snooker => DailyMatch::GAME_KIND_SNOOKER,
        }
    }

    /// Lowercase display name; also the token used in usage banners.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chess => "chess",
            Self::Chess960 => "chess960",
            Self::Battleship => "battleship",
            Self::ConnectFour => "connect4",
            Self::Reversi => "reversi",
            Self::Checkers => "checkers",
            Self::Backgammon => "backgammon",
            Self::Briscola => "briscola",
            Self::EightBall => "8ball",
            Self::NineBall => "9ball",
            Self::Snooker => "snooker",
        }
    }

    /// The human-readable game name for prose surfaces (e.g. the #lounge result
    /// line "won a game of Connect Four"). Distinct from `label`, which is the
    /// lowercase command token.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Chess => "Chess",
            Self::Chess960 => "Chess960",
            Self::Battleship => "Battleship",
            Self::ConnectFour => "Connect Four",
            Self::Reversi => "Reversi",
            Self::Checkers => "Checkers",
            Self::Backgammon => "Backgammon",
            Self::Briscola => "Briscola",
            Self::EightBall => "Eight-Ball",
            Self::NineBall => "Nine-Ball",
            Self::Snooker => "Snooker",
        }
    }

    /// Chips the winner takes. This is the displayed number; the credited
    /// amount comes from the game's seeded reward template — keep in sync.
    pub const fn win_payout(self) -> i64 {
        match self {
            Self::Chess => 500,
            Self::Chess960 => 500,
            Self::Battleship => 300,
            Self::ConnectFour => 400,
            Self::Reversi => 400,
            Self::Checkers => 400,
            Self::Backgammon => 400,
            Self::Briscola => 400,
            Self::EightBall => 400,
            Self::NineBall => 400,
            // A frame is the longest match on the roster by a distance, and the
            // payout says so.
            Self::Snooker => 700,
        }
    }

    pub const fn reward_key(self) -> &'static str {
        match self {
            Self::Chess => DAILY_CHESS_WIN_REWARD_KEY,
            Self::Chess960 => DAILY_CHESS960_WIN_REWARD_KEY,
            Self::Battleship => DAILY_BATTLESHIP_WIN_REWARD_KEY,
            Self::ConnectFour => DAILY_CONNECT4_WIN_REWARD_KEY,
            Self::Reversi => DAILY_REVERSI_WIN_REWARD_KEY,
            Self::Checkers => DAILY_CHECKERS_WIN_REWARD_KEY,
            Self::Backgammon => DAILY_BACKGAMMON_WIN_REWARD_KEY,
            Self::Briscola => DAILY_BRISCOLA_WIN_REWARD_KEY,
            Self::EightBall => DAILY_EIGHTBALL_WIN_REWARD_KEY,
            Self::NineBall => DAILY_NINEBALL_WIN_REWARD_KEY,
            Self::Snooker => DAILY_SNOOKER_WIN_REWARD_KEY,
        }
    }

    pub const fn chip_move(self) -> ChipMove {
        match self {
            Self::Chess => ChipMove::DailyChessWin,
            Self::Chess960 => ChipMove::DailyChess960Win,
            Self::Battleship => ChipMove::DailyBattleshipWin,
            Self::ConnectFour => ChipMove::DailyConnectFourWin,
            Self::Reversi => ChipMove::DailyReversiWin,
            Self::Checkers => ChipMove::DailyCheckersWin,
            Self::Backgammon => ChipMove::DailyBackgammonWin,
            Self::Briscola => ChipMove::DailyBriscolaWin,
            Self::EightBall => ChipMove::DailyEightBallWin,
            Self::NineBall => ChipMove::DailyNineBallWin,
            Self::Snooker => ChipMove::DailySnookerWin,
        }
    }

    /// One-line rules blurb for the board screen's info rail.
    pub const fn tagline(self) -> &'static str {
        match self {
            Self::Chess => "one move per day",
            Self::Chess960 => "shuffled back rank · castle onto your rook",
            Self::Battleship => "one salvo per day · a hit fires again",
            Self::ConnectFour => "one drop per day · four in a row wins",
            Self::Reversi => "one move per day · most discs wins",
            Self::Checkers => "one move per day · capture or block to win",
            Self::Backgammon => "one roll per day · bear off all fifteen",
            Self::Briscola => "one card per day · most points wins",
            Self::EightBall => "one shot per day · potting shoots again",
            Self::NineBall => "one shot per day · lowest ball first",
            Self::Snooker => "one shot per day · reds, colours, and a scoreboard",
        }
    }

    /// Whether this game is played on a pool table.
    ///
    /// One answer, asked everywhere the roster needs to know — the chat floor,
    /// the move-count wording, the claim and the shot channel. Spelling the
    /// variants out at each site is how snooker shipped with an input layer
    /// that was switched off: one of the lists had not heard of it, and that
    /// list happened to be the one gating every key and every click.
    pub const fn is_pool(self) -> bool {
        matches!(self, Self::EightBall | Self::NineBall | Self::Snooker)
    }

    pub fn from_kind(kind: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|game| game.kind() == kind)
    }

    /// Parse a user-typed game name (e.g. `battleship`).
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|game| game.label().eq_ignore_ascii_case(label))
    }

    /// Every label joined with `|` — for usage banners and help copy.
    pub fn usage_labels() -> String {
        Self::ALL
            .into_iter()
            .map(Self::label)
            .collect::<Vec<_>>()
            .join("|")
    }
}
