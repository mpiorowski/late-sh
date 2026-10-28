use std::collections::HashMap;

use chrono::{Duration, NaiveDate};

use super::word_wheel::{WordWheel, choose_for_date, fits, is_full_wheel};

fn wheel(letters: &str) -> WordWheel {
    WordWheel {
        letters: letters.to_string(),
        answers: Vec::new(),
    }
}

/// The rule the pack is cut by: four letters or more, all on the wheel,
/// repeats allowed, the hub required.
#[test]
fn a_word_fits_when_it_is_long_enough_on_the_wheel_and_uses_the_hub() {
    let letters = "racdeit";
    let words = [
        "tired", "edit", "credit", "trace", "rat", "dirt", "arid", "radar", "cart", "radicate",
    ];
    let verdicts: Vec<(&str, bool, bool)> = words
        .into_iter()
        .map(|word| (word, fits(letters, word), is_full_wheel(letters, word)))
        .collect();
    assert_eq!(
        verdicts,
        [
            ("tired", true, false),
            ("edit", false, false),
            ("credit", true, false),
            ("trace", true, false),
            ("rat", false, false),
            ("dirt", true, false),
            ("arid", true, false),
            ("radar", true, false),
            ("cart", true, false),
            ("radicate", true, true),
        ]
    );
}

/// Unclaimed wheels come first, picked by the date; once all are used, the
/// one claimed longest ago comes round.
#[test]
fn the_pick_prefers_unused_wheels_then_the_one_used_longest_ago() {
    let pack: Vec<WordWheel> = ["abcdefg", "hijklmn", "opqrtuv"]
        .into_iter()
        .map(wheel)
        .collect();
    let day = NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date");

    let mut last_used = HashMap::new();
    let first = choose_for_date(&pack, &last_used, day).letters.clone();
    last_used.insert(first.clone(), day);
    let second = choose_for_date(&pack, &last_used, day + Duration::days(1))
        .letters
        .clone();
    assert_ne!(first, second);
    last_used.insert(second.clone(), day + Duration::days(1));
    let third = choose_for_date(&pack, &last_used, day + Duration::days(2))
        .letters
        .clone();
    last_used.insert(third.clone(), day + Duration::days(2));

    let mut all = vec![first.clone(), second, third];
    all.sort();
    assert_eq!(all, ["abcdefg", "hijklmn", "opqrtuv"]);
    assert_eq!(
        choose_for_date(&pack, &last_used, day + Duration::days(3)).letters,
        first
    );
}
