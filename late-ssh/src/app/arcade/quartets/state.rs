//! Quartets: one wall a day for everyone, sixteen words hiding four
//! quartets, four words to a link. A try is four picked words. A quartet
//! locks in with its link; a miss is a strike and says how the four split
//! across the quartets (three and one, two pairs, one pair, or all apart).
//! Three strikes end the wall unsolved. Once three quartets are in, the
//! last four lock in by themselves. One board a day, one tier.

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::NaiveDate;
use late_core::{daily, models::quartets::GameParams};
use serde::Deserialize;
use uuid::Uuid;

use super::svc::{QuartetsService, Today};
use crate::app::arcade::daily_load::{DailyLoad, Polled};
use crate::metrics::{ArcadeDifficulty, ArcadeFinish, ArcadeMode};

const PACK: &[u8] = include_bytes!("../../../../assets/quartets/puzzles.json");
static PUZZLES: OnceLock<Vec<Puzzle>> = OnceLock::new();

/// Quartets on a wall, and words in a quartet.
pub const QUARTETS: usize = 4;
/// Words on a wall.
pub const WORDS: usize = QUARTETS * QUARTETS;
/// The misses that end a wall.
pub const MAX_STRIKES: usize = 3;
const UNAVAILABLE: &str = "Quartets is unavailable. Try again soon.";
const READY: &str = "Find the four quartets: four words that share a link.";

/// One wall as the pack stores it: four quartets of four words.
#[derive(Debug, Deserialize)]
pub struct Puzzle {
    /// Stable across pack edits; the daily claim stores it.
    pub id: i32,
    pub quartets: [Quartet; QUARTETS],
}

#[derive(Debug, Deserialize)]
pub struct Quartet {
    pub link: String,
    pub words: [String; QUARTETS],
}

impl Puzzle {
    /// Word `index`: quartet `index / 4`, its `index % 4`th word.
    pub fn word(&self, index: u8) -> &str {
        &self.quartets[usize::from(quartet_of(index))].words[usize::from(index) % QUARTETS]
    }

    pub fn link(&self, quartet: u8) -> &str {
        &self.quartets[usize::from(quartet)].link
    }
}

/// The quartet word `index` belongs to.
pub fn quartet_of(index: u8) -> u8 {
    index / QUARTETS as u8
}

/// The embedded pack, by id, parsed once per process.
pub fn pack() -> &'static [Puzzle] {
    PUZZLES.get_or_init(|| serde_json::from_slice(PACK).expect("the quartets pack parses"))
}

pub fn find_puzzle(id: i32) -> Option<&'static Puzzle> {
    let puzzles = pack();
    puzzles
        .binary_search_by_key(&id, |puzzle| puzzle.id)
        .ok()
        .map(|index| &puzzles[index])
}

/// The puzzle to claim for `date`, by the shared daily pick
/// (`daily::choose_for_date`) keyed on the id and seeded by the date.
/// `last_used` maps a puzzle id to the last day it was claimed.
pub fn choose_for_date<'a>(
    pack: &'a [Puzzle],
    last_used: &HashMap<i32, NaiveDate>,
    date: NaiveDate,
) -> &'a Puzzle {
    daily::choose_for_date(
        pack,
        |puzzle| &puzzle.id,
        last_used,
        &format!("late-sh-quartets-daily-v1:{date}"),
    )
    .expect("the quartets pack is not empty")
}

/// The wall everyone starts the day on: the sixteen words dealt by a hash
/// of the date and the puzzle.
pub fn opening_wall(date: NaiveDate, puzzle_id: i32) -> [u8; WORDS] {
    let mut wall: [u8; WORDS] = std::array::from_fn(|index| index as u8);
    daily::shuffle(
        &mut wall,
        daily::fnv1a(&format!("late-sh-quartets-wall-v1:{date}:{puzzle_id}")),
    );
    wall
}

/// How a stored list of tries left the wall, replayed in order as
/// `State::restore` does: a try is a quartet or a strike, three quartets
/// lock the last, three strikes end it. The saving session wrote the list
/// whole, so every try is four distinct words; a malformed chunk is skipped.
pub fn stored_outcome(tries: &[i16]) -> Outcome {
    let mut found = 0;
    let mut strikes = 0;
    for chunk in tries.chunks(QUARTETS) {
        let Some(tried) = words_of(chunk) else {
            continue;
        };
        if Split::of(tried) == Split::Quartet {
            found += 1;
        } else {
            strikes += 1;
        }
        if found == QUARTETS - 1 {
            return Outcome::Solved;
        }
        if strikes == MAX_STRIKES {
            return Outcome::OutOfStrikes;
        }
    }
    Outcome::Playing
}

