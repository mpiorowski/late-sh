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
use super::ui::truncate;

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

/// Twenty colours in two columns of ten, read down: each its three shades,
/// its name, and who wears it already. The cursor never sits on a taken one.
fn color_lines(cursor: usize, taken: &[(usize, String)], text_width: usize) -> Vec<Line<'static>> {
    let rows = PLAYER_PALETTES.len().div_ceil(2);
    let column = text_width / 2;
    (0..rows)
        .map(|row| {
            let mut spans = vec![Span::raw(" ")];
            for idx in [row, row + rows] {
                if idx < PLAYER_PALETTES.len() {
                    spans.extend(color_cell(idx, cursor, taken, column));
                }
            }
            Line::from(spans)
        })
        .collect()
}

/// One colour of the picker, padded to its column.
fn color_cell(
    idx: usize,
    cursor: usize,
    taken: &[(usize, String)],
    width: usize,
) -> Vec<Span<'static>> {
    let palette = &PLAYER_PALETTES[idx];
    let held = taken.iter().find(|(slot, _)| *slot == idx);
    let is_selected = idx == cursor && held.is_none();
    let mut spans = vec![marker_span(is_selected)];
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
        format!(" {:<8}", palette.name),
        Style::default().fg(match (held.is_some(), is_selected) {
            (true, _) => theme::TEXT_FAINT(),
            (false, true) => theme::TEXT_BRIGHT(),
            (false, false) => theme::TEXT(),
        }),
    ));
    // Marker, shades and name take fourteen columns; who wears it gets the
    // rest of the cell.
    let room = width.saturating_sub(15);
    let worn = match held {
        Some((_, who)) => truncate(who, room),
        None => String::new(),
    };
    spans.push(Span::styled(
        format!(" {worn:<room$}"),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    spans
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
