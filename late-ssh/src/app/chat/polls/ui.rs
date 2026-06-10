use late_core::models::chat_poll::{
    POLL_MAX_OPTIONS, POLL_OPTION_MAX_CHARS, POLL_QUESTION_MAX_CHARS,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    chat::polls::state::{PollField, PollModalState},
    common::theme,
};

pub(crate) fn draw_modal(frame: &mut Frame, area: Rect, state: &PollModalState) {
    if !state.is_open() {
        return;
    }
    let popup = centered_rect(area, 68, 18);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Poll ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Length(3),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme::SUCCESS())),
            Span::styled(" create  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Tab", Style::default().fg(theme::AMBER())),
            Span::styled(" next  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::ERROR())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]))
        .style(Style::default().bg(theme::BG_CANVAS())),
        areas[0],
    );
    draw_field(
        frame,
        areas[1],
        "Question",
        state.question(),
        state.focus() == PollField::Question,
        POLL_QUESTION_MAX_CHARS,
    );
    for index in 0..POLL_MAX_OPTIONS {
        draw_field(
            frame,
            areas[2 + index],
            &format!("Option {}", index + 1),
            &state.options()[index],
            state.focus() == PollField::Option(index),
            POLL_OPTION_MAX_CHARS,
        );
    }
    draw_duration_field(
        frame,
        areas[2 + POLL_MAX_OPTIONS],
        state,
        state.focus() == PollField::Duration,
    );
}

fn draw_field(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    input: &ratatui_textarea::TextArea<'static>,
    focused: bool,
    max_chars: usize,
) {
    let border = if focused {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let title = format!(
        " {label} {}/{} ",
        input.lines().join(" ").chars().count(),
        max_chars
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(if focused {
                    theme::TEXT_BRIGHT()
                } else {
                    theme::TEXT_DIM()
                })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(input, inner);
}

fn draw_duration_field(frame: &mut Frame, area: Rect, state: &PollModalState, focused: bool) {
    let border = if focused {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let block = Block::default()
        .title(Span::styled(
            " Duration ",
            Style::default()
                .fg(if focused {
                    theme::TEXT_BRIGHT()
                } else {
                    theme::TEXT_DIM()
                })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut spans = Vec::new();
    for (index, duration_secs) in state.duration_options_secs().iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        let minutes = duration_secs / 60;
        let selected = state.duration_index() == index;
        let style = if selected {
            Style::default()
                .fg(theme::BG_CANVAS())
                .bg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {}m ", minutes), style));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_CANVAS())),
        inner,
    );
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
