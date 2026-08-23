use chrono::NaiveDate;
use late_core::models::le_word::{DailyWord, Game, GameParams};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::svc::LeWordService;

/// Mirrors the `le_word_daily_daily_win` reward template. Update both together.
pub const DAILY_WIN_REWARD_CHIPS: i64 = 250;
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
    /// In flight only while a rolled-over day is fetching its word; see
    /// `ensure_current_daily`.
    word_reload_rx: Option<tokio::sync::oneshot::Receiver<Option<DailyWord>>>,
    /// Set when a rollover fetch failed; `ensure_current_daily` holds off
    /// until this passes, then tries again. Without it a transient DB error
    /// at midnight left the board dead for the rest of the session.
    word_reload_backoff_until: Option<std::time::Instant>,
    pub svc: LeWordService,
}

/// How long a failed rollover word fetch waits before the next attempt.
const WORD_RELOAD_RETRY: std::time::Duration = std::time::Duration::from_secs(30);

impl State {
    pub fn new(
        user_id: Uuid,
        svc: LeWordService,
        daily_word: Option<DailyWord>,
        saved_game: Option<Game>,
    ) -> Self {
        let daily_word_loaded = daily_word.is_some();
        let puzzle_date = daily_word
            .as_ref()
            .map(|word| word.puzzle_date)
            .unwrap_or_else(|| svc.today());
        let answer = daily_word.map(|word| word.answer_word).unwrap_or_default();
        let mut state = Self {
            user_id,
            puzzle_date,
            answer,
            daily_word_loaded,
            guesses: Vec::new(),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
            show_rules: false,
            message: if daily_word_loaded {
                "Guess today's Le Word.".to_string()
            } else {
                "Le Word is unavailable. Try again soon.".to_string()
            },
            word_reload_rx: None,
            word_reload_backoff_until: None,
            svc,
        };
        if let Some(game) = saved_game
            && game.puzzle_date == state.puzzle_date
            && game.answer_word == state.answer
        {
            state.guesses = serde_json::from_value(game.guesses).unwrap_or_default();
            state.current_guess = game.current_guess;
            state.is_game_over = game.is_game_over;
            state.won = game.won;
            state.message = if state.won {
                format!("Solved in {}.", state.guesses.len())
            } else if state.is_game_over {
                format!("The word was {}.", state.answer.to_uppercase())
            } else {
                "Keep going.".to_string()
            };
        }
        state
    }

    /// Roll the round over when the UTC date changes under a live session.
    /// The word itself lives in the database (the session's first one arrives
    /// with the bootstrap), so this clears the board immediately and fetches
    /// the new word in the background; `poll_word_reload` installs it.
    /// `puzzle_date` only advances once the word lands, so a failed fetch is
    /// retried here (after `WORD_RELOAD_RETRY`) instead of leaving the board
    /// dead until reconnect. Returns true when the screen changed.
    pub fn ensure_current_daily(&mut self) -> bool {
        let today = self.svc.today();
        if self.puzzle_date == today {
            return false;
        }
        if self.word_reload_rx.is_some() {
            return false;
        }
        if let Some(until) = self.word_reload_backoff_until
            && std::time::Instant::now() < until
        {
            return false;
        }
        self.word_reload_backoff_until = None;

        // Yesterday's word must stop scoring guesses right away, whether or
        // not the fetch succeeds.
        self.answer = String::new();
        self.daily_word_loaded = false;
        self.guesses.clear();
        self.current_guess.clear();
        self.is_game_over = false;
        self.won = false;
        self.message = "Loading today's Le Word.".to_string();

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.word_reload_rx = Some(rx);
        let svc = self.svc.clone();
        // Pure state tests drive this without a runtime; the state is already
        // cleared, and the poll below installs nothing when nothing arrives.
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let word = match svc.ensure_daily_word().await {
                    Ok(word) => Some(word),
                    Err(error) => {
                        tracing::error!(error = ?error, "failed to load the rolled-over Le Word");
                        None
                    }
                };
                let _ = tx.send(word);
            });
        }
        true
    }

    /// Install a word fetched by `ensure_current_daily`. Returns true when the
    /// screen changed.
    pub fn poll_word_reload(&mut self) -> bool {
        let Some(rx) = self.word_reload_rx.as_mut() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Some(word)) => {
                self.word_reload_rx = None;
                self.word_reload_backoff_until = None;
                self.puzzle_date = word.puzzle_date;
                self.answer = word.answer_word;
                self.daily_word_loaded = true;
                self.message = "Guess today's Le Word.".to_string();
                true
            }
            Ok(None) | Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                self.word_reload_rx = None;
                // `puzzle_date` was not advanced, so `ensure_current_daily`
                // tries again once the backoff passes.
                self.word_reload_backoff_until =
                    Some(std::time::Instant::now() + WORD_RELOAD_RETRY);
                self.message = "Le Word is unavailable. Retrying soon.".to_string();
                true
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => false,
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
            self.message = format!("Solved in {}.", self.guesses.len());
            self.save_async();
            self.svc
                .record_win_task(self.user_id, self.puzzle_date, self.guesses.len());
            return true;
        }

        if self.guesses.len() >= MAX_GUESSES {
            self.is_game_over = true;
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
// drive the private reload channel and backoff directly.
#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
