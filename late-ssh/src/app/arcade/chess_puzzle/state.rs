//! Chess Puzzle: one Lichess puzzle a day per tier. The opponent's setup
//! move is already on the board; the player finds each solution move in
//! turn and the opponent's replies play themselves after a short delay. A
//! wrong move is taken back and costs one of three lives; out of lives, the
//! puzzle is lost and its solution shown. A checkmate always wins, whether
//! or not it is the move the puzzle lists. This file holds the embedded
//! pack, the rules and the per-session state; persistence goes through
//! `svc.rs`.

use std::sync::OnceLock;

use chrono::{NaiveDate, Utc};
use cozy_chess::{Board, Color, GameStatus, Move};
use late_core::{
    chess_puzzle::{ChessPuzzle, Theme, select_for_date},
    models::{
        chess_puzzle::{Game, GameParams},
        chips::Difficulty,
    },
};
use rand_core::{OsRng, RngCore};
use uuid::Uuid;

use crate::{
    app::games::chess_core::{
        cursor, rules,
        types::{ChessColor, ChessPiece},
    },
    metrics::{ArcadeDifficulty, ArcadeFinish, ArcadeMode},
};

use super::svc::ChessPuzzleService;

const DIFFICULTIES: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

/// Wrong moves a puzzle forgives, as Minesweeper's lives.
pub const LIVES: usize = 3;

/// Ticks of the Arcade's 15 fps loop before the opponent's reply lands, so
/// the player sees their own move on the board first.
pub const REPLY_DELAY_TICKS: u8 = 6;

const EASY_PACK: &[u8] = include_bytes!("../../../../assets/chess_puzzles/easy.json");
const MEDIUM_PACK: &[u8] = include_bytes!("../../../../assets/chess_puzzles/medium.json");
const HARD_PACK: &[u8] = include_bytes!("../../../../assets/chess_puzzles/hard.json");

/// A tier's shipped puzzles, parsed once per process. The pack is pinned
/// by `state_test.rs::every_shipped_puzzle_replays_through_the_board`, so a
/// parse failure here is a broken build, never a runtime case.
pub fn pack(difficulty: Difficulty) -> &'static [ChessPuzzle] {
    static EASY: OnceLock<Vec<ChessPuzzle>> = OnceLock::new();
    static MEDIUM: OnceLock<Vec<ChessPuzzle>> = OnceLock::new();
    static HARD: OnceLock<Vec<ChessPuzzle>> = OnceLock::new();
    let (cell, bytes) = match difficulty {
        Difficulty::Easy => (&EASY, EASY_PACK),
        Difficulty::Medium => (&MEDIUM, MEDIUM_PACK),
        Difficulty::Hard => (&HARD, HARD_PACK),
    };
    cell.get_or_init(|| serde_json::from_slice(bytes).expect("embedded chess puzzle pack parses"))
}

/// The pack's moves for a puzzle, setup move first.
pub fn line(puzzle: &ChessPuzzle) -> Vec<Move> {
    puzzle
        .moves
        .iter()
        .map(|mv| mv.parse().expect("embedded chess puzzle move parses"))
        .collect()
}

/// The position the pack's FEN describes, before the setup move.
pub fn start_board(puzzle: &ChessPuzzle) -> Board {
    puzzle
        .fen
        .parse()
        .expect("embedded chess puzzle position parses")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Daily,
    Personal,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Personal => "personal",
        }
    }

    /// Sentence-case name, for the start of a message line.
    fn title(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Personal => "Personal",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResetAction {
    Restart,
    NewPersonal,
}

/// How a puzzle ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Solved,
    Lost,
}

/// A puzzle in play. The puzzle, `ply` and `mistake_plies` persist; the
/// board and its move list are replayed from them, and the reply timer is
/// this session's only.
struct Slot {
    puzzle: &'static ChessPuzzle,
    line: Vec<Move>,
    /// Solution moves on the board after the setup move. The player moves
    /// on even plies, the opponent on odd ones.
    ply: usize,
    /// The ply of each wrong move: one per life spent.
    mistake_plies: Vec<usize>,
    board: Board,
    player: Color,
    /// SAN of every move on the board, the setup move first.
    san: Vec<String>,
    last: Option<(usize, usize)>,
    pending_reply: Option<u8>,
    /// Set once this puzzle's finish is counted, so a restored or replayed
    /// finish never counts twice.
    finish_reported: bool,
}

