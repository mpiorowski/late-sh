use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;
use crate::app::games::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
};
use super::state::{State, ThingOnScreen, CobraState};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_sidebar: bool) {
    
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("score", state.score.to_string(), theme::AMBER_GLOW()),
            ("best", state.best_score.to_string(), theme::SUCCESS()),
            ("level", state.level.to_string(), theme::TEXT_BRIGHT()),
            ("lives left", state.cobra.lives.to_string(), theme::TEXT_BRIGHT()),
            ("tick", state.field_tick.to_string(), theme::TEXT_BRIGHT()),
            ("stutter_left", state.stutter_left.to_string(), theme::TEXT_BRIGHT()),
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
        state.field.width as u16 + 4,
        state.field.height as u16 + 4,
    );
    let field = Paragraph::new(get_field_lines(state)).alignment(Alignment::Center);
    frame.render_widget(field, board_rect);

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
    } else if let CobraState::Dead = state.cobra.state {
        draw_game_overlay(
            frame,
            board_area,
            "YOU DIED!",
            "Restarting level...",
            theme::ERROR(),
        );
    }
}

fn get_field_lines(state: &State) -> Vec<Line<'static>> {
    let field = state.get_field();
    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        format!("┌{}┐", "─".repeat(state.field.width as usize)),
        Style::default().fg(theme::BORDER_ACTIVE()),
    )));

    for row in field {
        let mut spans = vec![Span::styled(
            "│",
            Style::default().fg(theme::BORDER_ACTIVE()),
        )];
        for cell in row {
            spans.push(cell_span(cell));
        }
        spans.push(Span::styled(
            "│",
            Style::default().fg(theme::BORDER_ACTIVE()),
        ));
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(Span::styled(
        format!("└{}┘", "─".repeat(state.field.width as usize)),
        Style::default().fg(theme::BORDER_ACTIVE()),
    )));

    lines
}

fn cell_span(something: Option<&ThingOnScreen>) -> Span<'static> {
    match something {
        Some(thing) => Span::styled(
            thing.value.clone(),
            Style::default()
                .fg(thing.color)
                .bg(theme::BG_SELECTION())
        ),
        None => Span::styled(" ", Style::default()
            .bg(theme::BG_SELECTION())),
    }
}

