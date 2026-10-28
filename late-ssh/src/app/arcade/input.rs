use late_core::models::leaderboard::{ArcadeGame, DailyPuzzle, ScoreGame};
use ratatui::layout::Rect;

use crate::app::arcade::share::{self, ShareCard, ShareCardKind, ShareFormat};
use crate::app::common::primitives::{Banner, Screen};
use crate::app::help_modal::data::HelpTopic;
use crate::app::state::App;

/// Where `selection` sits in `order`, the lobby order. Every game is in it:
/// `ArcadeGame::all()` is generated from the rosters.
fn lobby_position(order: &[ArcadeGame], selection: ArcadeGame) -> usize {
    order
        .iter()
        .position(|game| *game == selection)
        .expect("every arcade game is in the lobby")
}

fn next_lobby_selection(selection: ArcadeGame) -> ArcadeGame {
    let order: Vec<ArcadeGame> = ArcadeGame::all().collect();
    let position = lobby_position(&order, selection);
    order[(position + 1) % order.len()]
}

fn prev_lobby_selection(selection: ArcadeGame) -> ArcadeGame {
    let order: Vec<ArcadeGame> = ArcadeGame::all().collect();
    let position = lobby_position(&order, selection);
    order[(position + order.len() - 1) % order.len()]
}

/// Esc, `q` and `Q` close a board, except where the game types letters.
fn closes_board(byte: u8) -> bool {
    matches!(byte, 0x1B | b'q' | b'Q')
}

pub fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.is_playing_game {
        // Backtick hops the workspace cycle out of daily puzzles. Real-time
        // games (Lateris, Snake, Traffic) and personal (non-daily) boards
        // are not stops and keep the byte for themselves.
        if byte == b'`' && crate::app::workspace::arcade::active_daily_stop(app).is_some() {
            return crate::app::workspace::cycle::cycle_game_workspace(app);
        }
        // A finished daily offers its share card on `s`. Only then: while a
        // board is open the byte is the game's (a Le Word letter).
        if byte == b's'
            && let Some((kind, card)) = active_daily_card(app)
        {
            share_card(app, kind, &card);
            return true;
        }
        return match app.game_selection {
            ArcadeGame::Puzzle(puzzle) => handle_puzzle_key(app, puzzle, byte),
            ArcadeGame::Score(game) => handle_score_key(app, game, byte),
        };
    }

    // Lobby mode. The solvers roster owns the lobby while it is open: Esc
    // or `o` closes it, j/k move it to the next card, nothing else gets
    // through (Enter must not launch the game behind it).
    if app.arcade_solvers_open {
        match byte {
            0x1B | b'o' | b'O' => app.arcade_solvers_open = false,
            b'j' | b'J' => app.game_selection = next_lobby_selection(app.game_selection),
            b'k' | b'K' => app.game_selection = prev_lobby_selection(app.game_selection),
            _ => {}
        }
        return true;
    }
    if byte == b's' {
        let card = day_card(app);
        share_card(app, ShareCardKind::Day, &card);
        return true;
    }
    match byte {
        b'j' | b'J' => {
            app.game_selection = next_lobby_selection(app.game_selection);
            true
        }
        b'k' | b'K' => {
            app.game_selection = prev_lobby_selection(app.game_selection);
            true
        }
        b'o' | b'O' if selected_daily_is_available(app) => {
            app.arcade_solvers_open = true;
            true
        }
        b'\r' | b'\n' => {
            launch_selected(app);
            true
        }
        _ => false,
    }
}