fn words_of(chunk: &[i16]) -> Option<[u8; QUARTETS]> {
    let chunk: [i16; QUARTETS] = chunk.try_into().ok()?;
    let mut tried = [0u8; QUARTETS];
    for (slot, word) in tried.iter_mut().zip(chunk) {
        *slot = u8::try_from(word).ok()?;
    }
    Some(tried)
}

/// How the four words of a try fall across the quartets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    /// All four share a link.
    Quartet,
    ThreeOne,
    TwoTwo,
    TwoOneOne,
    /// No two share a link.
    Apart,
}

impl Split {
    pub fn of(tried: [u8; QUARTETS]) -> Split {
        let mut counts = [0u8; QUARTETS];
        for word in tried {
            counts[usize::from(quartet_of(word))] += 1;
        }
        counts.sort_unstable_by(|a, b| b.cmp(a));
        match counts {
            [4, 0, 0, 0] => Split::Quartet,
            [3, 1, 0, 0] => Split::ThreeOne,
            [2, 2, 0, 0] => Split::TwoTwo,
            [2, 1, 1, 0] => Split::TwoOneOne,
            [1, 1, 1, 1] => Split::Apart,
            other => unreachable!("four words cannot split as {other:?}"),
        }
    }

    /// What the try said, as the message and the rules put it.
    pub const fn text(self) -> &'static str {
        match self {
            Split::Quartet => "All four share a link.",
            Split::ThreeOne => "Three of those share a link, one does not.",
            Split::TwoTwo => "Those split into two pairs.",
            Split::TwoOneOne => "Only two of those share a link.",
            Split::Apart => "No two of those share a link.",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    Solved,
    OutOfStrikes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Up,
    Down,
    Left,
    Right,
}

pub struct State {
    user_id: Uuid,
    svc: QuartetsService,
    puzzle_date: NaiveDate,
    /// `None` while today's wall is loading or could not be loaded.
    puzzle: Option<&'static Puzzle>,
    /// The words still on the wall, as word indices in the order drawn.
    wall: Vec<u8>,
    /// The quartets locked in, in the order found.
    found: Vec<u8>,
    /// Every try in the order made, each four word indices ascending.
    tries: Vec<[u8; QUARTETS]>,
    /// The words picked for the next try, at most four.
    picked: Vec<u8>,
    /// A position in `wall`.
    cursor: usize,
    shuffles: u64,
    show_rules: bool,
    message: String,
    /// Today's wall while the board has none; see `poll_load`.
    load: DailyLoad<Today>,
}

impl State {
    /// A session's board from its connect-time load; `None` (the load
    /// failed) shows the board unavailable and retries from `poll_load`.
    pub fn new(user_id: Uuid, svc: QuartetsService, today: Option<Today>) -> Self {
        let puzzle_date = match &today {
            Some(today) => today.daily.puzzle_date,
            None => svc.today(),
        };
        let mut state = Self {
            user_id,
            svc,
            puzzle_date,
            puzzle: None,
            wall: Vec::new(),
            found: Vec::new(),
            tries: Vec::new(),
            picked: Vec::new(),
            cursor: 0,
            shuffles: 0,
            show_rules: false,
            message: UNAVAILABLE.to_string(),
            load: DailyLoad::after_connect(today.is_some()),
        };
        if let Some(today) = today {
            state.install(today);
        }
        state
    }

    /// Put a day's wall on the board with the saved tries that belong to it.
    fn install(&mut self, today: Today) {
        let Today { daily, saved } = today;
        self.puzzle_date = daily.puzzle_date;
        self.puzzle = Some(daily.puzzle);
        self.wall = opening_wall(daily.puzzle_date, daily.puzzle.id).to_vec();
        self.found.clear();
        self.tries.clear();
        self.picked.clear();
        self.cursor = 0;
        self.shuffles = 0;
        self.message = READY.to_string();
        if let Some(game) = saved
            && game.puzzle_date == daily.puzzle_date
            && game.puzzle_id == daily.puzzle.id
        {
            self.restore(&game.tries);
        }
    }

    /// Replay saved tries. One that is not four distinct words still on
    /// the wall, or repeats an earlier try, is dropped, and the replay
    /// stops where the wall ended. A wall that ended here was banked when
    /// it ended.
    fn restore(&mut self, saved: &[i16]) {
        for chunk in saved.chunks(QUARTETS) {
            if self.outcome() != Outcome::Playing {
                break;
            }
            if let Some(tried) = self.saved_try(chunk) {
                self.take_try(tried);
            }
        }
        self.message = match self.outcome() {
            Outcome::Playing if self.tries.is_empty() => READY,
            Outcome::Playing => "Keep going.",
            Outcome::Solved => "Solved. A new wall comes at midnight UTC.",
            Outcome::OutOfStrikes => "Out of strikes. A new wall comes at midnight UTC.",
        }
        .to_string();
    }

