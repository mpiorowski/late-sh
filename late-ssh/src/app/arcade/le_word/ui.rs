use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{LetterScore, MAX_GUESSES, State, WORD_LEN};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_game_overlay, keys_line, status_line,
    tip_line,
};
use crate::app::common::theme;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", "daily".to_string(), theme::AMBER_GLOW()),
            (
                "guess",
                format!("{}/{}", state.guesses.len().min(MAX_GUESSES), MAX_GUESSES),
                theme::SUCCESS(),
            ),
            ("reward", "100".to_string(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(vec![
            ("a-z", "type"),
            ("Backspace", "delete"),
            ("Enter", "guess"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: Some(tip_line(state.message.clone())),
    };

    let board_area = draw_game_frame(frame, area, "Le Word", bottom, show_bottom_bar);
    let board_rect = centered_rect(
        board_area,
        24.min(board_area.width),
        13.min(board_area.height),
    );
    frame.render_widget(
        Paragraph::new(board_lines(state)).alignment(Alignment::Center),
        board_rect,
    );

    if state.won {
        draw_game_overlay(
            frame,
            board_area,
            "YOU WON!",
            "Come back tomorrow",
            theme::SUCCESS(),
        );
    } else if state.is_game_over {
        draw_game_overlay(
            frame,
            board_area,
            "GAME OVER",
            &state.answer.to_uppercase(),
            theme::ERROR(),
        );
    }
}

fn board_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(MAX_GUESSES * 2 - 1);
    for row in 0..MAX_GUESSES {
        if row > 0 {
            lines.push(Line::from(""));
        }

        let mut spans = Vec::with_capacity(WORD_LEN * 2 - 1);
        let guess = state.guesses.get(row).map(String::as_str);
        let current =
            (guess.is_none() && row == state.guesses.len()).then_some(&state.current_guess);
        for col in 0..WORD_LEN {
            if col > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(cell_span(state, guess, current, col));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn cell_span(
    state: &State,
    guess: Option<&str>,
    current: Option<&String>,
    col: usize,
) -> Span<'static> {
    let (ch, style) = if let Some(guess) = guess {
        let ch = guess
            .as_bytes()
            .get(col)
            .copied()
            .map(char::from)
            .unwrap_or(' ')
            .to_ascii_uppercase();
        let scores = state.scores_for_guess(guess);
        (ch, score_style(scores[col]))
    } else if let Some(current) = current {
        let ch = current
            .as_bytes()
            .get(col)
            .copied()
            .map(char::from)
            .unwrap_or(' ')
            .to_ascii_uppercase();
        (
            ch,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION()),
        )
    } else {
        (
            ' ',
            Style::default()
                .fg(theme::TEXT_DIM())
                .bg(theme::BG_SELECTION()),
        )
    };

    Span::styled(format!(" {ch} "), style.add_modifier(Modifier::BOLD))
}

fn score_style(score: LetterScore) -> Style {
    match score {
        LetterScore::Correct => Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(theme::SUCCESS()),
        LetterScore::Present => Style::default()
            .fg(ratatui::style::Color::Black)
            .bg(theme::AMBER()),
        LetterScore::Absent => Style::default()
            .fg(theme::TEXT_DIM())
            .bg(theme::BG_SELECTION()),
    }
}