impl Slot {
    fn new(puzzle: &'static ChessPuzzle, ply: usize, mistake_plies: Vec<usize>) -> Self {
        let line = line(puzzle);
        let mut board = start_board(puzzle);
        let mut san = Vec::with_capacity(line.len());
        let mut last = None;
        let mut player = Color::White;
        for (index, &mv) in line[..=ply].iter().enumerate() {
            san.push(rules::san_label(&board, mv));
            board.play(mv);
            last = Some((mv.from as usize, mv.to as usize));
            if index == 0 {
                player = board.side_to_move();
            }
        }
        let mut slot = Self {
            puzzle,
            line,
            ply,
            mistake_plies,
            board,
            player,
            san,
            last,
            pending_reply: None,
            finish_reported: false,
        };
        slot.finish_reported = slot.outcome().is_some();
        slot
    }

    fn outcome(&self) -> Option<Outcome> {
        if self.ply + 1 == self.line.len() {
            Some(Outcome::Solved)
        } else if self.mistake_plies.len() >= LIVES {
            Some(Outcome::Lost)
        } else {
            None
        }
    }

    fn has_started(&self) -> bool {
        self.ply > 0 || !self.mistake_plies.is_empty()
    }

    /// Play the next move of the line.
    fn play_next(&mut self) {
        let mv = self.line[self.ply + 1];
        self.play(mv);
        self.ply += 1;
    }

    fn play(&mut self, mv: Move) {
        self.san.push(rules::san_label(&self.board, mv));
        self.board.play(mv);
        self.last = Some((mv.from as usize, mv.to as usize));
    }
}

pub struct State {
    user_id: Uuid,
    puzzle_date: NaiveDate,
    mode: Mode,
    selected_difficulty: usize,
    daily: [Slot; 3],
    personal: [Option<Slot>; 3],
    cursor: usize,
    selected: Option<usize>,
    reset_pending: Option<ResetAction>,
    message: String,
    svc: ChessPuzzleService,
}

impl State {
    pub fn new(user_id: Uuid, svc: ChessPuzzleService, saved_games: Vec<Game>) -> Self {
        Self::new_for_date(user_id, svc, Utc::now().date_naive(), saved_games)
    }

    pub(crate) fn new_for_date(
        user_id: Uuid,
        svc: ChessPuzzleService,
        puzzle_date: NaiveDate,
        saved_games: Vec<Game>,
    ) -> Self {
        let daily = DIFFICULTIES.map(|difficulty| {
            let puzzle = daily_puzzle(puzzle_date, difficulty);
            saved_games
                .iter()
                .find(|game| {
                    game.user_id == user_id
                        && game.mode == "daily"
                        && game.difficulty_key == difficulty.key()
                        && game.puzzle_date == Some(puzzle_date)
                        && game.puzzle_id == puzzle.id
                })
                .and_then(|game| slot_from_game(game, difficulty))
                .unwrap_or_else(|| Slot::new(puzzle, 0, Vec::new()))
        });
        let personal = DIFFICULTIES.map(|difficulty| {
            saved_games
                .iter()
                .find(|game| {
                    game.user_id == user_id
                        && game.mode == "personal"
                        && game.difficulty_key == difficulty.key()
                        && game.puzzle_date.is_none()
                })
                .and_then(|game| slot_from_game(game, difficulty))
        });

        let mut state = Self {
            user_id,
            puzzle_date,
            mode: Mode::Daily,
            selected_difficulty: 0,
            daily,
            personal,
            cursor: 0,
            selected: None,
            reset_pending: None,
            message: String::new(),
            svc,
        };
        state.focus_board();
        state
    }

    pub fn difficulty(&self) -> Difficulty {
        DIFFICULTIES[self.selected_difficulty]
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn mode_label(&self) -> &'static str {
        self.mode.as_str()
    }

    pub fn reward_chips(&self) -> Option<i64> {
        (self.mode == Mode::Daily).then(|| self.difficulty().chips())
    }

    pub fn puzzle_date(&self) -> NaiveDate {
        self.puzzle_date
    }

