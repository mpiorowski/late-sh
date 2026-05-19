use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::{CatMood, CatState};
use crate::app::common::theme;

const MODAL_W: u16 = 36;
const MODAL_H: u16 = 13;

pub(crate) fn draw(frame: &mut Frame, state: &CatState) {
    let area = centered_rect(MODAL_W, MODAL_H, frame.area());
    frame.render_widget(Clear, area);

    let mood = state.mood();
    let eyes = mood.eyes();
    let mood_color = match mood {
        CatMood::Happy => theme::AMBER(),
        CatMood::Content => theme::TEXT_BRIGHT(),
        _ => theme::TEXT_DIM(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .title(Span::styled(
            " Cat Companion ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // art line 1
        Constraint::Length(1), // art line 2
        Constraint::Length(1), // art line 3
        Constraint::Length(1), // blank
        Constraint::Length(1), // mood
        Constraint::Length(1), // feedback or blank
        Constraint::Fill(1),   // spacer
        Constraint::Length(1), // f feed
        Constraint::Length(1), // w water
        Constraint::Length(1), // p play / q close
    ])
    .split(inner);

    let art_style = Style::default().fg(mood_color);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("  /\\_/\\  ", art_style))),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" ( {} ) ", eyes),
            art_style,
        ))),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("  >   <  ", art_style))),
        rows[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("mood: ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(
                mood.label(),
                Style::default().fg(mood_color).add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[4],
    );

    if let Some(fb) = state.action_feedback {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                fb,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::ITALIC),
            ))),
            rows[5],
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

    frame.render_widget(Paragraph::new(keybind("f", "feed")), rows[7]);
    frame.render_widget(Paragraph::new(keybind("w", "water")), rows[8]);
    frame.render_widget(Paragraph::new(keybind("p", "play  ·  q close")), rows[9]);
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
