use ratatui::layout::Rect;

use super::{handle_arrow, handle_key, handle_mouse};
use crate::app::arcade::solitaire::state::State;
use crate::app::arcade::solitaire::state::state_test::almost_won_state;
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, MouseModifiers};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

fn mouse(kind: MouseEventKind, button: Option<MouseButton>) -> MouseEvent {
    MouseEvent {
        kind,
        button,
        x: 30,
        y: 12,
        modifiers: MouseModifiers::default(),
    }
}

fn cascading() -> State {
    let mut state = almost_won_state();
    state.auto_move();
    assert!(state.win_cascade_running());
    state
}

#[tokio::test]
async fn a_key_press_stops_the_cascade() {
    let mut state = cascading();
    assert!(handle_key(&mut state, b'j'));
    assert!(!state.win_cascade_running());
}

#[tokio::test]
async fn an_arrow_stops_the_cascade() {
    let mut state = cascading();
    assert!(handle_arrow(&mut state, b'B'));
    assert!(!state.win_cascade_running());
}

#[tokio::test]
async fn a_click_stops_the_cascade() {
    let mut state = cascading();
    assert!(handle_mouse(
        &mut state,
        AREA,
        mouse(MouseEventKind::Down, Some(MouseButton::Left))
    ));
    assert!(!state.win_cascade_running());
}

/// Everything else the mouse emits is noise the player did not mean as
/// "enough": a pointer resting on the board, or a wheel nudge, must not cut
/// the finish short.
#[tokio::test]
async fn scrolling_and_drifting_leave_the_cascade_alone() {
    for kind in [
        MouseEventKind::Moved,
        MouseEventKind::Drag,
        MouseEventKind::Up,
        MouseEventKind::ScrollUp,
        MouseEventKind::ScrollDown,
    ] {
        let mut state = cascading();
        handle_mouse(&mut state, AREA, mouse(kind, None));
        assert!(state.win_cascade_running(), "{kind:?} stopped the cascade",);
    }
}
