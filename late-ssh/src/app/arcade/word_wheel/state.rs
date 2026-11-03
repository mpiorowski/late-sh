//! Word Wheel: one wheel a day for everyone, seven letters with a hub.
//! Words of four letters or more, spelled from the wheel and using the hub,
//! score a point per letter past three, doubled for a full wheel (a word
//! that uses all seven). The ranks are shares of the wheel's top score;
//! Bronze, Silver and Gold are its easy, medium and hard wins, all banked on
//! the same wheel as the score crosses them.

use std::sync::OnceLock;

use chrono::NaiveDate;
use late_core::{
    daily,
    models::{chips::Difficulty, word_wheel::GameParams},
    word_wheel::{WHEEL_LETTERS, WordWheel, is_full_wheel},
};
use uuid::Uuid;

use super::svc::{Today, WordWheelService};
use crate::app::arcade::daily_load::{DailyLoad, Polled};
use crate::metrics::{ArcadeDifficulty, ArcadeFinish, ArcadeMode};

const PACK: &[u8] = include_bytes!("../../../../assets/word_wheel/wheels.json");
static WHEELS: OnceLock<Vec<WordWheel>> = OnceLock::new();

/// The most letters the typed line holds; the pack's longest answer has 15.
pub const MAX_TYPED: usize = 20;
const UNAVAILABLE: &str = "Word Wheel is unavailable. Try again soon.";
const READY: &str = "Spell words from the wheel. Every word uses the hub.";

/// The embedded pack, alphabetical by letters, parsed once per process.
pub fn pack() -> &'static [WordWheel] {
    WHEELS.get_or_init(|| serde_json::from_slice(PACK).expect("the word wheel pack parses"))
}

/// The pack wheel with these letters (hub first).
pub fn find_wheel(letters: &str) -> Option<&'static WordWheel> {
    let wheels = pack();
    wheels
        .binary_search_by(|wheel| wheel.letters.as_str().cmp(letters))
        .ok()
        .map(|index| &wheels[index])
}

/// A word's points on the wheel `letters`: one per letter past three,
/// doubled when it uses all seven letters.
pub fn word_score(letters: &str, word: &str) -> u32 {
    let base = word.len() as u32 - 3;
    if is_full_wheel(letters, word) {
        base * 2
    } else {
        base
    }
}

/// How far up the wheel a player is, as a share of its top score.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Start,
    Bronze,
    Silver,
    Gold,
    AllWords,
}

impl Rank {
    pub const ALL: [Rank; 5] = [
        Rank::Start,
        Rank::Bronze,
        Rank::Silver,
        Rank::Gold,
        Rank::AllWords,
    ];

    /// The share of the wheel's top score this rank needs, in percent.
    pub const fn percent(self) -> u32 {
        match self {
            Rank::Start => 0,
            Rank::Bronze => 20,
            Rank::Silver => 45,
            Rank::Gold => 70,
            Rank::AllWords => 100,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Rank::Start => "Start",
            Rank::Bronze => "Bronze",
            Rank::Silver => "Silver",
            Rank::Gold => "Gold",
            Rank::AllWords => "Every word",
        }
    }

    /// The daily board this rank pays for.
    pub const fn difficulty(self) -> Option<Difficulty> {
        match self {
            Rank::Start | Rank::AllWords => None,
            Rank::Bronze => Some(Difficulty::Easy),
            Rank::Silver => Some(Difficulty::Medium),
            Rank::Gold => Some(Difficulty::Hard),
        }
    }

    /// The score this rank needs on a wheel whose top score is `max`.
    pub const fn needs(self, max: u32) -> u32 {
        (self.percent() * max).div_ceil(100)
    }

    /// The highest rank `score` reaches on a wheel whose top score is `max`.
    pub fn reached(score: u32, max: u32) -> Rank {
        Rank::ALL
            .into_iter()
            .rev()
            .find(|rank| score >= rank.needs(max))
            .unwrap_or(Rank::Start)
    }
}

