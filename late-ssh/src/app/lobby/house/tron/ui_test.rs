use super::*;
use crate::app::lobby::house::tron::{state::BOARD_CELLS, svc::TronPlayerSnapshot};

fn blank_snapshot() -> TronSnapshot {
    TronSnapshot {
        room_id: Uuid::nil(),
        seats: [None; SEAT_COUNT],
        board: [None; BOARD_CELLS],
        pickups: [None; BOARD_CELLS],
        players: [TronPlayerSnapshot {
            head: None,
            direction: Direction::Right,
            alive: false,
            crashed: false,
            boost_ticks: 0,
            phase_charges: 0,
            gap_moves: 0,
        }; SEAT_COUNT],
        phase: TronPhase::Waiting,
        outcome: None,
        status_message: "test".to_string(),
        speed_label: "standard".to_string(),
        mode_label: "classic".to_string(),
    }
}

#[test]
fn board_lines_have_uniform_width() {
    let snapshot = blank_snapshot();
    for cell_w in [1u16, 2] {
        let lines = board_lines(&snapshot, cell_w);
        assert_eq!(lines.len(), BOARD_HEIGHT);
        for line in &lines {
            let width: usize = line
                .spans
                .iter()
                .map(|span| span.content.chars().count())
                .sum();
            assert_eq!(width, BOARD_WIDTH * cell_w as usize);
        }
    }
}

#[test]
fn plan_prefers_widest_grid_that_fits() {
    assert_eq!(plan(40), (false, 1));
    assert_eq!(plan(70), (false, 1));
    assert_eq!(plan(86), (true, 1));
    assert_eq!(plan(114), (false, 2));
    assert_eq!(plan(142), (true, 2));
}