/// Keys on an open daily board.
fn handle_puzzle_key(app: &mut App, puzzle: DailyPuzzle, byte: u8) -> bool {
    match puzzle {
        DailyPuzzle::LeWord => {
            if byte == b'?' {
                app.le_word_state.close_rules();
                open_global_help(app);
                return true;
            }
            // Le Word is a text-entry game where `q`/`Q` are valid letters,
            // so only `Esc` exits to the lobby; `q`/`Q` fall through to the
            // letter handler below.
            if byte == 0x1B && !app.le_word_state.show_rules {
                app.is_playing_game = false;
                return true;
            }
            super::le_word::input::handle_key(&mut app.le_word_state, byte)
        }
        DailyPuzzle::RubiksCube => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            app.rubiks_cube_state.ensure_current_daily();
            super::rubiks_cube::input::handle_key(&mut app.rubiks_cube_state, byte)
        }
        DailyPuzzle::SlidingPuzzle => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::sliding_puzzle::input::handle_key(&mut app.sliding_puzzle_state, byte)
        }
        DailyPuzzle::Sudoku => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::sudoku::input::handle_key(&mut app.sudoku_state, byte)
        }
        DailyPuzzle::Nonogram => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::nonogram::input::handle_key(&mut app.nonogram_state, byte)
        }
        DailyPuzzle::Minesweeper => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::minesweeper::input::handle_key(&mut app.minesweeper_state, byte)
        }
        DailyPuzzle::Solitaire => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::solitaire::input::handle_key(&mut app.solitaire_state, byte)
        }
        DailyPuzzle::Netwalk => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::netwalk::input::handle_key(&mut app.netwalk_state, byte)
        }
        DailyPuzzle::ChessPuzzle => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::chess_puzzle::input::handle_key(&mut app.chess_puzzle_state, byte)
        }
        DailyPuzzle::WordWheel => {
            if byte == b'?' {
                app.word_wheel_state.close_rules();
                open_global_help(app);
                return true;
            }
            // Word Wheel types letters, `q` among them, so only `Esc`
            // leaves, as in Le Word.
            if byte == 0x1B && !app.word_wheel_state.show_rules() {
                app.is_playing_game = false;
                return true;
            }
            super::word_wheel::input::handle_key(&mut app.word_wheel_state, byte)
        }
        DailyPuzzle::Quartets => {
            if closes_board(byte) && !app.quartets_state.show_rules() {
                app.is_playing_game = false;
                return true;
            }
            super::quartets::input::handle_key(&mut app.quartets_state, byte)
        }
    }
}

/// Keys on an open score-game run.
fn handle_score_key(app: &mut App, game: ScoreGame, byte: u8) -> bool {
    match game {
        ScoreGame::TwentyFortyEight => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            if byte == b'`' {
                // Personal board: backtick jumps to the dashboard (the
                // bottom-bar "dashboard" hint) rather than cycling stops.
                app.is_playing_game = false;
                app.set_screen(Screen::Dashboard);
                return true;
            }
            super::twenty_forty_eight::input::handle_key(&mut app.twenty_forty_eight_state, byte)
        }
        ScoreGame::Lateris => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::tetris::input::handle_key(&mut app.tetris_state, byte)
        }
        ScoreGame::Snake => {
            if closes_board(byte) {
                app.snake_state.persist_progress();
                app.is_playing_game = false;
                return true;
            }
            super::snake::input::handle_key(&mut app.snake_state, byte)
        }
        ScoreGame::Traffic => {
            if closes_board(byte) {
                app.is_playing_game = false;
                return true;
            }
            super::traffic::input::handle_key(&mut app.traffic_state, byte)
        }
    }
}

/// Enter on a lobby card opens its board, unless it is a daily that cannot
/// be played today.
fn launch_selected(app: &mut App) {
    if let ArcadeGame::Puzzle(puzzle) = app.game_selection {
        if !super::ui::daily_available(puzzle, &app.nonogram_state) {
            return;
        }
        match puzzle {
            // Sudoku generates its boards off-thread on first open.
            DailyPuzzle::Sudoku => app.sudoku_state.ensure_loaded(),
            DailyPuzzle::LeWord
            | DailyPuzzle::RubiksCube
            | DailyPuzzle::SlidingPuzzle
            | DailyPuzzle::Nonogram
            | DailyPuzzle::Minesweeper
            | DailyPuzzle::Solitaire
            | DailyPuzzle::Netwalk
            | DailyPuzzle::ChessPuzzle
            | DailyPuzzle::WordWheel
            | DailyPuzzle::Quartets => {}
        }
    }
    app.is_playing_game = true;
}

