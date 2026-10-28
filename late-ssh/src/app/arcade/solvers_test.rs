use std::collections::HashMap;

use chrono::NaiveDate;
use late_core::models::leaderboard::{DailyCompletionStatus, DailyPuzzle};
use uuid::Uuid;

use super::{Roster, TierSolvers, TodaySolvers};

const TIERS: &[(&str, i64)] = &[("easy", 100), ("medium", 250), ("hard", 500)];

fn status(marks: &[(DailyPuzzle, &str)]) -> DailyCompletionStatus {
    let mut status = DailyCompletionStatus::default();
    for (game, difficulty) in marks {
        status.mark_completed(*game, (*difficulty).to_string());
    }
    status
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date")
}

/// The whole roster for one day: hardest tier first, every solver named
/// once per tier they finished, the session's own fresh win counted before
/// the snapshot has it, and a tier nobody solved kept as an empty row.
#[test]
fn roster_lists_hardest_first_with_the_session_win_folded_in() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);
    let carol = Uuid::from_u128(4);
    let statuses = HashMap::from([
        (
            alice,
            status(&[(DailyPuzzle::Sudoku, "hard"), (DailyPuzzle::Sudoku, "easy")]),
        ),
        (bob, status(&[(DailyPuzzle::Sudoku, "hard")])),
        // Carol solved a different daily: not on this roster at all.
        (carol, status(&[(DailyPuzzle::Minesweeper, "hard")])),
    ]);
    let usernames = HashMap::from([
        (alice, "alice".to_string()),
        (bob, "bob".to_string()),
        (carol, "carol".to_string()),
    ]);
    let own = status(&[(DailyPuzzle::Sudoku, "easy")]);
    let today = TodaySolvers {
        statuses: &statuses,
        usernames: &usernames,
        own_user_id: me,
        own_username: "me",
        own_session: Some(&own),
    };

    let roster = today.roster(date(), DailyPuzzle::Sudoku, TIERS);

    assert_eq!(
        roster.total, 3,
        "alice, bob and me; carol solved another game"
    );
    assert_eq!(roster.tiers.len(), 3);
    assert_eq!(roster.tiers[0].difficulty_key, "hard");
    assert_eq!(roster.tiers[0].chips, 500);
    let mut hard = roster.tiers[0].names.clone();
    hard.sort();
    assert_eq!(hard, vec!["alice", "bob"]);
    assert_eq!(
        roster.tiers[1],
        TierSolvers {
            difficulty_key: "medium",
            chips: 250,
            names: Vec::new(),
        }
    );
    let mut easy = roster.tiers[2].names.clone();
    easy.sort();
    assert_eq!(easy, vec!["alice", "me"]);
    assert_eq!(today.solved_count(DailyPuzzle::Minesweeper), 1);
    assert_eq!(today.solved_count(DailyPuzzle::LeWord), 0);
}

/// The order is a function of the day, not of map iteration or the clock:
/// two viewers, and two renders, see the same shuffle, and another day
/// reshuffles.
#[test]
fn tier_order_is_fixed_per_day_and_identical_for_every_viewer() {
    let ids: Vec<Uuid> = (10..30).map(Uuid::from_u128).collect();
    let statuses: HashMap<Uuid, DailyCompletionStatus> = ids
        .iter()
        .map(|id| (*id, status(&[(DailyPuzzle::LeWord, "daily")])))
        .collect();
    let usernames: HashMap<Uuid, String> = ids
        .iter()
        .map(|id| (*id, format!("player{}", id.as_u128())))
        .collect();
    // Two viewers whose own ids are not on the roster.
    let viewer_a = TodaySolvers {
        statuses: &statuses,
        usernames: &usernames,
        own_user_id: Uuid::from_u128(1),
        own_username: "a",
        own_session: None,
    };
    let viewer_b = TodaySolvers {
        statuses: &statuses,
        usernames: &usernames,
        own_user_id: Uuid::from_u128(2),
        own_username: "b",
        own_session: None,
    };
    let tiers = &[("daily", 200)];

    let first = viewer_a.roster(date(), DailyPuzzle::LeWord, tiers);
    let again = viewer_a.roster(date(), DailyPuzzle::LeWord, tiers);
    let other_viewer = viewer_b.roster(date(), DailyPuzzle::LeWord, tiers);
    let tomorrow = viewer_a.roster(
        date().succ_opt().expect("tomorrow"),
        DailyPuzzle::LeWord,
        tiers,
    );

    assert_eq!(first, again);
    assert_eq!(first, other_viewer);
    assert_eq!(first.total, 20);
    assert_eq!(first.tiers[0].names.len(), 20);
    let mut sorted = first.tiers[0].names.clone();
    sorted.sort();
    assert_ne!(first.tiers[0].names, sorted, "a shuffle, not a ranking");
    assert_ne!(first.tiers[0].names, tomorrow.tiers[0].names);
}

/// A session win already in the snapshot is one person, not two, and a
/// solver the snapshot knows but has no name for is left out rather than
/// shown blank.
#[test]
fn a_solver_is_counted_once_and_a_nameless_row_is_skipped() {
    let me = Uuid::from_u128(1);
    let ghost = Uuid::from_u128(9);
    let statuses = HashMap::from([
        (me, status(&[(DailyPuzzle::Solitaire, "draw-3")])),
        (ghost, status(&[(DailyPuzzle::Solitaire, "draw-1")])),
    ]);
    let usernames = HashMap::new();
    let own = status(&[(DailyPuzzle::Solitaire, "draw-3")]);
    let today = TodaySolvers {
        statuses: &statuses,
        usernames: &usernames,
        own_user_id: me,
        own_username: "me",
        own_session: Some(&own),
    };

    let roster = today.roster(
        date(),
        DailyPuzzle::Solitaire,
        &[("draw-1", 250), ("draw-3", 500)],
    );

    assert_eq!(
        roster,
        Roster {
            total: 2,
            tiers: vec![
                TierSolvers {
                    difficulty_key: "draw-3",
                    chips: 500,
                    names: vec!["me".to_string()],
                },
                TierSolvers {
                    difficulty_key: "draw-1",
                    chips: 250,
                    names: Vec::new(),
                },
            ],
        }
    );
}
