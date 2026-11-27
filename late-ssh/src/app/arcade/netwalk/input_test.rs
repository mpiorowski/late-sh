use late_core::db::{Db, DbConfig};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::*;
use crate::app::activity::event::ActivityEvent;
use crate::app::arcade::netwalk::state::{LENT_DIFFICULTY, Mode, rotate};
use crate::app::arcade::netwalk::svc::NetwalkService;

fn lent_board() -> State {
    let db = Db::new(&DbConfig::default()).expect("inert test pool");
    let (activity, _) = broadcast::channel::<ActivityEvent>(8);
    let mut state = State::new(
        Uuid::now_v7(),
        NetwalkService::new(db, activity),
        Vec::new(),
    );
    state.open_lent_daily();
    state
}

/// The keys the tour lends with the board move, turn and lock; the keys
/// that would swap the board out are refused and change nothing.
#[test]
fn the_lent_keys_play_the_board_and_never_leave_it() {
    let mut state = lent_board();
    let before = state.board().to_vec();

    for byte in *b"npd[]qs`" {
        assert!(!handle_lent_key(&mut state, byte), "{}", byte as char);
    }
    assert_eq!(state.mode(), Mode::Daily);
    assert_eq!(state.difficulty(), LENT_DIFFICULTY);
    assert_eq!(state.board(), before);
    assert_eq!(state.moves(), 0);

    // Walk to a tile that looks different turned (the source can be a
    // cross, the same every way round), then lock it, free it, turn it
    // each way.
    let dimension = state.dimension();
    let target = before
        .iter()
        .position(|mask| rotate(*mask, Turn::Clockwise) != *mask)
        .expect("a scrambled board has a tile that turns");
    let (row, column) = (target / dimension, target % dimension);
    let (cursor_row, cursor_column) = (state.cursor() / dimension, state.cursor() % dimension);
    for _ in cursor_row..row {
        assert!(handle_lent_key(&mut state, b'j'));
    }
    for _ in row..cursor_row {
        assert!(handle_lent_key(&mut state, b'k'));
    }
    for _ in cursor_column..column {
        assert!(handle_lent_key(&mut state, b'l'));
    }
    for _ in column..cursor_column {
        assert!(handle_lent_key(&mut state, b'h'));
    }
    assert_eq!(state.cursor(), target);
    assert!(handle_lent_key(&mut state, b'f'));
    assert!(state.is_locked(target));
    assert!(handle_lent_key(&mut state, b'f'));
    assert!(!state.is_locked(target));
    assert!(handle_lent_key(&mut state, b' '));
    assert_ne!(state.board(), before);
    assert!(handle_lent_key(&mut state, b'x'));
    assert_eq!(state.board(), before, "a turn each way puts the tile back");
    assert_eq!(state.moves(), 2);
}
