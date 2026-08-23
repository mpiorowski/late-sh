use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use ratatui::style::Color;

use super::state::{CobraState, State, ThingKind, ThingOnScreen};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
};
use crate::app::common::theme;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
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
        tip: None,
    };

    let board_area = draw_game_frame(frame, area, "Snake", bottom, show_bottom_bar);
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
            spans.push(cell_span(cell, &state.cobra.state));
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn cell_span(something: Option<&ThingOnScreen>, cobra: &CobraState) -> Span<'static> {
    match something {
        Some(thing) => Span::styled(
            cell_text(thing),
            Style::default()
                .fg(glyph_color(&thing.kind, cobra))
                .bg(theme::BG_SELECTION()),
        ),
        None => Span::styled("  ", Style::default().bg(theme::BG_SELECTION())),
    }
}

/// Board glyph colors resolve here at draw time, not in the state: re-read
/// every frame so a mid-level theme switch repaints glyphs together with the
/// board, and passed through `legible_on_selection` because twelve AMOLED
/// palettes reuse an accent as the very `bg_selection` the board paints on
/// (e.g. `success` on Greenery), where the raw token renders fg == bg.
fn glyph_color(kind: &ThingKind, cobra: &CobraState) -> Color {
    let accent = match kind {
        ThingKind::Food => theme::AMBER_GLOW(),
        ThingKind::Drug => theme::MENTION(),
        ThingKind::Rock => theme::TEXT_FAINT(),
        // The powered-up snake wears the same color as the star that powered
        // it up, so the effect reads at a glance on every palette.
        ThingKind::Cobra => match cobra {
            CobraState::PoweredUp => theme::MENTION(),
            CobraState::Alive | CobraState::Dead => theme::SUCCESS(),
        },
        ThingKind::Edge => theme::TEXT_BRIGHT(),
    };
    theme::legible_on_selection(accent)
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

// A child of this module (not a sibling in mod.rs) so the palette sweep can
// drive the private glyph color resolution directly.
#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
