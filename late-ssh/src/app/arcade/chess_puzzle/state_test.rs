use std::collections::HashSet;

use chrono::{Duration, Utc};
use late_core::{
    chess_puzzle::rating_range,
    db::{Db, DbConfig},
};
use tokio::sync::broadcast;

use super::*;
use crate::app::activity::event::ActivityEvent;

fn service() -> ChessPuzzleService {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    ChessPuzzleService::new(db, activity)
}

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 28).expect("valid date")
}

fn pack_puzzle(difficulty: Difficulty, id: &str) -> &'static ChessPuzzle {
    find_puzzle(difficulty, id).expect("puzzle in the pack")
}

/// A day's claim as the service would hand it over: one pack puzzle per
/// tier.
fn daily(puzzle_date: NaiveDate) -> Daily {
    Daily {
        puzzle_date,
        puzzles: [
            pack_puzzle(Difficulty::Easy, "3WAR5"),
            pack_puzzle(Difficulty::Medium, "uHqD7"),
            pack_puzzle(Difficulty::Hard, "IYCVq"),
        ],
    }
}

/// Easy `02LIr`: after White's Kg2, Black mates in two with Qf2+ Kh3
/// Qxh2#. Squares used below: b7 49, b6 41, c5 34, f2 13, h2 15.
fn mate_in_two() -> &'static ChessPuzzle {
    pack_puzzle(Difficulty::Easy, "02LIr")
}

/// Both rooks mate; the pack names Ra8#, so Rb8# is a mate off the line.
fn two_mates() -> &'static ChessPuzzle {
    Box::leak(Box::new(ChessPuzzle {
        id: "two-mates".to_string(),
        fen: "7k/p5pp/8/8/8/8/8/RR4K1 b - - 0 1".to_string(),
        moves: vec!["a7a6".to_string(), "a1a8".to_string()],
        rating: 1000,
        theme: Some(Theme::MateIn1),
    }))
}

fn saved(
    user_id: Uuid,
    mode: &str,
    difficulty: Difficulty,
    puzzle_date: Option<NaiveDate>,
    puzzle_id: &str,
    ply: i32,
    mistake_plies: Vec<i32>,
) -> Game {
    Game {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        user_id,
        mode: mode.to_string(),
        difficulty_key: difficulty.key().to_string(),
        puzzle_date,
        puzzle_id: puzzle_id.to_string(),
        ply,
        mistake_plies,
        mate_move: None,
    }
}

/// The pack validator: every shipped puzzle sits in its tier's rating band
/// under a unique id, its FEN parses, every move is legal in turn, and
/// every move the player has to find is exactly what the board makes of
/// its from/to squares, so a click can play it (a castle as king onto
/// rook, a promotion as the queen).
#[test]
fn every_shipped_puzzle_replays_through_the_board() {
    for difficulty in DIFFICULTIES {
        let puzzles = pack(difficulty);
        assert_eq!(puzzles.len(), 2000, "{}", difficulty.key());
        let (low, high) = rating_range(difficulty);
        let mut ids = HashSet::new();
        for puzzle in puzzles {
            assert!(ids.insert(puzzle.id.as_str()), "duplicate {}", puzzle.id);
            assert!(
                (low..=high).contains(&puzzle.rating),
                "{} is rated {}",
                puzzle.id,
                puzzle.rating
            );
            let moves = line(puzzle);
            assert!(
                moves.len().is_multiple_of(2) && (2..=8).contains(&moves.len()),
                "{} has {} moves",
                puzzle.id,
                moves.len()
            );
            let mut board = start_board(puzzle);
            for (index, &mv) in moves.iter().enumerate() {
                if index % 2 == 1 {
                    assert_eq!(
                        rules::legal_move_for(&board, mv.from as usize, mv.to as usize),
                        Some(mv),
                        "{} move {index} cannot be clicked",
                        puzzle.id
                    );
                }
                assert!(
                    board.try_play(mv).is_ok(),
                    "{} move {index} is illegal",
                    puzzle.id
                );
            }
        }
    }
}

