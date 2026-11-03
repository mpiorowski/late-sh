use chrono::NaiveDate;
use late_core::models::le_word::GameParams;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::svc::{LeWordService, Today};
use crate::app::arcade::daily_load::{DailyLoad, Polled};
use crate::metrics::{ArcadeDifficulty, ArcadeFinish, ArcadeMode};

pub const WORD_LEN: usize = 5;
pub const MAX_GUESSES: usize = 6;
pub const DAILY_DIFFICULTY_KEY: &str = "daily";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LetterScore {
    Correct,
    Present,
    Absent,
}

pub struct State {
    pub user_id: Uuid,
    pub puzzle_date: NaiveDate,
    pub answer: String,
    pub daily_word_loaded: bool,
    pub guesses: Vec<String>,
    pub current_guess: String,
    pub is_game_over: bool,
    pub won: bool,
    pub show_rules: bool,
    pub message: String,
    /// Today's word while the board has none; see `poll_load`.
    load: DailyLoad<Today>,
    pub svc: LeWordService,
}

impl State {
    /// A session's board from its connect-time load; `None` (the load
    /// failed) shows the board unavailable and retries from `poll_load`.
    pub fn new(user_id: Uuid, svc: LeWordService, today: Option<Today>) -> Self {
        let mut state = Self {
            user_id,
            puzzle_date: svc.today(),
            answer: String::new(),
            daily_word_loaded: false,
            guesses: Vec::new(),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
            show_rules: false,
            message: "Le Word is unavailable. Try again soon.".to_string(),
            load: DailyLoad::after_connect(today.is_some()),
            svc,
        };
        if let Some(today) = today {
            state.install(today);
        }
        state
    }

    /// Put a day's word on the board with the saved round that belongs to
    /// it.
    fn install(&mut self, today: Today) {
        let Today { word, saved } = today;
        self.puzzle_date = word.puzzle_date;
        self.answer = word.answer_word;
        self.daily_word_loaded = true;
        self.guesses.clear();
        self.current_guess.clear();
        self.is_game_over = false;
        self.won = false;
        self.message = "Guess today's Le Word.".to_string();
        if let Some(game) = saved
            && game.puzzle_date == self.puzzle_date
            && game.answer_word == self.answer
        {
            self.guesses = serde_json::from_value(game.guesses).unwrap_or_default();
            self.current_guess = game.current_guess;
            self.is_game_over = game.is_game_over;
            self.won = game.won;
            self.message = if self.won {
                format!("Solved in {}.", self.guesses.len())
            } else if self.is_game_over {
                format!("The word was {}.", self.answer.to_uppercase())
            } else {
                "Keep going.".to_string()
            };
        }
    }

    /// Roll the round over when the UTC date changes under a live session:
    /// yesterday's word comes off at once, and `poll_load` fetches today's.
    /// Returns true when the screen changed.
    pub fn ensure_current_daily(&mut self) -> bool {
        if !self.daily_word_loaded || self.puzzle_date == self.svc.today() {
            return false;
        }
        // Yesterday's word must stop scoring guesses right away, whether or
        // not today's loads.
        self.answer = String::new();
        self.daily_word_loaded = false;
        self.guesses.clear();
        self.current_guess.clear();
        self.is_game_over = false;
        self.won = false;
        self.message = "Loading today's Le Word.".to_string();
        true
    }

    /// Load today's word and saved round whenever the board has no word (a
    /// failed connect-time load, or the rollover), and install what lands.
    /// Runs on every tick, board open or not. Returns true when the screen
    /// changed.
    pub fn poll_load(&mut self) -> bool {
        let (svc, user_id) = (&self.svc, self.user_id);
        match self
            .load
            .poll(!self.daily_word_loaded, || svc.load_today_task(user_id))
        {
            Polled::Waiting => false,
            Polled::Loaded(today) => {
                self.install(today);
                true
            }
            Polled::Failed => {
                self.message = "Le Word is unavailable. Retrying soon.".to_string();
                true
            }
        }
    }

    /// Today's word has at least one submitted guess and the run is not over.
    pub fn has_unfinished_daily(&self) -> bool {
        self.daily_word_loaded
            && !self.guesses.is_empty()
            && !self.is_game_over
            && self.puzzle_date == self.svc.today()
    }

    pub fn guess_number(&self) -> usize {
        self.guesses
            .len()
            .saturating_add((!self.is_game_over) as usize)
    }