    fn saved_try(&self, chunk: &[i16]) -> Option<[u8; QUARTETS]> {
        let mut tried = words_of(chunk)?;
        tried.sort_unstable();
        let distinct = tried.windows(2).all(|pair| pair[0] < pair[1]);
        let on_wall = tried.iter().all(|word| self.wall.contains(word));
        (distinct && on_wall && !self.tries.contains(&tried)).then_some(tried)
    }

    /// Roll the wall over when the UTC date changes under a live session:
    /// yesterday's wall comes off at once, and `poll_load` fetches today's.
    /// Returns true when the screen changed.
    pub fn ensure_current_daily(&mut self) -> bool {
        if self.puzzle.is_none() || self.puzzle_date == self.svc.today() {
            return false;
        }
        // Yesterday's wall must stop taking tries right away, whether or
        // not today's loads.
        self.puzzle = None;
        self.wall.clear();
        self.found.clear();
        self.tries.clear();
        self.picked.clear();
        self.cursor = 0;
        self.message = "Loading today's Quartets.".to_string();
        true
    }

    /// Load today's wall and saved tries whenever the board has no wall (a
    /// failed connect-time load, or the rollover), and install what lands.
    /// Runs on every tick, board open or not. Returns true when the screen
    /// changed.
    pub fn poll_load(&mut self) -> bool {
        let (svc, user_id) = (&self.svc, self.user_id);
        match self
            .load
            .poll(self.puzzle.is_none(), || svc.load_today_task(user_id))
        {
            Polled::Waiting => false,
            Polled::Loaded(today) => {
                self.install(today);
                true
            }
            Polled::Failed => {
                self.message = "Quartets is unavailable. Retrying soon.".to_string();
                true
            }
        }
    }

    /// Today's wall has a try on it and has not ended.
    pub fn has_unfinished_daily(&self) -> bool {
        self.puzzle.is_some()
            && !self.tries.is_empty()
            && self.outcome() == Outcome::Playing
            && self.puzzle_date == self.svc.today()
    }

    pub fn puzzle_date(&self) -> NaiveDate {
        self.puzzle_date
    }

    pub fn puzzle(&self) -> Option<&'static Puzzle> {
        self.puzzle
    }

    pub fn is_loaded(&self) -> bool {
        self.puzzle.is_some()
    }

    pub fn wall(&self) -> &[u8] {
        &self.wall
    }

    pub fn found(&self) -> &[u8] {
        &self.found
    }

    pub fn tries(&self) -> &[[u8; QUARTETS]] {
        &self.tries
    }

    pub fn is_picked(&self, word: u8) -> bool {
        self.picked.contains(&word)
    }