/// A whole puzzle: a wrong move is taken back and costs a life, the right
/// one waits for the reply, the reply lands on the tick, and the last move
/// solves it with one move found first try and one not.
#[test]
fn a_puzzle_played_through_with_one_mistake() {
    let mut state = State::new(Uuid::now_v7(), service(), Some(daily(date())), Vec::new());
    state.set_puzzle_for_test(Difficulty::Easy, mate_in_two());
    assert_eq!(
        (
            state.player(),
            state.setup_san(),
            state.cursor(),
            state.message()
        ),
        (
            ChessColor::Black,
            "Kg2",
            14,
            "Daily easy puzzle: Black to move."
        )
    );

    state.click(49);
    assert_eq!(state.selected(), Some(49));
    state.click(41);
    assert_eq!(
        (state.selected(), state.mistakes(), state.message()),
        (None, 1, "b6 is not it. 2 lives left.")
    );

    state.click(34);
    state.click(13);
    assert_eq!(
        (state.played_san(), state.reply_pending(), state.message()),
        (
            &["Qf2+".to_string()][..],
            true,
            "Best move! Wait for the reply."
        )
    );
    state.click(15);
    assert_eq!(state.message(), "Wait for White's reply.");
    let ticks: Vec<bool> = (0..REPLY_DELAY_TICKS).map(|_| state.tick()).collect();
    assert_eq!(ticks, [false, false, false, false, false, true]);
    assert_eq!(state.message(), "White played Kh3. Black to move.");

    state.click(13);
    state.click(15);
    assert_eq!(
        (
            state.played_san(),
            state.first_tries(),
            state.mistakes(),
            state.lives_left(),
            state.outcome(),
            state.reply_pending(),
            state.last_move(),
            state.message(),
        ),
        (
            &["Qf2+".to_string(), "Kh3".to_string(), "Qxh2#".to_string()][..],
            vec![false, true],
            1,
            2,
            Some(Outcome::Solved),
            false,
            Some((13, 15)),
            "Solved with 1 mistake. Today's puzzle is banked."
        )
    );
    assert!(!state.tick());
}

/// A mate the puzzle does not list still wins, as the one move the player
/// made: the card counts one move, nothing remains to show, the row keeps
/// the mate, and a restore from that row ends on the player's mate, not
/// the pack's.
#[test]
fn any_mate_wins_even_one_the_puzzle_does_not_list() {
    let puzzle = two_mates();
    let user_id = Uuid::now_v7();
    let mut state = State::new(user_id, service(), Some(daily(date())), Vec::new());
    state.set_puzzle_for_test(Difficulty::Easy, puzzle);

    state.click(1);
    state.click(57);
    assert_eq!(
        (
            state.played_san(),
            state.first_tries(),
            state.remaining_san(),
            state.mistakes(),
            state.outcome(),
            state.message()
        ),
        (
            &["Rb8#".to_string()][..],
            vec![true],
            Vec::<String>::new(),
            0,
            Some(Outcome::Solved),
            "Solved with no mistakes. Today's puzzle is banked."
        )
    );
    let row = state.game_params().expect("a board to save");
    assert_eq!((row.ply, row.mate_move.as_deref()), (0, Some("b1b8")));

    let restored = slot_from_row(
        puzzle,
        &Game {
            mate_move: row.mate_move.clone(),
            ..saved(user_id, "daily", Difficulty::Easy, Some(date()), "two-mates", 0, vec![])
        },
    )
    .expect("the row fits its puzzle");
    assert_eq!(
        (restored.san.clone(), restored.outcome(), restored.found()),
        (
            vec!["a6".to_string(), "Rb8#".to_string()],
            Some(Outcome::Solved),
            1
        )
    );

    // A mate move that does not mate from the row's position is a corrupt
    // row, as is one on the opponent's ply.
    for (ply, mate_move) in [(0, "b1b7"), (0, "a1a7"), (1, "b1b8")] {
        let row = Game {
            mate_move: Some(mate_move.to_string()),
            ..saved(user_id, "daily", Difficulty::Easy, Some(date()), "two-mates", ply, vec![])
        };
        assert!(slot_from_row(puzzle, &row).is_none(), "{ply} {mate_move}");
    }
}

/// Three wrong moves lose: the board stops taking moves, the solution is
/// named, the daily is no longer unfinished, and it cannot be restarted.
#[test]
fn three_wrong_moves_lose_and_show_the_solution() {
    let mut state = State::new(Uuid::now_v7(), service(), Some(daily(date())), Vec::new());
    state.set_puzzle_for_test(Difficulty::Easy, mate_in_two());
    for _ in 0..LIVES {
        state.click(49);
        state.click(41);
    }
    let lost = "Out of lives. The solution: Qf2+ Kh3 Qxh2#.";
    assert_eq!(
        (
            state.outcome(),
            state.lives_left(),
            state.remaining_san(),
            state.message()
        ),
        (
            Some(Outcome::Lost),
            0,
            vec!["Qf2+".to_string(), "Kh3".to_string(), "Qxh2#".to_string()],
            lost
        )
    );

    state.click(34);
    assert_eq!(
        (state.selected(), state.legal_targets(), state.message()),
        (None, Vec::new(), lost)
    );
    assert!(!state.request_reset());
    assert_eq!(state.first_unfinished_daily(), None);
}

