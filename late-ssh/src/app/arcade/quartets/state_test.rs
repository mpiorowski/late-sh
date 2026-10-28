use std::collections::{HashMap, HashSet};

use chrono::{Duration, NaiveDate, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::quartets::Game,
};
use tokio::sync::broadcast;

use super::super::svc::Daily;
use super::*;
use crate::app::activity::event::ActivityEvent;

fn service() -> QuartetsService {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    QuartetsService::new(db, activity)
}

/// Shipped puzzle 1. Words 0-3 have keys (piano, lock, map, keyboard),
/// 4-7 have shells, 8-11 have needles, 12-15 bark.
fn daily(puzzle_date: NaiveDate) -> Daily {
    Daily {
        puzzle_date,
        puzzle: find_puzzle(1).expect("puzzle 1 in the pack"),
    }
}

fn pick(state: &mut State, words: &[u8]) {
    for word in words {
        let position = state
            .wall()
            .iter()
            .position(|on_wall| on_wall == word)
            .expect("word on the wall");
        state.click(position);
    }
}

fn try_words(state: &mut State, words: [u8; QUARTETS]) {
    pick(state, &words);
    state.submit();
}

fn saved(puzzle_date: NaiveDate, puzzle_id: i32, tries: &[i16]) -> Game {
    Game {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        user_id: Uuid::now_v7(),
        puzzle_date,
        puzzle_id,
        tries: tries.to_vec(),
    }
}

/// The pack validator: ids run 1, 2, 3... so `find_puzzle` finds each one;
/// every puzzle is sixteen distinct words of two to twelve lowercase
/// letters, and every link is one to 28 characters and does not name a
/// word of its own quartet.
#[test]
fn every_shipped_puzzle_is_sixteen_distinct_words_in_four_quartets() {
    let puzzles = pack();
    assert!(puzzles.len() >= 60);
    for (index, puzzle) in puzzles.iter().enumerate() {
        assert_eq!(puzzle.id, index as i32 + 1);
        assert!(std::ptr::eq(find_puzzle(puzzle.id).unwrap(), puzzle));
        let words: HashSet<&str> = (0..WORDS as u8).map(|word| puzzle.word(word)).collect();
        assert_eq!(words.len(), WORDS, "puzzle {} repeats a word", puzzle.id);
        for word in &words {
            assert!(
                (2..=12).contains(&word.len())
                    && word.bytes().all(|byte| byte.is_ascii_lowercase()),
                "puzzle {} word {word}",
                puzzle.id
            );
        }
        for quartet in &puzzle.quartets {
            let link = quartet.link.to_ascii_lowercase();
            assert!(
                (1..=28).contains(&quartet.link.len())
                    && quartet
                        .words
                        .iter()
                        .all(|word| !link.split([' ', '_']).any(|part| part == word)),
                "puzzle {} link {}",
                puzzle.id,
                quartet.link
            );
        }
    }
}

#[test]
fn a_try_splits_by_how_its_words_fall_across_the_quartets() {
    assert_eq!(
        [
            [0, 1, 2, 3],
            [0, 1, 2, 4],
            [0, 1, 4, 5],
            [0, 1, 4, 8],
            [0, 4, 8, 12],
        ]
        .map(Split::of),
        [
            Split::Quartet,
            Split::ThreeOne,
            Split::TwoTwo,
            Split::TwoOneOne,
            Split::Apart,
        ]
    );
}

/// A wall played through: fewer than four picks and a repeated try cost
/// nothing, a fifth pick is refused, a miss names its split and costs a
/// strike, a quartet locks in, and the third quartet brings in the last.
#[test]
fn a_wall_played_through_to_solved() {
    let today = Utc::now().date_naive();
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(today),
            saved: None,
        }),
    );
    let mut messages = Vec::new();

    pick(&mut state, &[0, 1, 2, 4]);
    state.click(state.wall().iter().position(|word| *word == 5).unwrap());
    messages.push(state.message().to_string());
    state.submit();
    messages.push(state.message().to_string());
    pick(&mut state, &[0, 1, 2, 4]);
    state.submit();
    messages.push(state.message().to_string());
    state.clear_picks();
    pick(&mut state, &[12, 13]);
    state.submit();
    messages.push(state.message().to_string());
    state.clear_picks();
    for words in [[0, 1, 2, 3], [4, 5, 8, 9], [4, 5, 6, 7], [8, 9, 10, 11]] {
        try_words(&mut state, words);
        messages.push(state.message().to_string());
    }

    assert_eq!(
        messages,
        [
            "Four are picked: Enter tries them, x clears.",
            "Three of those share a link, one does not. Strike 1 of 3.",
            "Those four were tried already. No strike.",
            "Pick four words, then Enter: 2 picked.",
            "Quartet: Have keys.",
            "Those split into two pairs. Strike 2 of 3.",
            "Quartet: Have shells.",
            "Quartet: Have needles. The last four: Bark. Solved!",
        ]
    );
    assert_eq!(
        (
            state.found(),
            state.wall(),
            state.tries(),
            state.strikes(),
            state.picked_count(),
            state.outcome(),
            state.has_unfinished_daily(),
        ),
        (
            &[0, 1, 2, 3][..],
            &[][..],
            &[
                [0, 1, 2, 4],
                [0, 1, 2, 3],
                [4, 5, 8, 9],
                [4, 5, 6, 7],
                [8, 9, 10, 11]
            ][..],
            2,
            0,
            Outcome::Solved,
            false,
        )
    );
}

