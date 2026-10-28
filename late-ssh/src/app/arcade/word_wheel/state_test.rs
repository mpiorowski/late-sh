use std::collections::HashSet;

use chrono::{Duration, NaiveDate, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::word_wheel::Game,
    word_wheel::{WHEEL_LETTERS, fits},
};
use tokio::sync::broadcast;

use super::super::svc::Daily;
use super::*;
use crate::app::activity::event::ActivityEvent;

fn service() -> WordWheelService {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    WordWheelService::new(db, activity)
}

/// Shipped wheel `rabcikt`: hub R, 42 answers, top score 110, so Bronze
/// needs 22, Silver 50 and Gold 77. BRICKBAT is its one full wheel.
fn daily(puzzle_date: NaiveDate) -> Daily {
    Daily {
        puzzle_date,
        wheel: find_wheel("rabcikt").expect("wheel in the pack"),
    }
}

fn play(state: &mut State, word: &str) {
    for letter in word.chars() {
        state.push_letter(letter);
    }
    state.submit();
}

fn saved(puzzle_date: NaiveDate, letters: &str, found: &[&str]) -> Game {
    Game {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        user_id: Uuid::now_v7(),
        puzzle_date,
        letters: letters.to_string(),
        found: found.iter().map(|word| word.to_string()).collect(),
    }
}

/// The pack validator: every shipped wheel is seven distinct letters with
/// no `s`, unique and in order, and its answers are alphabetical, unique,
/// all fit the wheel, number 25 to 70, and hold a full wheel.
#[test]
fn every_shipped_wheel_holds_its_own_answers() {
    let wheels = pack();
    assert_eq!(wheels.len(), 2000);
    for pair in wheels.windows(2) {
        assert!(pair[0].letters < pair[1].letters, "{}", pair[1].letters);
    }
    for wheel in wheels {
        let letters: HashSet<char> = wheel.letters.chars().collect();
        assert!(
            wheel.letters.len() == WHEEL_LETTERS
                && letters.len() == WHEEL_LETTERS
                && wheel
                    .letters
                    .chars()
                    .all(|letter| letter.is_ascii_lowercase())
                && !letters.contains(&'s'),
            "{}",
            wheel.letters
        );
        assert!(
            (25..=70).contains(&wheel.answers.len()),
            "{} has {} answers",
            wheel.letters,
            wheel.answers.len()
        );
        for pair in wheel.answers.windows(2) {
            assert!(pair[0] < pair[1], "{} lists {}", wheel.letters, pair[1]);
        }
        for answer in &wheel.answers {
            assert!(
                fits(&wheel.letters, answer),
                "{} lists {answer}",
                wheel.letters
            );
        }
        assert!(
            wheel
                .answers
                .iter()
                .any(|answer| is_full_wheel(&wheel.letters, answer)),
            "{} has no full wheel",
            wheel.letters
        );
    }
}

/// The ranks are cut at their share of the top score, rounded up.
#[test]
fn ranks_start_at_their_share_of_the_top_score() {
    let ranks: Vec<Rank> = [0, 21, 22, 49, 50, 76, 77, 109, 110]
        .into_iter()
        .map(|score| Rank::reached(score, 110))
        .collect();
    assert_eq!(
        ranks,
        [
            Rank::Start,
            Rank::Start,
            Rank::Bronze,
            Rank::Bronze,
            Rank::Silver,
            Rank::Silver,
            Rank::Gold,
            Rank::Gold,
            Rank::AllWords,
        ]
    );
}