/// The selected card is a daily that can be played today, so it has a
/// roster to show.
fn selected_daily_is_available(app: &App) -> bool {
    match app.game_selection {
        ArcadeGame::Puzzle(puzzle) => super::ui::daily_available(puzzle, &app.nonogram_state),
        ArcadeGame::Score(_) => false,
    }
}

/// Whether Esc on the Arcade page belongs to this domain: an open board, or
/// the lobby's solvers roster. `app/input.rs` asks before treating Esc as
/// the page's.
pub(crate) fn claims_escape(app: &App) -> bool {
    app.is_playing_game || app.arcade_solvers_open
}

/// The share card of the daily on screen, once it is finished.
fn active_daily_card(app: &App) -> Option<(ShareCardKind, ShareCard)> {
    match app.game_selection {
        ArcadeGame::Puzzle(puzzle) => {
            daily_share(app, puzzle).map(|card| (ShareCardKind::Daily(puzzle), card))
        }
        ArcadeGame::Score(_) => None,
    }
}

/// A daily's share card from its game's pure `share.rs`: `None` until the
/// board is finished, and always on a personal board.
fn daily_share(app: &App, puzzle: DailyPuzzle) -> Option<ShareCard> {
    match puzzle {
        DailyPuzzle::LeWord => super::le_word::share::from_state(&app.le_word_state),
        DailyPuzzle::RubiksCube => super::rubiks_cube::share::from_state(&app.rubiks_cube_state),
        DailyPuzzle::SlidingPuzzle => {
            super::sliding_puzzle::share::from_state(&app.sliding_puzzle_state)
        }
        DailyPuzzle::Sudoku => super::sudoku::share::from_state(&app.sudoku_state),
        DailyPuzzle::Nonogram => super::nonogram::share::from_state(&app.nonogram_state),
        DailyPuzzle::Minesweeper => super::minesweeper::share::from_state(&app.minesweeper_state),
        DailyPuzzle::Solitaire => super::solitaire::share::from_state(&app.solitaire_state),
        DailyPuzzle::Netwalk => super::netwalk::share::from_state(&app.netwalk_state),
        DailyPuzzle::ChessPuzzle => super::chess_puzzle::share::from_state(&app.chess_puzzle_state),
        DailyPuzzle::WordWheel => super::word_wheel::share::from_state(&app.word_wheel_state),
        DailyPuzzle::Quartets => super::quartets::share::from_state(&app.quartets_state),
    }
}

/// Today's day card: the leaderboard snapshot's completion marks OR'd
/// with this session's own wins, so a puzzle solved a moment ago counts.
pub(crate) fn day_card(app: &App) -> ShareCard {
    let snapshot = app.leaderboard.user_daily_statuses.get(&app.user_id);
    let session = app.session_daily_wins.today();
    let streak = app.quest_state.snapshot().daily_streak.consecutive_days;
    share::day_card(
        chrono::Utc::now().date_naive(),
        |puzzle| {
            snapshot.is_some_and(|s| s.completed(puzzle))
                || session.is_some_and(|s| s.completed(puzzle))
        },
        streak,
    )
}

/// Copy a rendered card to the clipboard, then count it. The one place a
/// card leaves the session, so the banner and the metric live here.
fn share_card(app: &mut App, kind: ShareCardKind, card: &ShareCard) {
    app.pending_clipboard = Some(share::render(card, ShareFormat::Emoji));
    app.banner = Some(Banner::success("Card copied. Paste it anywhere."));
    crate::metrics::record_share_card(kind);
}

fn open_global_help(app: &mut App) {
    app.help_modal_state
        .set_keep_composer_focused(app.profile_state.profile().keep_composer_focused);
    app.help_modal_state.open(HelpTopic::Pair);
    app.show_help = true;
}

pub fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.is_playing_game {
        return match app.game_selection {
            ArcadeGame::Puzzle(puzzle) => handle_puzzle_arrow(app, puzzle, key),
            ArcadeGame::Score(game) => handle_score_arrow(app, game, key),
        };
    }

    // Lobby mode
    match key {
        b'A' => {
            // Up
            app.game_selection = prev_lobby_selection(app.game_selection);
            true
        }
        b'B' => {
            // Down
            app.game_selection = next_lobby_selection(app.game_selection);
            true
        }
        _ => false,
    }
}