    pub fn submit_guess(&mut self) -> bool {
        if !self.daily_word_loaded {
            self.message = "Le Word is unavailable. Try again soon.".to_string();
            return true;
        }
        if self.is_game_over {
            return false;
        }
        if self.current_guess.len() != WORD_LEN {
            self.message = "Not enough letters.".to_string();
            return true;
        }
        if !self.svc.is_valid_guess(&self.current_guess) {
            self.message = "Not in word list.".to_string();
            return true;
        }

        let guess = std::mem::take(&mut self.current_guess);
        self.guesses.push(guess.clone());
        if guess == self.answer {
            self.won = true;
            self.is_game_over = true;
            self.svc.record_finish(
                ArcadeMode::Daily,
                ArcadeDifficulty::Single,
                ArcadeFinish::Won,
            );
            self.message = format!("Solved in {}.", self.guesses.len());
            self.save_async();
            self.svc
                .record_win_task(self.user_id, self.puzzle_date, self.guesses.len());
            return true;
        }

        if self.guesses.len() >= MAX_GUESSES {
            self.is_game_over = true;
            self.svc.record_finish(
                ArcadeMode::Daily,
                ArcadeDifficulty::Single,
                ArcadeFinish::Lost,
            );
            self.message = format!("The word was {}.", self.answer.to_uppercase());
        } else {
            self.message = "Try again.".to_string();
        }
        self.save_async();
        true
    }

    pub fn push_letter(&mut self, ch: char) -> bool {
        if !self.daily_word_loaded
            || self.is_game_over
            || self.current_guess.len() >= WORD_LEN
            || !ch.is_ascii_alphabetic()
        {
            return false;
        }
        self.current_guess.push(ch.to_ascii_lowercase());
        self.message.clear();
        true
    }

    pub fn pop_letter(&mut self) -> bool {
        if !self.daily_word_loaded || self.is_game_over {
            return false;
        }
        self.current_guess.pop().is_some()
    }

    pub fn scores_for_guess(&self, guess: &str) -> [LetterScore; WORD_LEN] {
        score_guess(guess, &self.answer)
    }

    pub fn score_for_keyboard_letter(&self, letter: char) -> Option<LetterScore> {
        score_letter_from_guesses(&self.guesses, &self.answer, letter)
    }

    pub fn open_rules(&mut self) {
        self.show_rules = true;
    }

    pub fn close_rules(&mut self) {
        self.show_rules = false;
    }

    fn save_async(&self) {
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            puzzle_date: self.puzzle_date,
            answer_word: self.answer.clone(),
            guesses: serde_json::to_value(&self.guesses).unwrap_or_default(),
            current_guess: self.current_guess.clone(),
            is_game_over: self.is_game_over,
            won: self.won,
        });
    }
}

pub fn score_guess(guess: &str, answer: &str) -> [LetterScore; WORD_LEN] {
    let guess = guess.as_bytes();
    let answer = answer.as_bytes();
    let mut scores = [LetterScore::Absent; WORD_LEN];
    let mut remaining = [0u8; 26];

    for (idx, score) in scores.iter_mut().enumerate() {
        if guess.get(idx) == answer.get(idx) {
            *score = LetterScore::Correct;
        } else if let Some(&b) = answer.get(idx)
            && b.is_ascii_lowercase()
        {
            remaining[(b - b'a') as usize] += 1;
        }
    }

    for (idx, score) in scores.iter_mut().enumerate() {
        if *score == LetterScore::Correct {
            continue;
        }
        let Some(&b) = guess.get(idx) else {
            continue;
        };
        if !b.is_ascii_lowercase() {
            continue;
        }
        let count = &mut remaining[(b - b'a') as usize];
        if *count > 0 {
            *score = LetterScore::Present;
            *count -= 1;
        }
    }

    scores
}

pub fn score_letter_from_guesses(
    guesses: &[String],
    answer: &str,
    letter: char,
) -> Option<LetterScore> {
    let letter = letter.to_ascii_lowercase();
    if !letter.is_ascii_lowercase() {
        return None;
    }

    let mut best = None;
    for guess in guesses {
        let scores = score_guess(guess, answer);
        for (idx, ch) in guess.chars().enumerate().take(WORD_LEN) {
            if ch.to_ascii_lowercase() != letter {
                continue;
            }
            if best.is_none_or(|score| score_rank(scores[idx]) > score_rank(score)) {
                best = Some(scores[idx]);
            }
        }
    }
    best
}

fn score_rank(score: LetterScore) -> u8 {
    match score {
        LetterScore::Correct => 3,
        LetterScore::Present => 2,
        LetterScore::Absent => 1,
    }
}

// A child of this module (not a sibling in mod.rs) so the rollover tests can
// drive the private daily load directly. `pub(crate)` so the
// UI tests share its `state_for` fixture.
#[cfg(test)]
#[path = "state_test.rs"]
pub(crate) mod state_test;
