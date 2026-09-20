//! Nightcap's one screen: a title, a row of seats, a footer. Deliberately
//! no backdrop art, no camera, no floor plan to walk around — see the
//! module doc on `state.rs` for why this stays small.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

use super::lobby::SEAT_COUNT;
use super::state::State;

pub struct NightcapView<'a> {
    pub state: &'a State,
}

pub fn draw(frame: &mut Frame, area: Rect, view: NightcapView<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "nightcap",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "a quiet spot out back of the clubhouse",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ]),
        header,
    );

    draw_seats(frame, body, view.state);
    draw_footer(frame, footer, view.state);
}

fn draw_seats(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let my_seat = state.my_seat();
    let rows: Vec<Constraint> = std::iter::repeat_n(Constraint::Length(1), SEAT_COUNT).collect();
    let layout = Layout::vertical(rows).split(area);

    for (idx, slot) in snapshot.iter().enumerate() {
        let seat_num = idx + 1;
        let mine = my_seat == Some(idx);
        let line = match slot {
            Some(occupant) => {
                let style = if mine {
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT_BRIGHT())
                };
                let name = if mine {
                    format!("{} (you)", occupant.username)
                } else {
                    occupant.username.clone()
                };
                let drinks = "●".repeat(occupant.drinks.min(5) as usize);
                let mut spans = vec![
                    Span::styled(
                        format!("{seat_num} "),
                        Style::default().fg(theme::TEXT_DIM()),
                    ),
                    Span::styled("●", style),
                    Span::raw(" "),
                    Span::styled(name, style),
                ];
                if !drinks.is_empty() {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        drinks,
                        Style::default().fg(theme::AMBER_DIM()),
                    ));
                }
                Line::from(spans)
            }
            None => Line::from(vec![
                Span::styled(
                    format!("{seat_num} "),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled("○", Style::default().fg(theme::BORDER_DIM())),
                Span::styled(
                    " empty stool",
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                ),
            ]),
        };
        frame.render_widget(Paragraph::new(line), layout[idx]);
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &State) {
    let spans = match &state.last_message {
        Some(message) => vec![Span::styled(
            message.clone(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::ITALIC),
        )],
        None => vec![
            Span::styled("1-6", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" sit/stand  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("d", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(" order a drink  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(
                " back to the clubhouse",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ],
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