/// What a submitted word comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    TooShort,
    MissingHub,
    NotInList,
    AlreadyFound,
    Found,
}

pub struct State {
    user_id: Uuid,
    svc: WordWheelService,
    puzzle_date: NaiveDate,
    /// `None` while today's wheel is loading or could not be loaded.
    wheel: Option<&'static WordWheel>,
    /// Alphabetical, every one an answer of `wheel`.
    found: Vec<String>,
    typed: String,
    /// The rim as drawn; `Tab` shuffles it, the session forgets it.
    rim: [char; WHEEL_LETTERS - 1],
    shuffles: u64,
    /// The highest rank already sent to be banked. Ranks only rise, so
    /// every paid rank at or below it has been sent.
    banked: Rank,
    show_rules: bool,
    message: String,
    /// Today's wheel while the board has none; see `poll_load`.
    load: DailyLoad<Today>,
}

impl State {
    /// A session's board from its connect-time load; `None` (the load
    /// failed) shows the board unavailable and retries from `poll_load`.
    pub fn new(user_id: Uuid, svc: WordWheelService, today: Option<Today>) -> Self {
        let puzzle_date = match &today {
            Some(today) => today.daily.puzzle_date,
            None => svc.today(),
        };
        let mut state = Self {
            user_id,
            svc,
            puzzle_date,
            wheel: None,
            found: Vec::new(),
            typed: String::new(),
            rim: ['a'; WHEEL_LETTERS - 1],
            shuffles: 0,
            banked: Rank::Start,
            show_rules: false,
            message: UNAVAILABLE.to_string(),
            load: DailyLoad::after_connect(today.is_some()),
        };
        if let Some(today) = today {
            state.install(today);
        }
        state
    }

    /// Put a day's wheel on the board with the saved words that belong to
    /// it.
    fn install(&mut self, today: Today) {
        let Today { daily, saved } = today;
        self.puzzle_date = daily.puzzle_date;
        self.wheel = Some(daily.wheel);
        self.found.clear();
        self.typed.clear();
        self.rim = daily.wheel.rim();
        self.shuffles = 0;
        self.banked = Rank::Start;
        self.message = READY.to_string();
        if let Some(game) = saved
            && game.puzzle_date == daily.puzzle_date
            && game.letters == daily.wheel.letters
        {
            self.restore(game.found);
        }
    }

    /// Take a saved word list. Words the wheel does not accept are dropped,
    /// and every paid rank the list reaches is banked again: the stored list
    /// merges every session's words, so it can pass a rank no single session
    /// banked, and a rank already recorded pays nothing twice.
    fn restore(&mut self, saved: Vec<String>) {
        let Some(wheel) = self.wheel else {
            return;
        };
        let mut found: Vec<String> = saved
            .into_iter()
            .filter(|word| is_answer(wheel, word))
            .collect();
        found.sort();
        found.dedup();
        self.found = found;
        self.banked = self.rank();
        let score = self.score() as i32;
        for paid in Rank::ALL {
            if let Some(difficulty) = paid.difficulty()
                && paid <= self.banked
            {
                self.bank_async(difficulty, score);
            }
        }
        if !self.found.is_empty() {
            self.message = "Keep going.".to_string();
        }
    }

    /// Roll the wheel over when the UTC date changes under a live session:
    /// yesterday's wheel comes off at once, and `poll_load` fetches today's.
    /// Returns true when the screen changed.
    pub fn ensure_current_daily(&mut self) -> bool {
        if self.wheel.is_none() || self.puzzle_date == self.svc.today() {
            return false;
        }
        // Yesterday's wheel must stop taking words right away, whether or
        // not today's loads.
        self.wheel = None;
        self.found.clear();
        self.typed.clear();
        self.banked = Rank::Start;
        self.message = "Loading today's Word Wheel.".to_string();
        true
    }

