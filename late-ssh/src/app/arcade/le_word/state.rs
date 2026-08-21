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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Daily,
    Replay,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Replay => "replay",
        }
    }
}

#[derive(Clone, Debug)]
struct Snapshot {
    puzzle_date: Option<NaiveDate>,
    answer: String,
    guesses: Vec<String>,
    current_guess: String,
    is_game_over: bool,
    won: bool,
}

pub struct State {
    pub user_id: Uuid,
    pub mode: Mode,
    pub puzzle_date: Option<NaiveDate>,
    pub answer: String,
    pub daily_word_loaded: bool,
    pub guesses: Vec<String>,
    pub current_guess: String,
    pub is_game_over: bool,
    pub won: bool,
    pub show_rules: bool,
    pub reset_pending: bool,
    pub message: String,
    daily_snapshot: Option<Snapshot>,
    replay_snapshot: Option<Snapshot>,
    pub svc: LeWordService,
}

impl State {
    pub fn new(
        user_id: Uuid,
        svc: LeWordService,
        daily_word: Option<DailyWord>,
        saved_games: Vec<Game>,
    ) -> Self {
        let daily_snapshot = daily_word.map(|word| {
            saved_games
                .iter()
                .find(|game| {
                    game.mode == "daily"
                        && game.puzzle_date == Some(word.puzzle_date)
                        && game.answer_word == word.answer_word
                })
                .map(snapshot_from_game)
                .unwrap_or_else(|| fresh_snapshot(Some(word.puzzle_date), word.answer_word))
        });
        let replay_snapshot = saved_games
            .iter()
            .find(|game| game.mode == "replay" && game.puzzle_date.is_none())
            .map(snapshot_from_game);
        let daily_word_loaded = daily_snapshot.is_some();
        let mut state = Self {
            user_id,
            mode: Mode::Daily,
            puzzle_date: None,
            answer: String::new(),
            daily_word_loaded,
            guesses: Vec::new(),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
            show_rules: false,
            reset_pending: false,
            message: String::new(),
            daily_snapshot,
            replay_snapshot,
            svc,
        };
        state.load_mode_snapshot();
        state
    }

    /// Today's word has at least one submitted guess and the run is not over.
    pub fn has_unfinished_daily(&self) -> bool {
        if !self.daily_word_loaded {
            return false;
        }
        let (guesses, is_game_over, puzzle_date) = if self.mode == Mode::Daily {
            (&self.guesses, self.is_game_over, self.puzzle_date)
        } else if let Some(snapshot) = &self.daily_snapshot {
            (
                &snapshot.guesses,
                snapshot.is_game_over,
                snapshot.puzzle_date,
            )
        } else {
            return false;
        };
        !guesses.is_empty() && !is_game_over && puzzle_date == Some(self.svc.today())
    }

    pub fn guess_number(&self) -> usize {
        self.guesses
            .len()
            .saturating_add((!self.is_game_over) as usize)
    }

    pub fn is_daily_active(&self) -> bool {
        self.mode == Mode::Daily
    }

    pub fn show_daily(&mut self) {
        self.clear_reset_pending();
        if self.mode == Mode::Daily {
            return;
        }
        self.store_active_snapshot();
        self.save_async();
        self.mode = Mode::Daily;
        self.load_mode_snapshot();
    }

    pub fn show_replay(&mut self) {
        self.clear_reset_pending();
        if self.mode == Mode::Replay {
            return;
        }
        self.store_active_snapshot();
        self.save_async();
        self.mode = Mode::Replay;
        let replay_conflicts_with_daily = self
            .replay_snapshot
            .as_ref()
            .zip(self.daily_snapshot.as_ref())
            .is_some_and(|(replay, daily)| replay.answer == daily.answer);
        if self.replay_snapshot.is_none() || replay_conflicts_with_daily {
            self.replay_snapshot = Some(fresh_snapshot(None, self.next_replay_answer()));
        }
        self.load_mode_snapshot();
        self.save_async();
    }

    pub fn new_replay(&mut self) {
        self.clear_reset_pending();
        if self.mode == Mode::Daily {
            self.store_active_snapshot();
            self.save_async();
        }
        let snapshot = fresh_snapshot(None, self.next_replay_answer());
        self.replay_snapshot = Some(snapshot.clone());
        self.mode = Mode::Replay;
        self.apply_snapshot(snapshot);
        self.save_async();
    }

    pub fn request_replay_reset(&mut self) -> bool {
        if self.reset_pending {
            self.reset_pending = false;
            return true;
        }
        self.reset_pending = true;
        self.message = "Press 0 again for a new random word.".to_string();
        false
    }

    pub fn clear_reset_pending(&mut self) {
        if std::mem::take(&mut self.reset_pending) {
            self.message.clear();
        }
    }