    pub fn picked_count(&self) -> usize {
        self.picked.len()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn strikes(&self) -> usize {
        self.tries
            .iter()
            .filter(|tried| Split::of(**tried) != Split::Quartet)
            .count()
    }

    pub fn outcome(&self) -> Outcome {
        if self.found.len() == QUARTETS {
            Outcome::Solved
        } else if self.strikes() >= MAX_STRIKES {
            Outcome::OutOfStrikes
        } else {
            Outcome::Playing
        }
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

    pub fn move_cursor(&mut self, step: Step) -> bool {
        if self.outcome() != Outcome::Playing || self.wall.is_empty() {
            return false;
        }
        let rows = self.wall.len() / QUARTETS;
        let (row, column) = (self.cursor / QUARTETS, self.cursor % QUARTETS);
        let (row, column) = match step {
            Step::Up => (row.saturating_sub(1), column),
            Step::Down => ((row + 1).min(rows - 1), column),
            Step::Left => (row, column.saturating_sub(1)),
            Step::Right => (row, (column + 1).min(QUARTETS - 1)),
        };
        self.cursor = row * QUARTETS + column;
        true
    }

    pub fn toggle_at_cursor(&mut self) -> bool {
        self.toggle(self.cursor)
    }

    /// Put the cursor on the word at `position` on the wall and pick or
    /// unpick it.
    pub fn click(&mut self, position: usize) -> bool {
        if self.outcome() != Outcome::Playing || position >= self.wall.len() {
            return false;
        }
        self.cursor = position;
        self.toggle(position)
    }

    fn toggle(&mut self, position: usize) -> bool {
        if self.outcome() != Outcome::Playing {
            return false;
        }
        let Some(&word) = self.wall.get(position) else {
            return false;
        };
        match self.picked.iter().position(|picked| *picked == word) {
            Some(index) => {
                self.picked.remove(index);
            }
            None if self.picked.len() < QUARTETS => self.picked.push(word),
            None => {
                self.message = "Four are picked: Enter tries them, x clears.".to_string();
            }
        }
        true
    }

    pub fn clear_picks(&mut self) -> bool {
        if self.picked.is_empty() {
            return false;
        }
        self.picked.clear();
        true
    }

    /// Deal the words left on the wall into a new order. The order is the
    /// session's alone, so it is drawn from a counter, not a clock.
    pub fn shuffle(&mut self) -> bool {
        if self.outcome() != Outcome::Playing || self.wall.is_empty() {
            return false;
        }
        self.shuffles += 1;
        daily::shuffle(&mut self.wall, self.shuffles);
        true
    }

    /// Try the four picked words. A quartet locks in; a miss is a strike
    /// and names its split. The try is saved, and a wall that ends here is
    /// counted, and banked when solved.
    pub fn submit(&mut self) -> bool {
        let Some(puzzle) = self.puzzle else {
            self.message = UNAVAILABLE.to_string();
            return true;
        };
        if self.outcome() != Outcome::Playing {
            return false;
        }
        let Ok(mut tried) = <[u8; QUARTETS]>::try_from(self.picked.as_slice()) else {
            self.message = format!("Pick four words, then Enter: {} picked.", self.picked.len());
            return true;
        };
        tried.sort_unstable();
        if self.tries.contains(&tried) {
            self.message = "Those four were tried already. No strike.".to_string();
            return true;
        }
        self.picked.clear();
        let split = self.take_try(tried);
        self.message = self.try_message(puzzle, split, tried);
        match self.outcome() {
            Outcome::Playing => self.save_async(puzzle),
            Outcome::Solved => {
                self.svc.record_finish(
                    ArcadeMode::Daily,
                    ArcadeDifficulty::Single,
                    ArcadeFinish::Won,
                );
                self.complete_async(puzzle);
            }
            Outcome::OutOfStrikes => {
                self.svc.record_finish(
                    ArcadeMode::Daily,
                    ArcadeDifficulty::Single,
                    ArcadeFinish::Lost,
                );
                self.save_async(puzzle);
            }
        }
        true
    }

    /// Count a try against the wall: a quartet locks in, and with three in
    /// the last four lock in too.
    fn take_try(&mut self, tried: [u8; QUARTETS]) -> Split {
        let split = Split::of(tried);
        self.tries.push(tried);
        if split == Split::Quartet {
            self.lock(quartet_of(tried[0]));
            if self.found.len() == QUARTETS - 1 {
                let last = (0..QUARTETS as u8)
                    .find(|quartet| !self.found.contains(quartet))
                    .expect("one quartet is left");
                self.lock(last);
            }
        }
        split
    }

    fn lock(&mut self, quartet: u8) {
        self.found.push(quartet);
        self.wall.retain(|word| quartet_of(*word) != quartet);
        self.picked.retain(|word| quartet_of(*word) != quartet);
        self.cursor = self.cursor.min(self.wall.len().saturating_sub(1));
    }

    fn try_message(&self, puzzle: &Puzzle, split: Split, tried: [u8; QUARTETS]) -> String {
        match split {
            Split::Quartet => {
                let link = puzzle.link(quartet_of(tried[0]));
                match (self.outcome(), self.found.last()) {
                    (Outcome::Solved, Some(last)) => format!(
                        "Quartet: {link}. The last four: {}. Solved!",
                        puzzle.link(*last)
                    ),
                    _ => format!("Quartet: {link}."),
                }
            }
            Split::ThreeOne | Split::TwoTwo | Split::TwoOneOne | Split::Apart => {
                match self.outcome() {
                    Outcome::OutOfStrikes => {
                        format!("{} Third strike: here are the quartets.", split.text())
                    }
                    Outcome::Playing | Outcome::Solved => format!(
                        "{} Strike {} of {MAX_STRIKES}.",
                        split.text(),
                        self.strikes()
                    ),
                }
            }
        }
    }

    // Pure state tests run without a runtime, and the save worker is a
    // spawned task; they assert on the state and the svc tests on the rows.
    fn complete_async(&self, puzzle: &Puzzle) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc
            .complete_game_task(self.params(puzzle), self.strikes() as i32);
    }

    fn save_async(&self, puzzle: &Puzzle) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc.save_game_task(self.params(puzzle));
    }

    fn params(&self, puzzle: &Puzzle) -> GameParams {
        GameParams {
            user_id: self.user_id,
            puzzle_date: self.puzzle_date,
            puzzle_id: puzzle.id,
            tries: self
                .tries
                .iter()
                .flatten()
                .map(|word| i16::from(*word))
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
