//! The Word Wheel pack: seven-letter wheels cut from the SCOWL word list by
//! `gen_word_wheel`, one JSON file late-ssh embeds, the word rule and answer
//! bounds the generator applies and the shipped pack is held to, and the
//! pick of the day's wheel.

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::daily;

/// The letters on a wheel: the hub and six on the rim.
pub const WHEEL_LETTERS: usize = 7;
/// The shortest word a wheel accepts.
pub const MIN_WORD_LEN: usize = 4;
/// The fewest answers a wheel in the pack holds.
pub const MIN_ANSWERS: usize = 25;
/// The most answers a wheel in the pack holds.
pub const MAX_ANSWERS: usize = 70;

/// One wheel as the pack stores it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordWheel {
    /// The hub letter, then the six rim letters in alphabetical order. Seven
    /// distinct lowercase letters, never `s`; also the wheel's id.
    pub letters: String,
    /// Every word the wheel accepts, alphabetical.
    pub answers: Vec<String>,
}

impl WordWheel {
    /// The letter every word must use.
    pub fn hub(&self) -> char {
        self.letters
            .chars()
            .next()
            .expect("a pack wheel has seven letters")
    }

    /// The six rim letters, alphabetical.
    pub fn rim(&self) -> [char; WHEEL_LETTERS - 1] {
        let mut rim = ['a'; WHEEL_LETTERS - 1];
        for (slot, letter) in rim.iter_mut().zip(self.letters.chars().skip(1)) {
            *slot = letter;
        }
        rim
    }
}

/// Whether `word` counts on the wheel `letters` (hub first): at least
/// `MIN_WORD_LEN` letters, every one of them on the wheel (repeats allowed),
/// the hub among them.
pub fn fits(letters: &str, word: &str) -> bool {
    let Some(hub) = letters.chars().next() else {
        return false;
    };
    word.len() >= MIN_WORD_LEN
        && word.contains(hub)
        && word.chars().all(|letter| letters.contains(letter))
}

/// Whether `word` uses all seven letters of the wheel `letters`.
pub fn is_full_wheel(letters: &str, word: &str) -> bool {
    letters.chars().all(|letter| word.contains(letter))
}

/// The wheel to claim for `date`, by the shared daily pick
/// (`daily::choose_for_date`) keyed on the letters and seeded by the date.
/// `last_used` maps a wheel's letters to the last day it was claimed.
pub fn choose_for_date<'a>(
    pack: &'a [WordWheel],
    last_used: &HashMap<String, NaiveDate>,
    date: NaiveDate,
) -> &'a WordWheel {
    daily::choose_for_date(
        pack,
        |wheel| &wheel.letters,
        last_used,
        &format!("late-sh-word-wheel-daily-v1:{date}"),
    )
    .expect("the word wheel pack is not empty")
}