    pub fn puzzle_id(&self) -> &str {
        &self.active().puzzle.id
    }

    pub fn rating(&self) -> u16 {
        self.active().puzzle.rating
    }

    pub fn theme(&self) -> Option<Theme> {
        self.active().puzzle.theme
    }

    pub fn pieces(&self) -> [Option<ChessPiece>; 64] {
        rules::board_pieces(&self.active().board)
    }

    /// The player's colour: the board is drawn from their side.
    pub fn player(&self) -> ChessColor {
        rules::chess_color(self.active().player)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn last_move(&self) -> Option<(usize, usize)> {
        self.active().last
    }

    /// The king in check, if the side to move is.
    pub fn check_square(&self) -> Option<usize> {
        let board = &self.active().board;
        (!board.checkers().is_empty()).then(|| board.king(board.side_to_move()) as usize)
    }

    /// Where the selected piece can go, while it is the player's move.
    pub fn legal_targets(&self) -> Vec<usize> {
        if !self.player_to_move() {
            return Vec::new();
        }
        cursor::legal_targets(&rules::legal_moves(&self.active().board), self.selected)
    }

    /// SAN of the opponent's setup move.
    pub fn setup_san(&self) -> &str {
        &self.active().san[0]
    }

    /// SAN of the solution moves played so far.
    pub fn played_san(&self) -> &[String] {
        &self.active().san[1..]
    }

    /// SAN of the moves still to play, shown once the puzzle is lost.
    pub fn remaining_san(&self) -> Vec<String> {
        let slot = self.active();
        let mut board = slot.board.clone();
        slot.line[slot.ply + 1..]
            .iter()
            .map(|&mv| {
                let san = rules::san_label(&board, mv);
                board.play(mv);
                san
            })
            .collect()
    }

    /// For each player move found so far, whether it was found first try.
    pub fn first_tries(&self) -> Vec<bool> {
        let slot = self.active();
        (0..slot.ply.div_ceil(2))
            .map(|found| !slot.mistake_plies.contains(&(found * 2)))
            .collect()
    }

    pub fn player_moves(&self) -> usize {
        self.active().line.len() / 2
    }

    pub fn mistakes(&self) -> usize {
        self.active().mistake_plies.len()
    }

    pub fn lives_left(&self) -> usize {
        LIVES.saturating_sub(self.mistakes())
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.active().outcome()
    }

    pub fn has_started(&self) -> bool {
        self.active().has_started()
    }

    pub fn reply_pending(&self) -> bool {
        self.active().pending_reply.is_some()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn reset_pending(&self) -> bool {
        self.reset_pending.is_some()
    }

    pub fn first_unfinished_daily(&self) -> Option<usize> {
        self.daily
            .iter()
            .position(|slot| slot.has_started() && slot.outcome().is_none())
    }

    pub fn has_unfinished_daily(&self) -> bool {
        self.puzzle_date == Utc::now().date_naive() && self.first_unfinished_daily().is_some()
    }

    pub fn is_daily_active(&self) -> bool {
        self.mode == Mode::Daily
    }

    pub fn open_daily(&mut self, difficulty_index: usize) {
        self.clear_reset_pending();
        self.mode = Mode::Daily;
        self.selected_difficulty = difficulty_index.min(DIFFICULTIES.len() - 1);
        self.focus_board();
    }

    /// Roll the dailies forward when the UTC date changes under a live
    /// session. Returns true when it moved.
    pub fn ensure_current_daily(&mut self) -> bool {
        let today = Utc::now().date_naive();
        if self.puzzle_date == today {
            return false;
        }
        self.puzzle_date = today;
        self.daily = DIFFICULTIES
            .map(|difficulty| Slot::new(daily_puzzle(today, difficulty), 0, Vec::new()));
        self.clear_reset_pending();
        if self.mode == Mode::Daily {
            self.focus_board();
            self.message = "Today's Chess Puzzle is ready.".to_string();
        }
        true
    }

    pub fn show_daily(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Daily;
        self.focus_board();
    }

    pub fn show_personal(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Personal;
        let dealt = self.ensure_personal();
        self.focus_board();
        if dealt {
            self.save_async();
        }
    }

    pub fn new_personal_board(&mut self) {
        self.clear_reset_pending();
        self.mode = Mode::Personal;
        self.personal[self.selected_difficulty] = Some(random_slot(self.difficulty()));
        self.focus_board();
        self.message = format!("New personal puzzle. {}", self.message);
        self.save_async();
    }

    pub fn next_difficulty(&mut self) {
        self.select_difficulty((self.selected_difficulty + 1) % DIFFICULTIES.len());
    }

    pub fn prev_difficulty(&mut self) {
        self.select_difficulty(
            (self.selected_difficulty + DIFFICULTIES.len() - 1) % DIFFICULTIES.len(),
        );
    }

    fn select_difficulty(&mut self, index: usize) {
        self.clear_reset_pending();
        self.selected_difficulty = index;
        let dealt = self.mode == Mode::Personal && self.ensure_personal();
        self.focus_board();
        if dealt {
            self.save_async();
        }
    }

    /// Two-press restart, personal puzzles only. A daily keeps its moves
    /// and its lives: restarting it would hand back the lives a wrong move
    /// spent, and a lost daily would replay with its solution already shown.
    pub fn request_reset(&mut self) -> bool {
        match self.mode {
            Mode::Daily => {
                self.reset_pending = None;
                self.message =
                    "The daily keeps its moves and lives; p plays a personal puzzle.".to_string();
                false
            }
            Mode::Personal => self.request_action(
                ResetAction::Restart,
                "Press r or 0 again to restart this puzzle.",
            ),
        }
    }

    pub fn request_new_personal(&mut self) -> bool {
        self.request_action(
            ResetAction::NewPersonal,
            "Press n again for a new personal puzzle.",
        )
    }

    pub fn clear_reset_pending(&mut self) {
        if self.reset_pending.take().is_some() {
            self.message = self.board_message();
        }
    }

    /// The personal puzzle from its start, lives restored.
    pub fn reset(&mut self) {
        let puzzle = self.active().puzzle;
        *self.active_mut() = Slot::new(puzzle, 0, Vec::new());
        self.reset_pending = None;
        self.focus_board();
        self.message = format!("Puzzle restarted. {}", self.message);
        self.save_async();
    }

    /// Step the cursor one square on screen; it stops at the edge.
    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        self.clear_reset_pending();
        self.cursor = cursor::move_cursor(self.cursor, self.player(), dx, dy);
    }

    /// `Space`/`Enter` on the cursor's square: put the held piece down
    /// again, move it there, or pick up the player's piece standing there.
    /// The move is tried before the pick-up, so the king onto its own rook
    /// castles.
    pub fn activate(&mut self) {
        self.clear_reset_pending();
        let target = self.cursor;
        if let Some(outcome) = self.outcome() {
            self.message = self.outcome_message(outcome);
            return;
        }
        if self.reply_pending() {
            self.message = format!("Wait for {}'s reply.", self.player().other().label());
            return;
        }
        let own_piece = self
            .pieces()
            .get(target)
            .copied()
            .flatten()
            .is_some_and(|piece| piece.color == self.player());
        let held_move = self
            .selected
            .and_then(|from| rules::legal_move_for(&self.active().board, from, target));
        match (self.selected, held_move) {
            (Some(from), _) if from == target => self.selected = None,
            (Some(_), Some(mv)) => self.attempt(mv),
            (Some(_) | None, None) if own_piece => self.selected = Some(target),
            (Some(_), None) => {
                self.selected = None;
                self.message = "That piece can't go there.".to_string();
            }
            (None, _) => {
                self.message = format!("Pick up one of the {} pieces.", self.player().label());
            }
        }
    }

    /// A left click: the cursor jumps to the square and acts there.
    pub fn click(&mut self, index: usize) {
        if index >= 64 {
            return;
        }
        self.cursor = index;
        self.activate();
    }

    /// The opponent's reply lands once its delay runs out. Driven by the
    /// Arcade tick while this board is open; true when the board changed.
    pub fn tick(&mut self) -> bool {
        let slot = self.active_mut();
        let Some(ticks) = slot.pending_reply else {
            return false;
        };
        if ticks > 1 {
            slot.pending_reply = Some(ticks - 1);
            return false;
        }
        slot.pending_reply = None;
        slot.play_next();
        let reply = slot.san[slot.ply].clone();
        self.message = format!(
            "{} played {reply}. {} to move.",
            self.player().other().label(),
            self.player().label()
        );
        self.save_async();
        true
    }

    /// A legal move by the player: the line's move or any mate goes on,
    /// anything else is taken back and costs a life.
    fn attempt(&mut self, mv: Move) {
        self.selected = None;
        let slot = self.active_mut();
        let expected = slot.line[slot.ply + 1];
        let mut after = slot.board.clone();
        after.play(mv);
        let mates = after.status() == GameStatus::Won;
        if mv != expected && !mates {
            let san = rules::san_label(&slot.board, mv);
            slot.mistake_plies.push(slot.ply);
            let lives = LIVES.saturating_sub(slot.mistake_plies.len());
            if lives == 0 {
                self.finish(Outcome::Lost);
            } else {
                self.message = match lives {
                    1 => format!("{san} is not it. 1 life left."),
                    lives => format!("{san} is not it. {lives} lives left."),
                };
                self.save_async();
            }
            return;
        }

        slot.play(mv);
        slot.ply += 1;
        if mates {
            // Any mate wins: the line ends here even when the puzzle listed
            // a different mating move.
            slot.ply = slot.line.len() - 1;
        }
        if slot.outcome() == Some(Outcome::Solved) {
            self.finish(Outcome::Solved);
            return;
        }
        slot.pending_reply = Some(REPLY_DELAY_TICKS);
        self.message = "Best move! Wait for the reply.".to_string();
        self.save_async();
    }

    /// Count the finish once, bank a solved daily, and save the board.
    fn finish(&mut self, outcome: Outcome) {
        let mode = self.mode;
        let difficulty = self.difficulty();
        let mistakes = self.mistakes();
        let slot = self.active_mut();
        let first_report = !slot.finish_reported;
        slot.finish_reported = true;
        self.message = self.outcome_message(outcome);
        if outcome == Outcome::Solved {
            self.message = match mistakes {
                0 => format!("Solved with no mistakes. {}", self.message),
                1 => format!("Solved with 1 mistake. {}", self.message),
                mistakes => format!("Solved with {mistakes} mistakes. {}", self.message),
            };
        }
        if !first_report {
            self.save_async();
            return;
        }
        let metric_mode = match mode {
            Mode::Daily => ArcadeMode::Daily,
            Mode::Personal => ArcadeMode::Personal,
        };
        let metric_difficulty = match difficulty {
            Difficulty::Easy => ArcadeDifficulty::Easy,
            Difficulty::Medium => ArcadeDifficulty::Medium,
            Difficulty::Hard => ArcadeDifficulty::Hard,
        };
        let finish = match outcome {
            Outcome::Solved => ArcadeFinish::Won,
            Outcome::Lost => ArcadeFinish::Lost,
        };
        self.svc
            .record_finish(metric_mode, metric_difficulty, finish);
        match (outcome, mode) {
            (Outcome::Solved, Mode::Daily) => self.complete_async(difficulty, mistakes as i32),
            (Outcome::Solved, Mode::Personal) | (Outcome::Lost, Mode::Daily | Mode::Personal) => {
                self.save_async()
            }
        }
    }

    /// The board just came up: cursor on the opponent's last move, nothing
    /// held, and the "who is to move" line.
    fn focus_board(&mut self) {
        self.selected = None;
        self.cursor = self.active().last.map_or(0, |(_, to)| to);
        self.message = self.board_message();
    }

    /// What an ended puzzle says whenever it is looked at. A lost one names
    /// its solution here, so it shows with or without the side panel.
    fn outcome_message(&self, outcome: Outcome) -> String {
        match (outcome, self.mode) {
            (Outcome::Solved, Mode::Daily) => "Today's puzzle is banked.".to_string(),
            (Outcome::Solved, Mode::Personal) => {
                "Personal puzzles have no reward; n twice deals another.".to_string()
            }
            (Outcome::Lost, Mode::Daily | Mode::Personal) => {
                format!(
                    "Out of lives. The solution: {}.",
                    self.remaining_san().join(" ")
                )
            }
        }
    }

    /// The one "which board is this" line, whichever key asked for it.
    fn board_message(&self) -> String {
        match self.outcome() {
            Some(outcome) => self.outcome_message(outcome),
            None => format!(
                "{} {} puzzle: {} to move.",
                self.mode.title(),
                self.difficulty().key(),
                self.player().label()
            ),
        }
    }

    fn player_to_move(&self) -> bool {
        let slot = self.active();
        slot.outcome().is_none() && slot.pending_reply.is_none()
    }

    fn save_async(&self) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc.save_game_task(self.game_params());
    }