/// A wheel played through: each kind of miss says what it was and scores
/// nothing, a letter off the wheel is refused, a full wheel scores double,
/// and the word that carries the score past 22 reaches Bronze.
#[test]
fn a_wheel_played_through_to_bronze() {
    let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(today),
            saved: None,
        }),
    );
    let mut messages = Vec::new();
    for word in ["rat", "attic", "brac", "brickbat", "brickbat"] {
        play(&mut state, word);
        messages.push(state.message().to_string());
    }
    state.push_letter('t');
    state.push_letter('z');
    messages.push(state.message().to_string());
    state.pop_letter();
    for word in ["backtrack", "baccarat", "barbaric"] {
        play(&mut state, word);
    }
    messages.push(state.message().to_string());

    assert_eq!(
        messages,
        [
            "RAT is too short: four letters or more.",
            "ATTIC misses the hub: every word uses R.",
            "BRAC is not in the word list.",
            "Full wheel! BRICKBAT +10.",
            "BRICKBAT is already found.",
            "There is no Z on today's wheel.",
            "BARBARIC +5. Bronze reached: 100 chips.",
        ]
    );
    assert_eq!(
        (
            state.found(),
            state.typed(),
            state.score(),
            state.max_score(),
            state.rank(),
            state.next_rank(),
            state.full_wheels_found(),
        ),
        (
            &[
                "baccarat".to_string(),
                "backtrack".to_string(),
                "barbaric".to_string(),
                "brickbat".to_string(),
            ][..],
            "",
            26,
            110,
            Rank::Bronze,
            Some((Rank::Silver, 24)),
            1,
        )
    );
}

/// A saved list comes back without the words the wheel does not accept,
/// and a rank it already reaches is not reached again. A save for another
/// day or another wheel is ignored.
#[test]
fn saved_words_restore_without_reaching_their_rank_again() {
    let today = Utc::now().date_naive();
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(today),
            saved: Some(saved(
                today,
                "rabcikt",
                &[
                    "brickbat",
                    "backtrack",
                    "nope",
                    "baccarat",
                    "barbaric",
                    "brickbat",
                ],
            )),
        }),
    );
    assert_eq!(
        (
            state.found().len(),
            state.score(),
            state.rank(),
            state.has_unfinished_daily(),
            state.message()
        ),
        (4, 26, Rank::Bronze, true, "Keep going.")
    );
    play(&mut state, "rabbit");
    assert_eq!((state.score(), state.message()), (29, "RABBIT +3."));

    for other in [
        saved(today - Duration::days(1), "rabcikt", &["brickbat"]),
        saved(today, "tabcikr", &["brickbat"]),
    ] {
        let state = State::new(
            Uuid::now_v7(),
            service(),
            Some(Today {
                daily: daily(today),
                saved: Some(other),
            }),
        );
        assert!(state.found().is_empty());
    }
}

/// `Tab` deals the rim into another order of the same six letters and
/// leaves the typed word alone.
#[test]
fn shuffle_reorders_the_rim_only() {
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(Utc::now().date_naive()),
            saved: None,
        }),
    );
    state.push_letter('r');
    let before = state.rim();
    let mut orders = HashSet::new();
    for _ in 0..6 {
        assert!(state.shuffle());
        let mut rim = state.rim();
        orders.insert(rim);
        rim.sort();
        assert_eq!(rim, before);
    }
    assert!(orders.len() > 1);
    assert_eq!(state.typed(), "r");
}

/// A session left open past UTC midnight clears yesterday's wheel at once;
/// a load that brings nothing says so and waits before trying again.
#[test]
fn the_rollover_clears_the_wheel_and_backs_off_a_failed_fetch() {
    let yesterday = Utc::now().date_naive() - Duration::days(1);
    let mut state = State::new(
        Uuid::now_v7(),
        service(),
        Some(Today {
            daily: daily(yesterday),
            saved: None,
        }),
    );
    play(&mut state, "brickbat");

    assert!(state.ensure_current_daily());
    assert_eq!(
        (
            state.is_loaded(),
            state.found().len(),
            state.message().to_string()
        ),
        (false, 0, "Loading today's Word Wheel.".to_string())
    );
    assert!(!state.ensure_current_daily());
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<Today>>();
    state.load = DailyLoad::Loading(rx);
    drop(tx);
    assert!(state.poll_load());
    assert_eq!(state.message(), "Word Wheel is unavailable. Retrying soon.");
    assert!(!state.ensure_current_daily());
    assert!(!state.push_letter('r'));
}
