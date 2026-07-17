use chrono::NaiveDate;
use late_core::models::le_word::{DailyWord, Game, GameParams};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::svc::LeWordService;

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
    pub svc: LeWordService,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_guess_handles_duplicate_letters() {
        assert_eq!(
            score_guess("allee", "apple"),
            [
                LetterScore::Correct,
                LetterScore::Present,
                LetterScore::Absent,
                LetterScore::Absent,
                LetterScore::Correct,
            ]
        );
        assert_eq!(
            score_guess("sassy", "abyss"),
            [
                LetterScore::Present,
                LetterScore::Present,
                LetterScore::Absent,
                LetterScore::Correct,
                LetterScore::Present,
            ]
        );
    }

    #[test]
    fn score_guess_matches_shade_screenshot_case() {
        assert_eq!(
            score_guess("wormy", "shade"),
            [
                LetterScore::Absent,
                LetterScore::Absent,
                LetterScore::Absent,
                LetterScore::Absent,
                LetterScore::Absent,
            ]
        );
        assert_eq!(
            score_guess("adieu", "shade"),
            [
                LetterScore::Present,
                LetterScore::Present,
                LetterScore::Absent,
                LetterScore::Present,
                LetterScore::Absent,
            ]
        );
        assert_eq!(
            score_guess("adeem", "shade"),
            [
                LetterScore::Present,
                LetterScore::Present,
                LetterScore::Present,
                LetterScore::Absent,
                LetterScore::Absent,
            ]
        );
        assert_eq!(
            score_guess("house", "shade"),
            [
                LetterScore::Present,
                LetterScore::Absent,
                LetterScore::Absent,
                LetterScore::Present,
                LetterScore::Correct,
            ]
        );
    }

    #[test]
    fn score_letter_from_guesses_keeps_best_keyboard_hint() {
        let guesses = vec!["allee".to_string(), "sassy".to_string()];

        assert_eq!(
            score_letter_from_guesses(&guesses, "apple", 'a'),
            Some(LetterScore::Correct)
        );
        assert_eq!(
            score_letter_from_guesses(&guesses, "apple", 'l'),
            Some(LetterScore::Present)
        );
        assert_eq!(
            score_letter_from_guesses(&guesses, "apple", 's'),
            Some(LetterScore::Absent)
        );
        assert_eq!(score_letter_from_guesses(&guesses, "apple", 'z'), None);
    }
}