    fn complete_async(&self, difficulty: Difficulty, mistakes: i32) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        self.svc
            .complete_game_task(self.game_params(), difficulty, self.puzzle_date, mistakes);
    }

    fn game_params(&self) -> GameParams {
        let slot = self.active();
        GameParams {
            user_id: self.user_id,
            mode: self.mode.as_str().to_string(),
            difficulty_key: self.difficulty().key().to_string(),
            puzzle_date: (self.mode == Mode::Daily).then_some(self.puzzle_date),
            puzzle_id: slot.puzzle.id.clone(),
            ply: slot.ply as i32,
            mistake_plies: slot.mistake_plies.iter().map(|&ply| ply as i32).collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn set_puzzle_for_test(
        &mut self,
        difficulty: Difficulty,
        puzzle: &'static ChessPuzzle,
    ) {
        self.selected_difficulty = DIFFICULTIES
            .iter()
            .position(|candidate| *candidate == difficulty)
            .expect("known difficulty");
        if self.mode == Mode::Personal {
            self.ensure_personal();
        }
        *self.active_mut() = Slot::new(puzzle, 0, Vec::new());
        self.focus_board();
    }

    fn request_action(&mut self, action: ResetAction, message: &str) -> bool {
        if self.reset_pending == Some(action) {
            self.reset_pending = None;
            return true;
        }
        self.reset_pending = Some(action);
        self.message = message.to_string();
        false
    }

    fn ensure_personal(&mut self) -> bool {
        let index = self.selected_difficulty;
        if self.personal[index].is_some() {
            return false;
        }
        self.personal[index] = Some(random_slot(self.difficulty()));
        true
    }

    fn active(&self) -> &Slot {
        let index = self.selected_difficulty;
        match self.mode {
            Mode::Daily => &self.daily[index],
            Mode::Personal => self.personal[index]
                .as_ref()
                .expect("personal puzzle is dealt before activation"),
        }
    }

    fn active_mut(&mut self) -> &mut Slot {
        let index = self.selected_difficulty;
        match self.mode {
            Mode::Daily => &mut self.daily[index],
            Mode::Personal => self.personal[index]
                .as_mut()
                .expect("personal puzzle is dealt before activation"),
        }
    }
}

/// The day's puzzle in a tier.
pub fn daily_puzzle(date: NaiveDate, difficulty: Difficulty) -> &'static ChessPuzzle {
    select_for_date(pack(difficulty), date, difficulty)
}