    /// Load today's wheel and saved words whenever the board has no wheel (a
    /// failed connect-time load, or the rollover), and install what lands.
    /// Runs on every tick, board open or not. Returns true when the screen
    /// changed.
    pub fn poll_load(&mut self) -> bool {
        let (svc, user_id) = (&self.svc, self.user_id);
        match self
            .load
            .poll(self.wheel.is_none(), || svc.load_today_task(user_id))
        {
            Polled::Waiting => false,
            Polled::Loaded(today) => {
                self.install(today);
                true
            }
            Polled::Failed => {
                self.message = "Word Wheel is unavailable. Retrying soon.".to_string();
                true
            }
        }
    }

    /// Today's wheel has a word found and Gold, the last paid rank, is
    /// still ahead.
    pub fn has_unfinished_daily(&self) -> bool {
        self.wheel.is_some()
            && !self.found.is_empty()
            && self.rank() < Rank::Gold
            && self.puzzle_date == self.svc.today()
    }

    pub fn puzzle_date(&self) -> NaiveDate {
        self.puzzle_date
    }

    pub fn is_loaded(&self) -> bool {
        self.wheel.is_some()
    }

    /// The hub letter, once the wheel is loaded.
    pub fn hub(&self) -> Option<char> {
        self.wheel.map(WordWheel::hub)
    }

    /// The rim in the order drawn.
    pub fn rim(&self) -> [char; WHEEL_LETTERS - 1] {
        self.rim
    }

    pub fn typed(&self) -> &str {
        &self.typed
    }

    pub fn found(&self) -> &[String] {
        &self.found
    }

    pub fn answer_count(&self) -> usize {
        self.wheel.map_or(0, |wheel| wheel.answers.len())
    }

    pub fn is_full_wheel(&self, word: &str) -> bool {
        self.wheel
            .is_some_and(|wheel| is_full_wheel(&wheel.letters, word))
    }

    pub fn full_wheels_found(&self) -> usize {
        self.found
            .iter()
            .filter(|word| self.is_full_wheel(word))
            .count()
    }

    pub fn score(&self) -> u32 {
        match self.wheel {
            Some(wheel) => self
                .found
                .iter()
                .map(|word| word_score(&wheel.letters, word))
                .sum(),
            None => 0,
        }
    }

    /// The wheel's top score: every answer found.
    pub fn max_score(&self) -> u32 {
        match self.wheel {
            Some(wheel) => wheel
                .answers
                .iter()
                .map(|word| word_score(&wheel.letters, word))
                .sum(),
            None => 0,
        }
    }

    pub fn rank(&self) -> Rank {
        match self.wheel {
            Some(_) => Rank::reached(self.score(), self.max_score()),
            None => Rank::Start,
        }
    }

    /// The next rank up and the points still needed for it.
    pub fn next_rank(&self) -> Option<(Rank, u32)> {
        let rank = self.rank();
        let max = self.max_score();
        Rank::ALL
            .into_iter()
            .find(|next| *next > rank)
            .map(|next| (next, next.needs(max).saturating_sub(self.score())))
    }

