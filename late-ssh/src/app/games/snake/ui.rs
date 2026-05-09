use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;
use crate::app::games::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
};
use super::state::State;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_sidebar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("score", state.score.to_string(), theme::AMBER_GLOW()),
            ("best", state.best_score.to_string(), theme::SUCCESS()),
            ("level", state.level.to_string(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(vec![
            ("h/l/j/k", "direction"),
            ("p", "pause"),
            ("r", "restart"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: None,
    };

    let board_area = draw_game_frame(frame, area, "Snake", bottom, show_sidebar);
    let board_rect = centered_rect(
        board_area,
        60.min(board_area.width),
        60.min(board_area.height),
    );
    let board = Paragraph::new("hi").alignment(Alignment::Center);
    frame.render_widget(board, board_rect);

    if state.is_paused {
        draw_game_overlay(
            frame,
            board_area,
            "PAUSED",
            "Press p to resume",
            theme::AMBER(),
        );
    } else if state.is_game_over {
        draw_game_overlay(
            frame,
            board_area,
            "GAME OVER",
            "Press r for a fresh run",
            theme::ERROR(),
        );
    }
}