    pub fn submit_guess(&mut self) -> bool {
        self.clear_reset_pending();
        if self.answer.is_empty() {
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
            self.store_active_snapshot();
            self.save_async();
            if self.mode == Mode::Daily
                && let Some(puzzle_date) = self.puzzle_date
            {
                self.svc
                    .record_win_task(self.user_id, puzzle_date, self.guesses.len());
            }
            return true;
        }

        if self.guesses.len() >= MAX_GUESSES {
            self.is_game_over = true;
            self.message = format!("The word was {}.", self.answer.to_uppercase());
        } else {
            self.message = "Try again.".to_string();
        }
        self.store_active_snapshot();
        self.save_async();
        true
    }

    pub fn push_letter(&mut self, ch: char) -> bool {
        self.clear_reset_pending();
        if self.answer.is_empty()
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
        self.clear_reset_pending();
        if self.answer.is_empty() || self.is_game_over {
            return false;
        }
        let changed = self.current_guess.pop().is_some();
        if changed {
            self.message.clear();
        }
        changed
    }

    pub fn scores_for_guess(&self, guess: &str) -> [LetterScore; WORD_LEN] {
        score_guess(guess, &self.answer)
    }

    pub fn score_for_keyboard_letter(&self, letter: char) -> Option<LetterScore> {
        score_letter_from_guesses(&self.guesses, &self.answer, letter)
    }

    pub fn open_rules(&mut self) {
        self.clear_reset_pending();
        self.show_rules = true;
    }

    pub fn close_rules(&mut self) {
        self.show_rules = false;
    }

    fn load_mode_snapshot(&mut self) {
        let snapshot = match self.mode {
            Mode::Daily => self.daily_snapshot.clone(),
            Mode::Replay => self.replay_snapshot.clone(),
        };
        if let Some(snapshot) = snapshot {
            self.apply_snapshot(snapshot);
        } else {
            self.puzzle_date = None;
            self.answer.clear();
            self.guesses.clear();
            self.current_guess.clear();
            self.is_game_over = false;
            self.won = false;
            self.message = "Le Word is unavailable. Try again soon.".to_string();
        }
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        self.puzzle_date = snapshot.puzzle_date;
        self.answer = snapshot.answer;
        self.guesses = snapshot.guesses;
        self.current_guess = snapshot.current_guess;
        self.is_game_over = snapshot.is_game_over;
        self.won = snapshot.won;
        self.message = snapshot_message(self.mode, self);
    }

    fn store_active_snapshot(&mut self) {
        if self.answer.is_empty() {
            return;
        }
        let snapshot = snapshot_from_state(self);
        match self.mode {
            Mode::Daily => self.daily_snapshot = Some(snapshot),
            Mode::Replay => self.replay_snapshot = Some(snapshot),
        }
    }

    fn next_replay_answer(&self) -> String {
        let daily_answer = self
            .daily_snapshot
            .as_ref()
            .map(|snapshot| snapshot.answer.as_str());
        self.svc
            .replay_answer(&self.answer, daily_answer)
            .to_string()
    }

    fn save_async(&self) {
        if self.answer.is_empty() {
            return;
        }
        self.svc.save_game_task(GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            puzzle_date: self.puzzle_date,
            answer_word: self.answer.clone(),
            guesses: serde_json::to_value(&self.guesses).unwrap_or_default(),
            current_guess: self.current_guess.clone(),
            is_game_over: self.is_game_over,
            won: self.won,
        });
    }
}

fn fresh_snapshot(puzzle_date: Option<NaiveDate>, answer: String) -> Snapshot {
    Snapshot {
        puzzle_date,
        answer,
        guesses: Vec::new(),
        current_guess: String::new(),
        is_game_over: false,
        won: false,
    }
}

fn snapshot_from_game(game: &Game) -> Snapshot {
    Snapshot {
        puzzle_date: game.puzzle_date,
        answer: game.answer_word.clone(),
        guesses: serde_json::from_value(game.guesses.clone()).unwrap_or_default(),
        current_guess: game.current_guess.clone(),
        is_game_over: game.is_game_over,
        won: game.won,
    }
}

fn snapshot_from_state(state: &State) -> Snapshot {
    Snapshot {
        puzzle_date: state.puzzle_date,
        answer: state.answer.clone(),
        guesses: state.guesses.clone(),
        current_guess: state.current_guess.clone(),
        is_game_over: state.is_game_over,
        won: state.won,
    }
}

fn snapshot_message(mode: Mode, state: &State) -> String {
    if state.won {
        format!("Solved in {}.", state.guesses.len())
    } else if state.is_game_over {
        format!("The word was {}.", state.answer.to_uppercase())
    } else if state.guesses.is_empty() && state.current_guess.is_empty() {
        match mode {
            Mode::Daily => "Guess today's Le Word.".to_string(),
            Mode::Replay => "Guess a random Le Word.".to_string(),
        }
    } else {
        "Keep going.".to_string()
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
