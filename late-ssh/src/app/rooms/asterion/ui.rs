use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use uuid::Uuid;

use crate::app::{common::theme, rooms::asterion::state::State};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, _usernames: &HashMap<Uuid, String>) {
    if area.height < 10 || area.width < 60 {
        draw_compact(frame, area, state);
        return;
    }
    let columns = Layout::horizontal([Constraint::Min(40), Constraint::Length(28)]).split(area);
    draw_maze(frame, columns[0], state);
    draw_sidebar(frame, columns[1], state);
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let lines = state.lines();
    if lines.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Asterion - loading...",
                Style::default().fg(theme::TEXT_DIM()),
            ))
            .alignment(Alignment::Center),
            area,
        );
        return;
    }
    frame.render_widget(Paragraph::new(lines.to_vec()), area);
}

fn draw_maze(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = state.lines();
    if lines.is_empty() {
        let msg = if state.private().seated {
            "Rendering..."
        } else {
            "Joining maze..."
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(theme::TEXT_DIM())))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }
    frame.render_widget(Paragraph::new(lines.to_vec()), inner);
}

fn draw_sidebar(frame: &mut Frame, area: Rect, state: &State) {
    let private = state.private();
    let public = state.public();

    let status = if private.has_won {
        "ESCAPED"
    } else if private.is_dead {
        "Knocked out"
    } else if private.seated {
        "Alive"
    } else {
        "Joining..."
    };

    let status_color = if private.has_won {
        theme::AMBER_GLOW()
    } else if private.is_dead {
        theme::ERROR()
    } else {
        theme::AMBER()
    };

    let lines = vec![
        Line::from(Span::styled(
            "ASTERION",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        line_kv("Status", status, Some(status_color)),
        line_kv("Maze", &format!("{}", private.maze_id), None),
        line_kv(
            "Pos",
            &format!("({}, {})", private.position.0, private.position.1),
            None,
        ),
        Line::from(""),
        line_kv("Heroes", &format!("{}", public.hero_count), None),
        Line::from(""),
        Line::from(Span::styled(
            public.status_message.clone(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Controls",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "wasd/hjkl move",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(Span::styled(
            "arrows move",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(Span::styled(
            ", . turn",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(Span::styled(
            "Esc leave",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ];

    let block = Block::bordered().border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn line_kv(label: &str, value: &str, value_color: Option<ratatui::style::Color>) -> Line<'static> {
    let value_style = if let Some(c) = value_color {
        Style::default().fg(c).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    Line::from(vec![
        Span::styled(
            format!(" {label:<7}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(value.to_string(), value_style),
    ])
}