/// Saved rows come back as they were left: a mistake keeps its life
/// spent, a row saved before the opponent's reply gets the reply, a daily
/// row from another puzzle is ignored, and the personal slot restores. A
/// row that does not fit its puzzle is never restored.
#[test]
fn saved_rows_restore_and_bad_rows_are_dealt_fresh() {
    let user_id = Uuid::now_v7();
    let rows = vec![
        saved(
            user_id,
            "daily",
            Difficulty::Easy,
            Some(date()),
            "3WAR5",
            0,
            vec![0],
        ),
        saved(
            user_id,
            "daily",
            Difficulty::Medium,
            Some(date()),
            "02LIr",
            2,
            vec![],
        ),
        saved(
            user_id,
            "daily",
            Difficulty::Hard,
            Some(date()),
            "IYCVq",
            1,
            vec![],
        ),
        saved(
            user_id,
            "personal",
            Difficulty::Easy,
            None,
            "02LIr",
            2,
            vec![0],
        ),
    ];
    let mut state = State::new(user_id, service(), Some(daily(date())), rows);

    let mut dailies = Vec::new();
    for index in 0..3 {
        state.open_daily(index);
        dailies.push((
            state.puzzle_id().map(str::to_string),
            state.played_san().len(),
            state.mistakes(),
            state.reply_pending(),
        ));
    }
    assert_eq!(
        dailies,
        [
            (Some("3WAR5".to_string()), 0, 1, false),
            (Some("uHqD7".to_string()), 0, 0, false),
            (Some("IYCVq".to_string()), 2, 0, false),
        ]
    );
    assert_eq!(state.first_unfinished_daily(), Some(0));

    state.open_daily(0);
    state.show_personal();
    assert_eq!(
        (
            state.puzzle_id(),
            state.played_san().len(),
            state.mistakes()
        ),
        (Some("02LIr"), 2, 1)
    );

    for (ply, mistakes) in [
        (0, vec![1]),
        (4, vec![]),
        (2, vec![0, 0, 2, 2]),
        (-1, vec![]),
    ] {
        let row = saved(
            user_id,
            "personal",
            Difficulty::Easy,
            None,
            "02LIr",
            ply,
            mistakes,
        );
        assert!(slot_from_game(&row, Difficulty::Easy).is_none(), "{row:?}");
    }
    let unknown = saved(
        user_id,
        "personal",
        Difficulty::Easy,
        None,
        "nope",
        0,
        vec![],
    );
    assert!(slot_from_game(&unknown, Difficulty::Easy).is_none());
}

/// A session that came up without the day's claim has no daily board: the
/// tip line says so, moves do nothing, and a personal puzzle still deals.
#[test]
fn without_the_days_claim_the_daily_boards_are_unavailable() {
    let mut state = State::new(Uuid::now_v7(), service(), None, Vec::new());
    assert_eq!(
        (
            state.is_loaded(),
            state.puzzle_id(),
            state.pieces().iter().all(Option::is_none),
            state.message()
        ),
        (false, None, true, "Chess Puzzle is unavailable. Try again soon.")
    );
    state.click(12);
    assert_eq!(
        (state.selected(), state.message()),
        (None, "Chess Puzzle is unavailable. Try again soon.")
    );
    assert!(!state.has_unfinished_daily());

    state.show_personal();
    assert!(state.is_loaded());
    assert!(state.player_moves() > 0);
}

/// A session left open past UTC midnight clears its dailies at once, asks
/// for the new day's puzzles, backs off a failed fetch, and leaves the
/// personal puzzles alone.
#[test]
fn the_utc_rollover_clears_the_dailies_and_backs_off_a_failed_fetch() {
    let user_id = Uuid::now_v7();
    let yesterday = Utc::now().date_naive() - Duration::days(1);
    let mut state = State::new(
        user_id,
        service(),
        Some(daily(yesterday)),
        vec![saved(
            user_id,
            "personal",
            Difficulty::Easy,
            None,
            "02LIr",
            2,
            vec![],
        )],
    );
    assert!(state.is_loaded());

    assert!(state.ensure_current_daily());
    assert_eq!(
        (
            state.is_loaded(),
            state.puzzle_date(),
            state.first_unfinished_daily(),
            state.message()
        ),
        (false, yesterday, None, "Loading today's Chess Puzzle.")
    );
    assert!(!state.ensure_current_daily());
    assert!(state.poll_reload());
    assert_eq!(state.message(), "Chess Puzzle is unavailable. Retrying soon.");
    assert!(!state.ensure_current_daily());

    state.show_personal();
    assert_eq!((state.puzzle_id(), state.played_san().len()), (Some("02LIr"), 2));
}
