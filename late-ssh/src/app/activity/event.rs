use std::time::Instant;

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::metrics;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityCategory {
    Session,
    Game,
    Bonsai,
    Quest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityKind {
    UserJoined,
    GameWon {
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
    },
    GamePlayed {
        game: ActivityGame,
        detail: Option<String>,
    },
    GameScored {
        game: ActivityGame,
        score: i32,
        level: Option<i32>,
    },
    BonsaiWatered,
    BonsaiLost {
        survived_days: i32,
    },
}

impl ActivityKind {
    pub fn category(&self) -> ActivityCategory {
        match self {
            Self::UserJoined => ActivityCategory::Session,
            Self::GameWon { .. } => ActivityCategory::Game,
            Self::GamePlayed { .. } | Self::GameScored { .. } => ActivityCategory::Quest,
            Self::BonsaiWatered | Self::BonsaiLost { .. } => ActivityCategory::Bonsai,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityGame {
    Asterion,
    Blackjack,
    Chess,
    LeWord,
    Minesweeper,
    Mud,
    Nonogram,
    Poker,
    RubiksCube,
    Sshattrick,
    Solitaire,
    Sudoku,
    TicTacToe,
    Lateris,
    TwentyFortyEight,
    Tron,
    Snake,
}

impl ActivityGame {
    pub fn key(self) -> &'static str {
        match self {
            Self::Asterion => "asterion",
            Self::Blackjack => "blackjack",
            Self::Chess => "chess",
            Self::LeWord => "le_word",
            Self::Minesweeper => "minesweeper",
            Self::Mud => "mud",
            Self::Nonogram => "nonogram",
            Self::Poker => "poker",
            Self::RubiksCube => "rubiks_cube",
            Self::Sshattrick => "sshattrick",
            Self::Solitaire => "solitaire",
            Self::Sudoku => "sudoku",
            Self::TicTacToe => "tictactoe",
            Self::Lateris => "tetris",
            Self::TwentyFortyEight => "2048",
            Self::Tron => "tron",
            Self::Snake => "snake",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Asterion => "Asterion",
            Self::Blackjack => "Blackjack",
            Self::Chess => "Chess",
            Self::LeWord => "Le Word",
            Self::Minesweeper => "Minesweeper",
            Self::Mud => "Lateania",
            Self::Nonogram => "Nonogram",
            Self::Poker => "Poker",
            Self::RubiksCube => "Rubik's Cube",
            Self::Sshattrick => "ssHattrick",
            Self::Solitaire => "Solitaire",
            Self::Sudoku => "Sudoku",
            Self::TicTacToe => "Tic-Tac-Toe",
            Self::Lateris => "Lateris",
            Self::TwentyFortyEight => "2048",
            Self::Tron => "Tron",
            Self::Snake => "Snake",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub username: String,
    pub action: String,
    pub kind: ActivityKind,
    pub at: Instant,
    pub occurred_at: DateTime<Utc>,
}

impl ActivityEvent {
    pub fn occurred_on_utc_date(date: NaiveDate) -> DateTime<Utc> {
        date.and_hms_opt(12, 0, 0)
            .expect("noon is a valid time")
            .and_utc()
    }

    pub fn joined(user_id: Uuid, username: impl Into<String>) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::UserJoined,
            "joined".to_string(),
        )
    }

    pub fn game_won(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
    ) -> Self {
        Self::game_won_at(user_id, username, game, detail, score, Utc::now())
    }

    pub fn game_won_at(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        metrics::record_game_win(game);
        let base_action = match game {
            ActivityGame::Asterion => "escaped the Asterion maze",
            ActivityGame::Blackjack => "won Blackjack hand",
            ActivityGame::Chess => "won Chess game",
            ActivityGame::LeWord => "solved Le Word",
            ActivityGame::Minesweeper => "cleared Minesweeper",
            ActivityGame::Mud => "triumphed in Lateania",
            ActivityGame::Nonogram => "solved Nonogram",
            ActivityGame::Poker => "won Poker hand",
            ActivityGame::RubiksCube => "solved Rubik's Cube",
            ActivityGame::Sshattrick => "won ssHattrick match",
            ActivityGame::Solitaire => "won Solitaire",
            ActivityGame::Sudoku => "solved Sudoku",
            ActivityGame::TicTacToe => "won Tic-Tac-Toe",
            ActivityGame::Lateris => "won Lateris",
            ActivityGame::TwentyFortyEight => "won 2048",
            ActivityGame::Tron => "won Tron round",
            ActivityGame::Snake => "won Snake",
        };
        let action = match detail.as_deref() {
            Some(detail) if !detail.is_empty() => format!("{base_action} ({detail})"),
            _ => base_action.to_string(),
        };
        Self::new_at(
            Some(user_id),
            username,
            ActivityKind::GameWon {
                game,
                detail,
                score,
            },
            action,
            occurred_at,
        )
    }

    pub fn game_played(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        detail: Option<String>,
    ) -> Self {
        let base_action = format!("played {} round", game.label());
        let action = match detail.as_deref() {
            Some(detail) if !detail.is_empty() => format!("{base_action} ({detail})"),
            _ => base_action,
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::GamePlayed { game, detail },
            action,
        )
    }

    pub fn game_scored(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        score: i32,
        level: Option<i32>,
    ) -> Self {
        let action = match level {
            Some(level) => format!("scored {score} in {} (level {level})", game.label()),
            None => format!("scored {score} in {}", game.label()),
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::GameScored { game, score, level },
            action,
        )
    }

    pub fn bonsai_watered(user_id: Uuid, username: impl Into<String>) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BonsaiWatered,
            "watered their bonsai".to_string(),
        )
    }

    pub fn bonsai_lost(user_id: Uuid, username: impl Into<String>, survived_days: i32) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BonsaiLost { survived_days },
            format!("lost their bonsai ({survived_days}d)"),
        )
    }

    fn new(
        user_id: Option<Uuid>,
        username: impl Into<String>,
        kind: ActivityKind,
        action: String,
    ) -> Self {
        Self::new_at(user_id, username, kind, action, Utc::now())
    }

    fn new_at(
        user_id: Option<Uuid>,
        username: impl Into<String>,
        kind: ActivityKind,
        action: String,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            user_id,
            username: username.into(),
            action,
            kind,
            at: Instant::now(),
            occurred_at,
        }
    }

    pub fn category(&self) -> ActivityCategory {
        self.kind.category()
    }
}
