use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use late_core::models::goldfish::MAX_FRIENDS;

use super::state::{GoldfishMood, GoldfishState};
use crate::app::common::theme;

const MODAL_W: u16 = 38;
const MODAL_H: u16 = 16;

pub(crate) fn draw(frame: &mut Frame, state: &GoldfishState) {
    let area = centered_rect(MODAL_W, MODAL_H, frame.area());
    frame.render_widget(Clear, area);

    let mood = state.mood();
    let mood_color = match mood {
        GoldfishMood::Happy => theme::AMBER(),
        GoldfishMood::Content => theme::TEXT_BRIGHT(),
        _ => theme::TEXT_DIM(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .title(Span::styled(
            " Goldfish Bowl ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // bowl top
        Constraint::Length(1), // fish
        Constraint::Length(1), // friends row
        Constraint::Length(1), // bowl bottom
        Constraint::Length(1), // blank
        Constraint::Length(1), // mood
        Constraint::Length(1), // feedback
        Constraint::Fill(1),   // spacer
        Constraint::Length(1), // f feed
        Constraint::Length(1), // d decorate
        Constraint::Length(1), // l lights
        Constraint::Length(1), // w water
        Constraint::Length(1), // a friend / q close
    ])
    .split(inner);

    let w = inner.width as usize;

    // Bowl
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(".{}.", "~".repeat(w.saturating_sub(2))),
            Style::default().fg(theme::BORDER_DIM()),
        ))),
        rows[0],
    );

    // Main fish + bubbles
    let fish = format!("><((({}> ", mood.eye());
    let bubbles = if mood == GoldfishMood::Happy { " o o" } else { "    " };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("| ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(fish, Style::default().fg(mood_color)),
            Span::styled(bubbles, Style::default().fg(theme::TEXT_FAINT())),
        ])),
        rows[1],
    );

    // Friends row
    let friends: String = (0..state.friend_count).map(|_| "><> ").collect();
    let empty_friends = format!(
        "{}/{}",
        state.friend_count,
        MAX_FRIENDS
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("| ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(
                if state.friend_count > 0 {
                    friends
                } else {
                    "no friends yet".to_string()
                },
                Style::default().fg(theme::TEXT_FAINT()),
            ),
            Span::raw("  "),
            Span::styled(empty_friends, Style::default().fg(theme::TEXT_FAINT())),
        ])),
        rows[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("`{}'", "~".repeat(w.saturating_sub(2))),
            Style::default().fg(theme::BORDER_DIM()),
        ))),
        rows[3],
    );

    // Mood
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("mood: ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(
                mood.label(),
                Style::default()
                    .fg(mood_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[5],
    );

    // Feedback
    if let Some(fb) = state.action_feedback {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fb,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::ITALIC),
            ))),
            rows[6],
        );
    }

    let keybind = |key: &'static str, label: &'static str| -> Line<'static> {
        Line::from(vec![
            Span::styled(
                key,
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                label,
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
        ])
    };

    frame.render_widget(Paragraph::new(keybind("f", "feed")), rows[8]);
    frame.render_widget(Paragraph::new(keybind("d", "rocks & plants")), rows[9]);
    frame.render_widget(Paragraph::new(keybind("l", "lights")), rows[10]);
    frame.render_widget(Paragraph::new(keybind("w", "change water")), rows[11]);
    frame.render_widget(
        Paragraph::new(keybind("a", "add friend  ·  q close")),
        rows[12],
    );
}

fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w.min(area.width),
        height: h.min(area.height),
    }
}
