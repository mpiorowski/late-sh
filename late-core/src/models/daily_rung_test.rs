use std::collections::HashSet;

use chrono::Duration;

use super::*;
use crate::test_utils::{create_test_user, test_db};

#[test]
fn the_rotation_names_every_daily_once_with_a_board_it_has() {
    let mut seen = HashSet::new();
    for (puzzle, key) in ROTATION {
        assert!(seen.insert(puzzle), "{puzzle:?} comes round twice");
        assert!(
            puzzle.tiers().list().iter().any(|(board, _)| *board == key),
            "{puzzle:?} has no {key} board"
        );
    }
    for puzzle in DailyPuzzle::ALL {
        assert!(seen.contains(puzzle), "{puzzle:?} is never the rung");
    }
}

/// Eleven weeks of rungs: no game inside any seven days twice, Saturday
/// the day's game at its hardest, every other day the rotation's board,
/// and easy Netwalk, the tour's board, never.
#[test]
fn a_week_never_repeats_a_game_and_saturday_is_the_hard_night() {
    let days: Vec<NaiveDate> = (0..77)
        .map(|offset| ROTATION_EPOCH + Duration::days(offset))
        .collect();
    let rungs: Vec<Rung> = days.iter().map(|day| rung_for(*day)).collect();
    for window in rungs.windows(7) {
        let games: HashSet<DailyPuzzle> = window.iter().map(|rung| rung.puzzle).collect();
        assert_eq!(games.len(), 7, "a game repeats inside a week: {window:?}");
    }
    for (day, rung) in days.iter().zip(&rungs) {
        let boards = rung.puzzle.tiers().list();
        if day.weekday() == Weekday::Sat {
            assert_eq!((rung.difficulty_key, rung.tier), boards[boards.len() - 1]);
        } else {
            let (_, rotation_key) = ROTATION
                .iter()
                .find(|(puzzle, _)| *puzzle == rung.puzzle)
                .unwrap();
            assert_eq!(rung.difficulty_key, *rotation_key, "{day}");
        }
        // A game with a choice of boards never gives its easy one; a game
        // with one board gives that one, whatever its tier is called.
        match rung.puzzle.tiers() {
            Tiers::Keyed(_) => assert!(
                rung.tier != Difficulty::Easy,
                "a star is never a game's easy board: {day}"
            ),
            Tiers::Single(_) => {}
        }
    }
    assert_eq!(rung_for(ROTATION_EPOCH).label(), "medium Sudoku");
    assert_eq!(
        rung_for(ROTATION_EPOCH + Duration::days(2)).label(),
        "Le Word"
    );
    assert_eq!(
        rung_for(ROTATION_EPOCH + Duration::days(5)).label(),
        "hard Chess Puzzle",
        "the first Saturday is Chess Puzzle at its hardest"
    );
    assert_eq!(
        rung_for(ROTATION_EPOCH + Duration::days(6)).label(),
        "draw-1 Solitaire",
        "Sunday is back on the rotation's board"
    );
}

/// The star is the rung's board on its own day, once; another board,
/// yesterday's rung's among them, is nothing.
#[tokio::test]
async fn a_star_is_the_rungs_board_on_its_day_once() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "rung-climber").await;
    let client = test_db.db.get().await.expect("db client");
    let day = ROTATION_EPOCH + Duration::days(3);
    let rung = rung_for(day);
    let yesterday = rung_for(day - Duration::days(1));

    let star = |won_on: NaiveDate, rung: Rung| {
        record_rung_win(&**client, user.id, won_on, rung.puzzle, rung.difficulty_key)
    };
    assert_eq!(
        record_rung_win(&**client, user.id, day, rung.puzzle, "hard")
            .await
            .unwrap(),
        None,
        "another board of the rung's game is not the rung"
    );
    assert_eq!(star(day, yesterday).await.unwrap(), None);
    assert_eq!(star(day, rung).await.unwrap(), Some(1));
    assert_eq!(star(day, rung).await.unwrap(), None, "one star a day");
    let next = day + Duration::days(1);
    assert_eq!(star(next, rung_for(next)).await.unwrap(), Some(2));
    assert_eq!(count_stars(&**client, user.id).await.unwrap(), 2);
    assert_eq!(
        latest_star_day(&**client, user.id).await.unwrap(),
        Some(next)
    );

    // The day's star beside the name: today's only.
    let stranger = create_test_user(&test_db.db, "rung-stranger").await;
    let ids = [user.id, stranger.id];
    assert_eq!(
        list_starred_on(&**client, &ids, next).await.unwrap(),
        HashSet::from([user.id])
    );
    assert_eq!(
        list_starred_on(&**client, &ids, next + Duration::days(1))
            .await
            .unwrap(),
        HashSet::new()
    );
    let everyone = starred_on(&**client, next).await.unwrap();
    assert!(everyone.contains(&user.id) && !everyone.contains(&stranger.id));
    assert_eq!(latest_star_day(&**client, stranger.id).await.unwrap(), None);
}
