//! The rung: one daily board a day, the same for everyone, keyed to the
//! calendar and never to the person, so its solvers are the day's crowd.
//! Winning it on its day earns a star in `daily_rung_stars`; there is no
//! catching up on yesterday's.

use std::collections::HashSet;

use anyhow::Result;
use chrono::{Datelike, NaiveDate, Weekday};
use tokio_postgres::GenericClient;
use uuid::Uuid;

use super::{
    chips::Difficulty,
    leaderboard::{DailyPuzzle, SINGLE_TIER_KEY, Tiers},
};

/// The star beside a name in chat on the day its rung landed, and on the
/// week's badges: the tour's own glyph, one cell.
pub const STAR_GLYPH: char = '✦';

/// A day's rung: a daily and the one board of it that counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rung {
    pub puzzle: DailyPuzzle,
    pub difficulty_key: &'static str,
    pub tier: Difficulty,
}

impl Rung {
    /// The rung as copy names it: `medium Sudoku`, `draw-1 Solitaire`, and a
    /// one-board daily by its title alone, `Le Word`.
    pub fn label(self) -> String {
        match self.puzzle.tiers() {
            Tiers::Single(_) => self.puzzle.title().to_string(),
            Tiers::Keyed(_) => format!("{} {}", self.difficulty_key, self.puzzle.title()),
        }
    }
}

/// The rotation: one game a day in this order, eleven games, so no game
/// comes round twice inside a week. A game with a choice of boards gives
/// at least its medium one: a star is for a real board, never a game's
/// easy one, and within that the pick is by the clock (hard Netwalk is
/// minutes, hard Sudoku an evening); a game with one board (Le Word,
/// Quartets, the cube) gives that one. Easy Netwalk, the board the
/// first-visit tour lends, is never a rung, so day one's win is never a
/// star and tomorrow can be the first.
const ROTATION: [(DailyPuzzle, &str); 11] = [
    (DailyPuzzle::Sudoku, "medium"),
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

/// How many boards the rotation holds: the days before a game comes round
/// again.
pub const ROTATION_LEN: usize = ROTATION.len();

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

/// A daily board won, `won_on` its day: when it is that day's rung, the
/// star lands and the user's star count comes back. `None` when the board
/// is not the rung, or the star is already there (a day has one). The
/// board's day is what counts, the date the hub stamps on the win, so a
/// board finished after midnight still lands on its own day's rung, and a
/// board that was not its day's rung is nothing whenever it is won.
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
    Ok(Some(count_stars(client, user_id).await?))
}

/// Every star a user has: the number on the profile and the day card.
pub async fn count_stars(client: &impl GenericClient, user_id: Uuid) -> Result<i64> {
    let row = client
        .query_one(
            "SELECT count(*) AS stars FROM daily_rung_stars WHERE user_id = $1",
            &[&user_id],
        )
        .await?;
    Ok(row.get("stars"))
}

/// The day of a user's newest star, `None` without one: today's star is
/// this on today, for the week's badge and the return push.
pub async fn latest_star_day(
    client: &impl GenericClient,
    user_id: Uuid,
) -> Result<Option<NaiveDate>> {
    let row = client
        .query_one(
            "SELECT max(day) AS day FROM daily_rung_stars WHERE user_id = $1",
            &[&user_id],
        )
        .await?;
    Ok(row.get("day"))
}

/// Everyone with the star of `day`, for the day's count on the week.
pub async fn starred_on(client: &impl GenericClient, day: NaiveDate) -> Result<HashSet<Uuid>> {
    let rows = client
        .query(
            "SELECT user_id FROM daily_rung_stars WHERE day = $1",
            &[&day],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get("user_id")).collect())
}

/// Which of `user_ids` have the star of `day`, for the marks beside their
/// names on that day.
pub async fn list_starred_on(
    client: &impl GenericClient,
    user_ids: &[Uuid],
    day: NaiveDate,
) -> Result<HashSet<Uuid>> {
    if user_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let rows = client
        .query(
            "SELECT user_id FROM daily_rung_stars WHERE user_id = ANY($1) AND day = $2",
            &[&user_ids, &day],
        )
        .await?;
    Ok(rows.into_iter().map(|row| row.get("user_id")).collect())
}

#[cfg(test)]
#[path = "daily_rung_test.rs"]
mod daily_rung_test;
