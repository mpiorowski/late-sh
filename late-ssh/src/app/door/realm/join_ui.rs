//! The colour picker drawn over the board when a visitor presses `j`: the
//! one thing to settle before marching in, and the one thing it must not do
//! is let two players wear the same colour.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;

use super::state::{PLAYER_PALETTES, RealmJoinDraft};

pub(crate) fn draw_join_overlay(frame: &mut Frame, area: Rect, draft: &RealmJoinDraft) {
    let width = 58u16.min(area.width);
    let text_width = width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = vec![Line::raw("")];
    lines.push(Line::from(Span::styled(
        format!(
            "   {}",
            truncate(&draft.game_name, text_width.saturating_sub(3))
        ),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(""));
    lines.extend(color_lines(draft.color, &draft.taken, text_width));
    lines.push(Line::raw(""));
    for chunk in wrap(
        "you spawn where there is room and your first days are protected: nobody can touch you while you find your feet",
        text_width.saturating_sub(3),
    ) {
        lines.push(Line::from(Span::styled(
            format!("   {chunk}"),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw(" "),
        key("j/k"),
        text(" colour"),
        gap(),
        key("enter"),
        text(" march in"),
        gap(),
        key("esc"),
        text(" not yet"),
    ]));

    let height = (lines.len() as u16 + 2).min(area.height);
    let rect = centered_rect(width, height, area);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(" take your spot: your colour ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// One row per palette colour: its five shades, its name, and who wears it
/// already. The cursor never sits on a taken one.
fn color_lines(cursor: usize, taken: &[(usize, String)], text_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (idx, palette) in PLAYER_PALETTES.iter().enumerate() {
        let held = taken.iter().find(|(slot, _)| *slot == idx);
        let is_selected = idx == cursor && held.is_none();
        let mut spans = vec![Span::raw(" "), marker_span(is_selected)];
        for (r, g, b) in palette.shades {
            spans.push(Span::styled(
                "\u{25ae}",
                Style::default().fg(if held.is_some() {
                    theme::TEXT_FAINT()
                } else {
                    Color::Rgb(r, g, b)
                }),
            ));
        }
        spans.push(Span::styled(
            format!(" {:<9}", palette.name),
            Style::default().fg(match (held.is_some(), is_selected) {
                (true, _) => theme::TEXT_FAINT(),
                (false, true) => theme::TEXT_BRIGHT(),
                (false, false) => theme::TEXT(),
            }),
        ));
        if let Some((_, who)) = held {
            let room = text_width.saturating_sub(21);
            spans.push(Span::styled(
                truncate(&format!("worn by {who}"), room),
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn marker_span(selected: bool) -> Span<'static> {
    match selected {
        true => Span::styled(
            "\u{25b8} ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        false => Span::raw("  "),
    }
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )
}

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
}

fn gap() -> Span<'static> {
    Span::raw("   ")
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let [vertical] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [rect] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(vertical);
    rect
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let keep: String = text.chars().take(width.saturating_sub(1)).collect();
    format!("{keep}\u{2026}")
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width.max(1) {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
