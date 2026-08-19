use super::*;
use crate::app::games::chess_core::types::ChessColor;
use crate::app::lobby::daily::svc::{DailyChessColors, DailyChessState, DailyMoveRecord};
use ratatui::{Terminal, backend::TestBackend};

/// A rail-only detail: the info rail reads the move history and nothing else,
/// so the board itself stays empty.
fn detail_with_moves(pairs: usize) -> ChessDetail {
    let mut move_history = Vec::new();
    for number in 1..=pairs {
        for side in ["w", "b"] {
            move_history.push(DailyMoveRecord {
                from: 0,
                to: 0,
                label: format!("{side}{number}"),
                at: Utc::now(),
            });
        }
    }
    ChessDetail {
        state: DailyChessState {
            version: 1,
            revision: 0,
            fen: String::new(),
            colors: DailyChessColors {
                white: Uuid::from_u128(1),
                black: Uuid::from_u128(2),
            },
            move_history,
            position_history: Vec::new(),
        },
        pieces: [const { None }; 64],
        legal_moves: Vec::new(),
        turn: ChessColor::White,
        in_check: false,
    }
}

fn rail_text(game: DailyGame, detail: &ChessDetail, height: u16) -> String {
    let backend = TestBackend::new(INFO_SIDEBAR_WIDTH, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_info_rail(frame, frame.area(), game, detail))
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

#[test]
fn a_wrapping_tagline_never_clips_the_newest_move_pair() {
    // The chess960 tagline is wider than the rail, so it renders as two rows;
    // budgeting the move list against logical header lines would overflow the
    // rail by exactly that extra row and drop the newest pair off the bottom.
    let detail = detail_with_moves(6);
    let text = rail_text(DailyGame::Chess960, &detail, 10);

    assert!(
        text.contains("w6"),
        "newest move pair fell off the bottom of the rail:\n{text}"
    );
}