    pub fn show_rules(&self) -> bool {
        self.show_rules
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn open_rules(&mut self) {
        self.show_rules = true;
    }

    pub fn close_rules(&mut self) {
        self.show_rules = false;
    }

    /// Type a letter. Only the wheel's letters go in; another one is
    /// named in the message instead.
    pub fn push_letter(&mut self, letter: char) -> bool {
        let Some(wheel) = self.wheel else {
            return false;
        };
        let letter = letter.to_ascii_lowercase();
        if !wheel.letters.contains(letter) {
            self.message = format!(
                "There is no {} on today's wheel.",
                letter.to_ascii_uppercase()
            );
            return true;
        }
        if self.typed.len() >= MAX_TYPED {
            return false;
        }
        self.typed.push(letter);
        self.message.clear();
        true
    }

    pub fn pop_letter(&mut self) -> bool {
        self.wheel.is_some() && self.typed.pop().is_some()
    }

    /// Deal the rim into a new order. The order is the session's alone,
    /// so it is drawn from a counter, not a clock.
    pub fn shuffle(&mut self) -> bool {
        if self.wheel.is_none() {
            return false;
        }
        self.shuffles += 1;
        daily::shuffle(&mut self.rim, self.shuffles);
        true
    }

    /// Submit the typed word: a new answer scores and saves, and every paid
    /// rank it carries the score past is banked.
    pub fn submit(&mut self) -> bool {
        let Some(wheel) = self.wheel else {
            self.message = UNAVAILABLE.to_string();
            return true;
        };
        if self.typed.is_empty() {
            return false;
        }
        let word = std::mem::take(&mut self.typed);
        let shown = word.to_ascii_uppercase();
        self.message = match self.verdict(wheel, &word) {
            Verdict::TooShort => format!("{shown} is too short: four letters or more."),
            Verdict::MissingHub => format!(
                "{shown} misses the hub: every word uses {}.",
                wheel.hub().to_ascii_uppercase()
            ),
            Verdict::NotInList => format!("{shown} is not in the word list."),
            Verdict::AlreadyFound => format!("{shown} is already found."),
            Verdict::Found => {
                let points = word_score(&wheel.letters, &word);
                let full_wheel = is_full_wheel(&wheel.letters, &word);
                let position = self.found.binary_search(&word).unwrap_err();
                self.found.insert(position, word);
                let found = if full_wheel {
                    format!("Full wheel! {shown} +{points}.")
                } else {
                    format!("{shown} +{points}.")
                };
                match self.bank_new_ranks() {
                    Some(rank) => format!("{found} {}", rank_message(rank)),
                    None => found,
                }
            }
        };
        true
    }

    fn verdict(&self, wheel: &WordWheel, word: &str) -> Verdict {
        if word.len() < late_core::word_wheel::MIN_WORD_LEN {
            Verdict::TooShort
        } else if !word.contains(wheel.hub()) {
            Verdict::MissingHub
        } else if !is_answer(wheel, word) {
            Verdict::NotInList
        } else if self
            .found
            .binary_search_by(|found| found.as_str().cmp(word))
            .is_ok()
        {
            Verdict::AlreadyFound
        } else {
            Verdict::Found
        }
    }

    /// Save the words, and bank every paid rank between the last banked
    /// one and the rank now reached. Returns the rank when it rose.
    fn bank_new_ranks(&mut self) -> Option<Rank> {
        let wheel = self.wheel?;
        let rank = self.rank();
        let score = self.score() as i32;
        let risen = rank > self.banked;
        for paid in Rank::ALL {
            let Some(difficulty) = paid.difficulty() else {
                continue;
            };
            if paid > self.banked && paid <= rank {
                self.svc.record_finish(
                    ArcadeMode::Daily,
                    ArcadeDifficulty::of(difficulty),
                    ArcadeFinish::Won,
                );
                self.bank_async(difficulty, score);
            }
        }
        self.banked = self.banked.max(rank);
        self.save_async(wheel);
        risen.then_some(rank)
    }

    // Pure state tests run without a runtime, and the save worker is a
    // spawned task; they assert on the state and the svc tests on the rows.
    fn bank_async(&self, difficulty: Difficulty, score: i32) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc
            .bank_task(self.user_id, difficulty, self.puzzle_date, score);
    }

    fn save_async(&self, wheel: &WordWheel) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            puzzle_date: self.puzzle_date,
            letters: wheel.letters.clone(),
            found: self.found.clone(),
        });
    }
}

fn is_answer(wheel: &WordWheel, word: &str) -> bool {
    wheel
        .answers
        .binary_search_by(|answer| answer.as_str().cmp(word))
        .is_ok()
}

fn rank_message(rank: Rank) -> String {
    match rank.difficulty() {
        Some(difficulty) => format!("{} reached: {} chips.", rank.label(), difficulty.chips()),
        None => match rank {
            Rank::AllWords => "Every word found!".to_string(),
            Rank::Start | Rank::Bronze | Rank::Silver | Rank::Gold => {
                format!("{} reached.", rank.label())
            }
        },
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
