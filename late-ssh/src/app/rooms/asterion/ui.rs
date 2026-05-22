use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{common::theme, rooms::asterion::state::State};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, _usernames: &HashMap<Uuid, String>) {
    let snapshot = state.snapshot();
    let mut lines = vec![
        Line::from(Span::styled(
            "Asterion",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            snapshot.status_message.clone(),
            Style::default().fg(theme::TEXT_DIM()),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
    ];
    for hero in &snapshot.heroes {
        let marker = if hero.player_id == state.user_id() {
            "▶"
        } else {
            " "
        };
        let color = if hero.player_id == state.user_id() {
            theme::AMBER()
        } else {
            theme::TEXT_DIM()
        };
        lines.push(
            Line::from(Span::styled(
                format!(
                    "{marker} {} · maze {} · {:?}",
                    hero.name, hero.maze_id, hero.position
                ),
                Style::default().fg(color),
            ))
            .alignment(Alignment::Center),
        );
    }
    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            "wasd / hjkl / arrows to move · , . to turn · o cycle ui · Esc to leave",
            Style::default().fg(theme::TEXT_FAINT()),
        ))
        .alignment(Alignment::Center),
    );
    frame.render_widget(Paragraph::new(lines), area);
}
