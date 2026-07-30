use std::cell::Cell;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;
use late_core::models::user::InteractionMode;

/// The three choices, in display order. The first is the highlighted default.
pub(crate) const OPTIONS: [(InteractionMode, &str, &str); 3] = [
    (
        InteractionMode::Hybrid,
        "Both",
        "Keyboard shortcuts and the mouse both work. (recommended)",
    ),
    (
        InteractionMode::Mouse,
        "Mouse",
        "Click everything, like Discord.",
    ),
    (
        InteractionMode::Keyboard,
        "Keyboard",
        "Classic terminal; mouse off so your own copy/paste works.",
    ),
];

const WIDTH: u16 = 66;
const HEIGHT: u16 = 13;

/// Draw the one-time first-run prompt and record each option's rect (so a click
/// can pick it). `selected` is the keyboard-highlighted row.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    option_rects: &Cell<[Option<Rect>; 3]>,
) {
    let popup = centered_rect(WIDTH, HEIGHT, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Welcome to late.sh ")
        .border_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        );
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // prompt
        Constraint::Length(1), // blank
        Constraint::Length(1), // option 0
        Constraint::Length(1), // option 1
        Constraint::Length(1), // option 2
        Constraint::Min(0),    // spacer
        Constraint::Length(1), // footer
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "How do you want to get around?",
            Style::default().fg(theme::TEXT_BRIGHT()),
        ))),
        rows[0],
    );

    let mut rects = [None; 3];
    for (i, (_, label, desc)) in OPTIONS.iter().enumerate() {
        let row = rows[2 + i];
        rects[i] = Some(row);
        let chosen = i == selected;
        let (marker, style) = if chosen {
            (
                "▶ ",
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            ("  ", Style::default().fg(theme::TEXT_DIM()))
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{marker}{}. ", i + 1), style),
                Span::styled(
                    format!("{label}  "),
                    if chosen {
                        style
                    } else {
                        Style::default().fg(theme::TEXT_BRIGHT())
                    },
                ),
                Span::styled(
                    (*desc).to_string(),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ])),
            row,
        );
    }
    option_rects.set(rects);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "1-3 or click to choose · ↑↓ then Enter · change it later in Settings",
            Style::default().fg(theme::TEXT_FAINT()),
        ))),
        rows[6],
    );
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);
    cell
}

#[cfg(test)]
mod tests {
    use super::OPTIONS;
    use late_core::models::user::InteractionMode;

    #[test]
    fn options_cover_every_mode_with_hybrid_first() {
        assert_eq!(OPTIONS.len(), 3);
        assert_eq!(
            OPTIONS[0].0,
            InteractionMode::Hybrid,
            "recommended default leads"
        );
        let modes: Vec<_> = OPTIONS.iter().map(|(m, _, _)| *m).collect();
        for m in [
            InteractionMode::Keyboard,
            InteractionMode::Mouse,
            InteractionMode::Hybrid,
        ] {
            assert!(modes.contains(&m), "every mode is offered: {m:?}");
        }
    }
}