/// The third strike ends the wall: the quartets not found stay off the
/// wall's tiles, and nothing more can be picked, tried or shuffled.
#[test]
fn the_third_strike_ends_the_wall() {
    let today = Utc::now().date_naive();
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(today),
            saved: None,
        }),
    );
    for words in [[0, 1, 2, 3], [4, 8, 12, 13], [4, 5, 8, 12], [8, 9, 12, 13]] {
        try_words(&mut state, words);
    }

    assert_eq!(
        (
            state.found(),
            state.strikes(),
            state.outcome(),
            state.message(),
            state.has_unfinished_daily(),
        ),
        (
            &[0][..],
            3,
            Outcome::OutOfStrikes,
            "Those split into two pairs. Third strike: here are the quartets.",
            false,
        )
    );
    assert!(!state.click(0));
    assert!(!state.submit());
    assert!(!state.shuffle());
}

/// Saved tries replay into the same wall. A try that is not four distinct
/// words still on the wall, repeats one before it, or is cut short is
/// dropped. A save for another day or another puzzle is ignored.
#[test]
fn saved_tries_replay_and_drop_what_the_wall_cannot_take() {
    let today = Utc::now().date_naive();
    let tries: &[i16] = &[
        0, 1, 2, 4, // a miss
        0, 1, 2, 3, // have keys
        99, 5, 6, 7, // no word 99
        4, 4, 5, 6, // a word twice
        0, 1, 2, 4, // no longer on the wall
        4, 5, 6, 7, // have shells
        8, 9, // cut short
    ];
    let state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(today),
            saved: Some(saved(today, 1, tries)),
        }),
    );
    assert_eq!(
        (
            state.tries(),
            state.found(),
            state.wall().len(),
            state.strikes(),
            state.message(),
            state.has_unfinished_daily(),
        ),
        (
            &[[0, 1, 2, 4], [0, 1, 2, 3], [4, 5, 6, 7]][..],
            &[0, 1][..],
            8,
            1,
            "Keep going.",
            true,
        )
    );

    for other in [
        saved(today - Duration::days(1), 1, &[0, 1, 2, 3]),
        saved(today, 2, &[0, 1, 2, 3]),
    ] {
        let state = State::new(
            Uuid::now_v7(),
            service(),
            Some(Today {
                daily: daily(today),
                saved: Some(other),
            }),
        );
        assert!(state.tries().is_empty());
    }
}

/// Everyone starts a day on the same deal of the sixteen words; `Tab`
/// deals the words left into another order and keeps the picks.
#[test]
fn the_opening_wall_is_the_days_and_shuffle_is_the_sessions() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let opening = opening_wall(today, 1);
    let mut sorted = opening;
    sorted.sort_unstable();
    assert_eq!(sorted, std::array::from_fn(|index| index as u8));
    assert_eq!(opening, opening_wall(today, 1));
    assert_ne!(opening, opening_wall(today + Duration::days(1), 1));

    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(today),
            saved: None,
        }),
    );
    assert_eq!(state.wall(), &opening[..]);
    pick(&mut state, &[0, 4, 0, 8]);
    let mut orders = HashSet::new();
    for _ in 0..4 {
        assert!(state.shuffle());
        orders.insert(state.wall().to_vec());
        let mut words = state.wall().to_vec();
        words.sort_unstable();
        assert_eq!(words, sorted);
    }
    assert!(orders.len() > 1);
    assert_eq!(
        (state.is_picked(4), state.is_picked(8), state.is_picked(0)),
        (true, true, false)
    );
}

/// A puzzle never claimed comes first; once all have had a day, the one
/// claimed longest ago comes round again.
#[test]
fn the_days_pick_prefers_unused_then_oldest() {
    let quartet = || Quartet {
        link: "Link".to_string(),
        words: std::array::from_fn(|_| "word".to_string()),
    };
    let pack: Vec<Puzzle> = (1..=3)
        .map(|id| Puzzle {
            id,
            quartets: std::array::from_fn(|_| quartet()),
        })
        .collect();
    let day = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();

    let used = HashMap::from([(1, day - Duration::days(2)), (3, day - Duration::days(1))]);
    assert_eq!(choose_for_date(&pack, &used, day).id, 2);
    let all_used = HashMap::from([
        (1, day - Duration::days(2)),
        (2, day - Duration::days(5)),
        (3, day - Duration::days(1)),
    ]);
    assert_eq!(choose_for_date(&pack, &all_used, day).id, 2);
}

/// A session left open past UTC midnight clears yesterday's wall at once;
/// a load that brings nothing says so and waits before trying again.
#[test]
fn the_rollover_clears_the_wall_and_backs_off_a_failed_fetch() {
    let yesterday = Utc::now().date_naive() - Duration::days(1);
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(yesterday),
            saved: None,
        }),
    );
    try_words(&mut state, [0, 1, 2, 3]);

    assert!(state.ensure_current_daily());
    assert_eq!(
        (
            state.is_loaded(),
            state.found().len(),
            state.wall().len(),
            state.message().to_string()
        ),
        (false, 0, 0, "Loading today's Quartets.".to_string())
    );
    assert!(!state.ensure_current_daily());
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<Today>>();
    state.load = DailyLoad::Loading(rx);
    drop(tx);
    assert!(state.poll_load());
    assert_eq!(state.message(), "Quartets is unavailable. Retrying soon.");
    assert!(!state.ensure_current_daily());
    assert!(!state.click(0));
}