fn random_slot(difficulty: Difficulty) -> Slot {
    let puzzles = pack(difficulty);
    let index = (OsRng.next_u64() % puzzles.len() as u64) as usize;
    Slot::new(&puzzles[index], 0, Vec::new())
}

/// A saved row, or `None` when it does not fit its puzzle: an id the pack
/// no longer has, a ply past the line, more mistakes than lives, or a
/// mistake on a ply the player never had. A row saved between the player's
/// move and the opponent's reply gets the reply played at once.
fn slot_from_game(game: &Game, difficulty: Difficulty) -> Option<Slot> {
    let puzzle = pack(difficulty)
        .iter()
        .find(|puzzle| puzzle.id == game.puzzle_id)?;
    let ply = usize::try_from(game.ply)
        .ok()
        .filter(|&ply| ply < puzzle.moves.len())?;
    let mistake_plies = game
        .mistake_plies
        .iter()
        .map(|&mistake| {
            usize::try_from(mistake)
                .ok()
                .filter(|&mistake| mistake <= ply && mistake % 2 == 0)
        })
        .collect::<Option<Vec<usize>>>()?;
    if mistake_plies.len() > LIVES {
        return None;
    }
    let mut slot = Slot::new(puzzle, ply, mistake_plies);
    if ply % 2 == 1 && slot.outcome().is_none() {
        slot.play_next();
    }
    Some(slot)
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
