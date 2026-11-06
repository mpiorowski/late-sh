//! The rung: one daily board a day, the same for everyone, keyed to the
//! calendar and never to the person, so its solvers are the day's crowd.
//! Winning it on its day earns a star in `daily_rung_stars`; there is no
//! catching up on yesterday's.

use anyhow::Result;
use chrono::{Datelike, NaiveDate, Weekday};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use super::{
    chips::Difficulty,
    leaderboard::{DailyPuzzle, SINGLE_TIER_KEY, Tiers},
};

/// A day's rung: a daily and the one board of it that counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    pub puzzle: DailyPuzzle,
    pub difficulty_key: &'static str,
    pub tier: Difficulty,
}

impl Rung {
    /// The rung as copy names it: `easy Sudoku`, `draw-1 Solitaire`, and a
    /// one-board daily by its title alone, `Le Word`.
    pub fn label(self) -> String {
        match self.puzzle.tiers() {
            Tiers::Single(_) => self.puzzle.title().to_string(),
            Tiers::Keyed(_) => format!("{} {}", self.difficulty_key, self.puzzle.title()),
        }
    }
}

/// The rotation: one game a day in this order, eleven games, so no game
/// comes round twice inside a week. Each game's board is the one a regular
/// finishes in five to fifteen minutes, picked by the clock and not the
/// label (medium Netwalk is three minutes, medium Sudoku can be thirty).
/// Easy Netwalk, the board the first-visit tour lends, is never a rung,
/// so day one's win is never a star and tomorrow can be the first.
const ROTATION: [(DailyPuzzle, &str); 11] = [
    (DailyPuzzle::Sudoku, "easy"),
    (DailyPuzzle::Netwalk, "hard"),
    (DailyPuzzle::LeWord, SINGLE_TIER_KEY),
    (DailyPuzzle::Minesweeper, "medium"),
    (DailyPuzzle::Nonogram, "medium"),
    (DailyPuzzle::ChessPuzzle, "medium"),
    (DailyPuzzle::Solitaire, "draw-1"),
    (DailyPuzzle::WordWheel, "medium"),
    (DailyPuzzle::SlidingPuzzle, "medium"),
    (DailyPuzzle::Quartets, SINGLE_TIER_KEY),
    (DailyPuzzle::RubiksCube, SINGLE_TIER_KEY),
];

/// The Monday the rotation starts counting from. Never move it: every day's
/// rung, and so every star, is derived from it.
const ROTATION_EPOCH: NaiveDate = match NaiveDate::from_ymd_opt(2026, 11, 9) {
    Some(day) => day,
    None => unreachable!(),
};

/// The one night a week the rung is the day's game at its hardest board.
const HARD_NIGHT: Weekday = Weekday::Sat;

/// The rung for a UTC day.
pub fn rung_for(day: NaiveDate) -> Rung {
    let index = (day - ROTATION_EPOCH)
        .num_days()
        .rem_euclid(ROTATION.len() as i64) as usize;
    let (puzzle, rotation_key) = ROTATION[index];
    let boards = puzzle.tiers().list();
    let (difficulty_key, tier) = if day.weekday() == HARD_NIGHT {
        *boards.last().expect("a daily has at least one board")
    } else {
        boards
            .into_iter()
            .find(|(key, _)| *key == rotation_key)
            .expect("the rotation names a board its daily has")
    };
    Rung {
        puzzle,
        difficulty_key,
        tier,
    }
}

/// A daily board won on `won_on`: when it is that day's rung, the star
/// lands and the user's star count comes back. `None` when the board is
/// not the rung, or the star is already there (a day has one). A win's own
/// date is what counts, so yesterday's rung won today is nothing.
pub async fn record_rung_win(
    client: &impl GenericClient,
    user_id: Uuid,
    won_on: NaiveDate,
    puzzle: DailyPuzzle,
    difficulty_key: &str,
) -> Result<Option<i64>> {
    let rung = rung_for(won_on);
    if rung.puzzle != puzzle || rung.difficulty_key != difficulty_key {
        return Ok(None);
    }
    let inserted = client
        .execute(
            "INSERT INTO daily_rung_stars (user_id, day, puzzle, difficulty_key)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (user_id, day) DO NOTHING",
            &[&user_id, &won_on, &puzzle.key(), &difficulty_key],
        )
        .await?;
    if inserted == 0 {
        return Ok(None);
    }
    let row = client
        .query_one(
            "SELECT count(*) AS stars FROM daily_rung_stars WHERE user_id = $1",
            &[&user_id],
        )
        .await?;
    Ok(Some(row.get("stars")))
}

#[cfg(test)]
#[path = "daily_rung_test.rs"]
mod daily_rung_test;
