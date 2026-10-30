use ratatui::{Terminal, backend::TestBackend};

use super::draw_game;
use crate::app::arcade::solitaire::state::State;
use crate::app::arcade::solitaire::state::state_test::almost_won_state;

fn rendered(state: &State) -> String {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    terminal
        .draw(|frame| draw_game(frame, frame.area(), state, true))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            text.push_str(buffer[(x, y)].symbol());
        }
        text.push('\n');
    }
    text
}

fn ink_cells(text: &str) -> usize {
    text.chars().filter(|ch| !ch.is_whitespace()).count()
}

/// Winning throws cards across the board: cells the finished board left
/// blank end up painted, and they stay painted.
#[tokio::test]
async fn the_cascade_paints_over_the_board() {
    let mut state = almost_won_state();
    state.auto_move();
    let settled = ink_cells(&rendered(&state));

    // The first draw is what tells the cascade how big the board is, so the
    // ticks that matter come after it.
    for _ in 0..30 {
        state.tick_win_animation();
    }
    let mid_flight = ink_cells(&rendered(&state));
    assert!(
        mid_flight > settled,
        "the cascade painted nothing: {settled} -> {mid_flight}"
    );
}

/// The win card waits for the last card to land; until then the cascade has
/// the board to itself.
#[tokio::test]
async fn the_win_card_waits_for_the_cascade() {
    let mut state = almost_won_state();
    state.auto_move();
    let flying = rendered(&state);
    assert!(!flying.contains("YOU WON!"), "{flying}");

    state.skip_win_animation();
    let landed = rendered(&state);
    assert!(landed.contains("YOU WON!"), "{landed}");
}

/// Cells drawn as the cursor: the amber fill `block_style` gives the focused
/// card or slot.
fn cursor_cells(state: &State) -> usize {
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("terminal");
    terminal
        .draw(|frame| draw_game(frame, frame.area(), state, true))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    buffer
        .content()
        .iter()
        .filter(|cell| {
            cell.fg == crate::app::common::theme::AMBER()
                && cell.modifier.contains(ratatui::style::Modifier::REVERSED)
        })
        .count()
}

/// The cursor fill would sit under the bouncing cards, so it hides while the
/// cascade plays and comes back once the cards have landed.
#[tokio::test]
async fn the_cursor_hides_while_the_cascade_plays() {
    let mut state = almost_won_state();
    assert!(cursor_cells(&state) > 0, "no cursor before the win");

    state.auto_move();
    assert!(state.win_cascade_running());
    assert_eq!(cursor_cells(&state), 0);
}
