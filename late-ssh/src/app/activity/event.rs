use std::time::Instant;

use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityCategory {
    Session,
    Vote,
    Game,
    Bonsai,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityKind {
    UserJoined,
    VoteCast {
        genre: String,
    },
    GameWon {
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
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
            Self::VoteCast { .. } => ActivityCategory::Vote,
            Self::GameWon { .. } => ActivityCategory::Game,
            Self::BonsaiWatered | Self::BonsaiLost { .. } => ActivityCategory::Bonsai,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityGame {
    Blackjack,
    Minesweeper,
    Nonogram,
    Poker,
    Solitaire,
    Sudoku,
    TicTacToe,
}

#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub user_id: Option<Uuid>,
    pub username: String,
    pub action: String,
    pub kind: ActivityKind,
    pub at: Instant,
}

impl ActivityEvent {
    pub fn joined(user_id: Uuid, username: impl Into<String>) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::UserJoined,
            "joined".to_string(),
        )
    }

    pub fn vote_cast(user_id: Uuid, username: impl Into<String>, genre: impl ToString) -> Self {
        let genre = genre.to_string();
        Self::new(
            Some(user_id),
            username,
            ActivityKind::VoteCast {
                genre: genre.clone(),
            },
            format!("voted {genre}"),
        )
    }

    pub fn game_won(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
    ) -> Self {
        let base_action = match game {
            ActivityGame::Blackjack => "won Blackjack hand",
            ActivityGame::Minesweeper => "cleared Minesweeper",
            ActivityGame::Nonogram => "solved Nonogram",
            ActivityGame::Poker => "won Poker hand",
            ActivityGame::Solitaire => "won Solitaire",
            ActivityGame::Sudoku => "solved Sudoku",
            ActivityGame::TicTacToe => "won Tic-Tac-Toe",
        };
        let action = match detail.as_deref() {
            Some(detail) if !detail.is_empty() => format!("{base_action} ({detail})"),
            _ => base_action.to_string(),
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::GameWon {
                game,
                detail,
                score,
            },
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
        Self {
            user_id,
            username: username.into(),
            action,
            kind,
            at: Instant::now(),
        }
    }

    pub fn category(&self) -> ActivityCategory {
        self.kind.category()
    }
}
