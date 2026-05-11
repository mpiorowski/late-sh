use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{CobraState, State, ThingOnScreen};
use crate::app::common::theme;
use crate::app::games::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
    tip_line,
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_sidebar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("score", state.score.to_string(), theme::AMBER_GLOW()),
            ("best", state.best_score.to_string(), theme::SUCCESS()),
            ("level", state.level.to_string(), theme::TEXT_BRIGHT()),
            // ("tick", state.field_tick.to_string(), theme::TEXT_BRIGHT()),
            // ("stutter_left", state.stutter_left.to_string(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(vec![
            ("hjkl/wsad", "direction"),
            ("p", "pause"),
            ("r", "restart"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: Some(tip_line("Snake by github.com/AndreLobato")),
    };

    let board_area = draw_game_frame(frame, area, "Snake", bottom, show_sidebar);
    let board_rect = centered_rect(
        board_area,
        state.field.width as u16 * 2,
        state.field.height as u16,
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

    for row in field {
        let mut spans = Vec::with_capacity(row.len());
        for cell in row {
            spans.push(cell_span(cell));
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn cell_span(something: Option<&ThingOnScreen>) -> Span<'static> {
    match something {
        Some(thing) => Span::styled(
            cell_text(thing),
            Style::default().fg(thing.color).bg(theme::BG_SELECTION()),
        ),
        None => Span::styled("  ", Style::default().bg(theme::BG_SELECTION())),
    }
}

fn cell_text(thing: &ThingOnScreen) -> String {
    match thing.value.as_str() {
        "═" => "══".to_string(),
        "╔" => "╔═".to_string(),
        "╗" => "═╗".to_string(),
        "╚" => "╚═".to_string(),
        "╝" => "═╝".to_string(),
        _ => format!("{:<2}", thing.value),
    }
}