fn handle_puzzle_arrow(app: &mut App, puzzle: DailyPuzzle, key: u8) -> bool {
    match puzzle {
        DailyPuzzle::LeWord => super::le_word::input::handle_arrow(&mut app.le_word_state, key),
        DailyPuzzle::RubiksCube => {
            app.rubiks_cube_state.ensure_current_daily();
            super::rubiks_cube::input::handle_arrow(&mut app.rubiks_cube_state, key)
        }
        DailyPuzzle::SlidingPuzzle => {
            super::sliding_puzzle::input::handle_arrow(&mut app.sliding_puzzle_state, key)
        }
        DailyPuzzle::Sudoku => super::sudoku::input::handle_arrow(&mut app.sudoku_state, key),
        DailyPuzzle::Nonogram => super::nonogram::input::handle_arrow(&mut app.nonogram_state, key),
        DailyPuzzle::Minesweeper => {
            super::minesweeper::input::handle_arrow(&mut app.minesweeper_state, key)
        }
        DailyPuzzle::Solitaire => {
            super::solitaire::input::handle_arrow(&mut app.solitaire_state, key)
        }
        DailyPuzzle::Netwalk => super::netwalk::input::handle_arrow(&mut app.netwalk_state, key),
        DailyPuzzle::ChessPuzzle => {
            super::chess_puzzle::input::handle_arrow(&mut app.chess_puzzle_state, key)
        }
        DailyPuzzle::WordWheel => {
            super::word_wheel::input::handle_arrow(&mut app.word_wheel_state, key)
        }
        DailyPuzzle::Quartets => super::quartets::input::handle_arrow(&mut app.quartets_state, key),
    }
}

fn handle_score_arrow(app: &mut App, game: ScoreGame, key: u8) -> bool {
    match game {
        ScoreGame::TwentyFortyEight => {
            super::twenty_forty_eight::input::handle_arrow(&mut app.twenty_forty_eight_state, key)
        }
        ScoreGame::Lateris => super::tetris::input::handle_arrow(&mut app.tetris_state, key),
        ScoreGame::Snake => super::snake::input::handle_arrow(&mut app.snake_state, key),
        ScoreGame::Traffic => super::traffic::input::handle_arrow(&mut app.traffic_state, key),
    }
}

pub(crate) fn handle_event(app: &mut App, event: &crate::app::input::ParsedInput) -> bool {
    let crate::app::input::ParsedInput::Mouse(mouse) = event else {
        return false;
    };

    let area = arcade_content_area(app);
    match app.game_selection {
        ArcadeGame::Puzzle(DailyPuzzle::SlidingPuzzle) => {
            super::sliding_puzzle::input::handle_mouse(&mut app.sliding_puzzle_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::LeWord) => {
            super::le_word::input::handle_mouse(&mut app.le_word_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::Solitaire) => {
            super::solitaire::input::handle_mouse(&mut app.solitaire_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::Minesweeper) => {
            super::minesweeper::input::handle_mouse(&mut app.minesweeper_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::Netwalk) => {
            super::netwalk::input::handle_mouse(&mut app.netwalk_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::ChessPuzzle) => {
            super::chess_puzzle::input::handle_mouse(&mut app.chess_puzzle_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::WordWheel) => {
            super::word_wheel::input::handle_mouse(&mut app.word_wheel_state, area, *mouse)
        }
        ArcadeGame::Puzzle(DailyPuzzle::Quartets) => {
            super::quartets::input::handle_mouse(&mut app.quartets_state, area, *mouse)
        }
        ArcadeGame::Puzzle(
            DailyPuzzle::RubiksCube | DailyPuzzle::Sudoku | DailyPuzzle::Nonogram,
        ) => false,
        // The score games are keyboard-only.
        ArcadeGame::Score(_) => false,
    }
}

fn arcade_content_area(app: &App) -> Rect {
    // Mouse coordinates must land on the same cells the draw path used, so
    // this reads the shared definition rather than re-deriving the layout.
    app.content_area()
}

#[cfg(test)]
#[path = "input_test.rs"]
mod input_test;
