use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap,
    },
};

use crate::app::common::theme;
use crate::app::input::{MouseEvent, MouseEventKind};
use crate::moderation::command::ModCommandHead;

use super::state::{ModLogKind, ModLogLine, ModModalState};

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let popup = centered_percent_rect(80, 93, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Moderation ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Min(6),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_log(frame, layout[0], state);
    draw_input(frame, layout[1], state);
    draw_footer(frame, layout[2]);
    if state.is_autocomplete_active() {
        crate::app::chat::ui::draw_mention_autocomplete(
            frame,
            layout[1],
            state.autocomplete_matches(),
            state.autocomplete_selected(),
        );
    }
}

pub(crate) fn mouse_scroll_delta(mouse: MouseEvent) -> Option<i16> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(3),
        MouseEventKind::ScrollDown => Some(-3),
        _ => None,
    }
}

fn draw_log(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    let height = inner.height as usize;
    let log = state.log();
    let lines: Vec<Line<'static>> = log.iter().map(log_line).collect();
    let line_heights: Vec<usize> = lines
        .iter()
        .map(|line| {
            Paragraph::new(line.clone())
                .wrap(Wrap { trim: false })
                .line_count(inner.width)
        })
        .collect();
    let total_height = line_heights.iter().sum::<usize>();
    let start = state.viewport_start(height, &line_heights);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((start.min(u16::MAX as usize) as u16, 0)),
        inner,
    );

    if total_height > height {
        let mut scrollbar_state = ScrollbarState::new(total_height)
            .position(start.min(total_height.saturating_sub(1)))
            .viewport_content_length(height);
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_style(Style::default().fg(theme::BORDER()))
            .thumb_style(Style::default().fg(theme::AMBER_DIM()));
        frame.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn draw_input(frame: &mut Frame, area: Rect, state: &ModModalState) {
    let block = Block::default()
        .title(" Command ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(state.command_input(), inner);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" run  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Ctrl+X", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" clear screen  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ PgUp/PgDn", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn log_line(line: &ModLogLine) -> Line<'static> {
    let style = match line.kind {
        ModLogKind::Help => return help_line(&line.text),
        ModLogKind::Input => Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
        ModLogKind::Separator => Style::default().fg(theme::AMBER_DIM()),
        ModLogKind::Info => Style::default().fg(theme::TEXT_DIM()),
        ModLogKind::Success => Style::default().fg(theme::SUCCESS()),
        ModLogKind::Error => Style::default().fg(theme::ERROR()),
    };
    Line::from(Span::styled(line.text.clone(), style))
}

fn help_line(text: &str) -> Line<'static> {
    let body = Style::default().fg(theme::TEXT_DIM());
    let shade = theme::blend_toward(theme::BG_HIGHLIGHT(), theme::BG_CANVAS(), 0.65);
    if text.starts_with("==") {
        let title = text.trim_matches('=').trim();
        if title.is_empty() {
            return Line::from(Span::styled(text.to_owned(), body.bg(shade)));
        }
        let start = text.find(title).expect("title is part of the heading");
        return Line::from(vec![
            Span::styled(text[..start].to_owned(), body.bg(shade)),
            Span::styled(
                title.to_owned(),
                Style::default()
                    .fg(theme::TEXT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(text[start + title.len()..].to_owned(), body.bg(shade)),
        ]);
    }

    let syntax = text.split(" - ").next().unwrap_or(text);
    let mut tokens = syntax.split_whitespace();
    let command = tokens.next().unwrap_or_default();
    let known_command = ModCommandHead::from_word(command).is_some();
    let mut has_argument = false;
    let is_usage = tokens.all(|token| {
        if token.starts_with(['<', '[', '@', '#']) {
            has_argument = true;
            true
        } else {
            !has_argument || matches!(token, "mod" | "by")
        }
    });
    if !known_command
        || !is_usage
        || !(has_argument || syntax.trim() == command || text.contains(" - "))
    {
        return Line::from(Span::styled(text.to_owned(), body));
    }

    let mut spans = Vec::new();
    let mut description = false;
    let mut first_token = true;
    for part in text.split_inclusive(char::is_whitespace) {
        let token = part.trim_end();
        if token == "-" {
            description = true;
        }
        let style = if token.is_empty() || description {
            body
        } else if first_token {
            first_token = false;
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(shade)
                .add_modifier(Modifier::BOLD)
        } else if token.starts_with(['<', '[', '@', '#']) {
            body.bg(shade)
        } else {
            Style::default().fg(theme::TEXT_MUTED()).bg(shade)
        };
        if !token.is_empty() {
            if !description && token.starts_with(['<', '[', '@', '#']) {
                push_help_argument(&mut spans, token, style);
            } else {
                spans.push(Span::styled(token.to_owned(), style));
            }
        }
        let spacing = &part[token.len()..];
        if !spacing.is_empty() {
            spans.push(Span::styled(spacing.to_owned(), body));
        }
    }
    Line::from(spans)
}

fn push_help_argument(spans: &mut Vec<Span<'static>>, token: &str, style: Style) {
    let punctuation = style.fg(theme::blend_toward(
        theme::TEXT_FAINT(),
        style.bg.unwrap_or(theme::BG_CANVAS()),
        1.0 / 3.0,
    ));
    let mut start = 0;
    for (index, ch) in token.char_indices() {
        if matches!(ch, '<' | '>' | '[' | ']' | '|' | '.') {
            if start < index {
                spans.push(Span::styled(token[start..index].to_owned(), style));
            }
            spans.push(Span::styled(ch.to_string(), punctuation));
            start = index + ch.len_utf8();
        }
    }
    if start < token.len() {
        spans.push(Span::styled(token[start..].to_owned(), style));
    }
}

fn centered_percent_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let width = percent_of(area.width, width_percent).max(1);
    let height = percent_of(area.height, height_percent).max(1);
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn percent_of(value: u16, percent: u16) -> u16 {
    ((value as u32 * percent as u32) / 100) as u16
}
