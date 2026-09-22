//! The picker popup: the chosen line, the query, the list under its group
//! headings with the cursor on a tag, the keys.

use late_core::vocab::{self, TAG_LIMIT};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::{Row, TagPickerState};
use crate::app::common::{primitives::hint_line, theme};

const POPUP_W: u16 = 64;
const POPUP_H: u16 = 26;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &TagPickerState) {
    let Some(scope) = state.scope() else {
        return;
    };
    let popup = centered(area, POPUP_W.min(area.width), POPUP_H.min(area.height));
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(" Pick {} ", scope.title()))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [chosen_area, query_area, _, list_area, foot_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(inner);

    draw_chosen(frame, chosen_area, state);

    let query = Line::from(vec![
        Span::raw(" "),
        Span::styled("search ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_GLOW())),
        if state.query().is_empty() {
            Span::styled(
                "type to filter, by name or alias",
                Style::default().fg(theme::TEXT_FAINT()),
            )
        } else {
            Span::styled(
                format!("{}█", state.query()),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )
        },
    ]);
    frame.render_widget(Paragraph::new(query), query_area);

    draw_list(frame, list_area, state);

    let foot = match state.notice() {
        Some(notice) => Line::from(Span::styled(
            format!(" {notice}"),
            Style::default().fg(theme::ERROR()),
        )),
        None => hint_line(&[
            ("Space/Enter", "pick"),
            ("Backspace", "drop last"),
            ("Esc", "done"),
        ]),
    };
    frame.render_widget(Paragraph::new(foot), foot_area);
}

/// `chosen › rust · go · postgres` with the count on the first line's
/// right; dim when nothing is chosen yet.
fn draw_chosen(frame: &mut Frame, area: Rect, state: &TagPickerState) {
    let mut spans = vec![
        Span::raw(" "),
        Span::styled("chosen ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("› ", Style::default().fg(theme::AMBER_GLOW())),
    ];
    if state.chosen().is_empty() {
        spans.push(Span::styled(
            "nothing yet",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    for (idx, tag) in state.chosen().iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(
                " · ",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        spans.push(Span::styled(
            tag.clone(),
            Style::default().fg(theme::AMBER_DIM()),
        ));
    }
    spans.push(Span::styled(
        format!("  {} of {TAG_LIMIT}", state.chosen().len()),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_list(frame: &mut Frame, area: Rect, state: &TagPickerState) {
    let rows = state.rows();
    let height = area.height as usize;
    state.set_visible_height(height);
    let width = area.width as usize;
    let scroll = state.scroll().min(rows.len().saturating_sub(1));
    let end = (scroll + height).min(rows.len());
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, row) in rows[scroll..end].iter().enumerate() {
        let at = scroll + idx;
        match row {
            Row::Heading(group) => lines.push(Line::from(Span::styled(
                format!("   {}", group.label()),
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::BOLD),
            ))),
            Row::Tag(tag) => {
                let under_cursor = at == state.cursor();
                let chosen = state.is_chosen(tag);
                let mark = if chosen { "●" } else { "○" };
                let caret = if under_cursor { "›" } else { " " };
                let aliases: Vec<&str> = vocab::aliases_of(tag).iter().skip(1).copied().collect();
                let mut text = format!(" {caret} {mark} {tag}");
                if !aliases.is_empty() {
                    text.push_str("   ");
                    text.push_str(&aliases.join(", "));
                }
                let text = pad(&text, width);
                let mut style = Style::default().fg(if chosen {
                    theme::AMBER_DIM()
                } else {
                    theme::TEXT()
                });
                if under_cursor {
                    style = style
                        .fg(theme::AMBER_GLOW())
                        .bg(theme::BG_HIGHLIGHT())
                        .add_modifier(Modifier::BOLD);
                }
                lines.push(Line::from(Span::styled(text, style)));
            }
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "   nothing in the list spells that",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn pad(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        return text.chars().take(width).collect();
    }
    let mut out = String::from(text);
    out.push_str(&" ".repeat(width - len));
    out
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
