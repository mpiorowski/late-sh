//! The event modal and the fight panel. Rendering only: everything here reads
//! session state and returns lines, and nothing it does advances a clock.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use unicode_width::UnicodeWidthStr;

use crate::app::common::theme;

use super::event::{Phase, Row as EventRow};
use super::state::{Row, State};

/// Draw the modal over whatever panel is behind it.
pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let Some(active) = state.event.as_ref() else {
        return;
    };
    let width = area.width.saturating_sub(8).clamp(20, 64);
    // Only the width stays capped, so the modal reads as a fixed-size card.
    // Height follows the terminal: a fight with a full pack of weapons and
    // items needs every row it can get.
    let height = area.height.saturating_sub(4).max(10);
    let modal = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .title(format!(" {} ", active.event.title))
        .title_style(
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let body = body_lines(state);
    let actions = action_lines(state);
    let body_height = body_height(&body, inner.width, inner.height);

    let rows = Layout::vertical([
        Constraint::Length(body_height),
        Constraint::Length(1),
        Constraint::Min(3),
    ])
    .split(inner);

    frame.render_widget(Paragraph::new(body).wrap(Wrap { trim: true }), rows[0]);

    let visible = rows[2].height as usize;
    let offset = scroll_offset(state.cursor, actions.len(), visible);
    frame.render_widget(Paragraph::new(actions).scroll((offset, 0)), rows[2]);
}

/// Rows the body (scene text, or the two fighters) gets: what it needs once
/// wrapped at `width`, up to half the panel, so a long action list (many
/// weapons and items) gets the rest.
pub fn body_height(body: &[Line<'_>], width: u16, height: u16) -> u16 {
    let needed: usize = body
        .iter()
        .map(|line| wrapped_rows(&line.to_string(), usize::from(width)))
        .sum();
    let cap = height.saturating_sub(4).max(3) / 2 + 3;
    (needed.min(usize::from(u16::MAX)) as u16).clamp(3, cap)
}

/// Rows `text` takes word-wrapped at `width`, the way the body's `Wrap`
/// breaks it: on spaces, with a word longer than the width split across
/// rows. Always at least 1.
fn wrapped_rows(text: &str, width: usize) -> usize {
    if width == 0 {
        return 1;
    }
    let mut rows = 1usize;
    let mut col = 0usize;
    for (i, token) in text.split(' ').enumerate() {
        if i > 0 {
            if col + 1 > width {
                rows += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        let tw = UnicodeWidthStr::width(token);
        if tw == 0 {
            continue;
        }
        if col + tw <= width {
            col += tw;
        } else {
            if col > 0 {
                rows += 1;
            }
            if tw > width {
                // a single word longer than the card breaks across rows
                let extra = (tw - 1) / width;
                rows += extra;
                col = tw - extra * width;
            } else {
                col = tw;
            }
        }
    }
    rows
}

/// How many action rows to scroll past so the cursor stays on screen. Follows
/// the cursor rather than paging, so moving one row at a time never jumps the
/// list further than necessary.
pub fn scroll_offset(cursor: usize, total: usize, visible: usize) -> u16 {
    if visible == 0 || total <= visible {
        return 0;
    }
    let cursor = cursor.min(total - 1);
    let max_offset = total - visible;
    cursor.saturating_sub(visible - 1).min(max_offset) as u16
}

/// The scene text, or the two fighters and their health.
fn body_lines(state: &State) -> Vec<Line<'static>> {
    let Some(active) = state.event.as_ref() else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    match &active.phase {
        Phase::Fighting(fight) => {
            let Some(combat) = active.scene.combat else {
                return lines;
            };
            let (hp, max) = state
                .game()
                .map(|game| {
                    (
                        game.expedition.as_ref().map(|trip| trip.hp).unwrap_or(0),
                        game.max_health(),
                    )
                })
                .unwrap_or((0, 0));
            lines.push(Line::from(Span::styled(
                combat.enemy.to_string(),
                Style::default().fg(theme::TEXT_BRIGHT()),
            )));
            let mut enemy_row = vec![
                Span::styled(
                    format!("{}  ", combat.chara),
                    Style::default().fg(theme::ERROR()),
                ),
                Span::styled(
                    format!("{}/{}", fight.enemy_hp, fight.enemy_max),
                    Style::default().fg(theme::TEXT()),
                ),
            ];
            if let Some(held) = fight.enemy_status {
                enemy_row.push(Span::styled(
                    format!("  ({})", held.status.label()),
                    Style::default().fg(theme::AMBER()),
                ));
            }
            lines.push(Line::from(enemy_row));
            lines.push(Line::from(""));
            let mut player_row = vec![
                Span::styled("@  ", Style::default().fg(theme::AMBER())),
                Span::styled(format!("{hp}/{max}"), Style::default().fg(theme::TEXT())),
            ];
            if let Some(held) = fight.player_status {
                player_row.push(Span::styled(
                    format!("  ({})", held.status.label()),
                    Style::default().fg(theme::AMBER()),
                ));
            }
            if fight.bleed.is_some() {
                player_row.push(Span::styled(
                    "  (bleeding)",
                    Style::default().fg(theme::ERROR()),
                ));
            }
            lines.push(Line::from(player_row));
            if let Some(last) = &fight.last_hit {
                lines.push(Line::from(Span::styled(
                    last.clone(),
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
        }
        Phase::Exploding { .. } => {
            lines.push(Line::from(Span::styled(
                "the wreck shudders, and something inside it starts to whine.",
                Style::default().fg(theme::ERROR()),
            )));
        }
        Phase::DropFor { .. } => {
            lines.push(Line::from(Span::styled(
                "not enough room. choose what to drop:",
                Style::default().fg(theme::TEXT()),
            )));
        }
        Phase::Story | Phase::Spoils { .. } => {
            for text in active.scene.text {
                lines.push(Line::from(Span::styled(
                    (*text).to_string(),
                    Style::default().fg(theme::TEXT()),
                )));
            }
            if matches!(active.phase, Phase::Spoils { .. }) && active.loot.is_empty() {
                lines.push(Line::from(Span::styled(
                    "nothing to take",
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
        }
    }
    lines
}

/// The buttons, loot rows and the way out.
fn action_lines(state: &State) -> Vec<Line<'static>> {
    let selected = state.selected();
    state
        .rows()
        .into_iter()
        .map(|row| {
            let is_selected = row == selected;
            let ready = !state.row_at_maximum(row);
            let marker = if is_selected { "> " } else { "  " };
            let style = match (is_selected, ready) {
                (true, _) => Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
                (false, true) => Style::default().fg(theme::TEXT()),
                (false, false) => Style::default().fg(theme::TEXT_FAINT()),
            };
            let mut spans = vec![
                Span::styled(marker, Style::default().fg(theme::AMBER())),
                Span::styled(state.row_label(row), style),
            ];
            let cooldown = state.row_cooldown(row);
            if cooldown > 0 {
                spans.push(Span::styled(
                    format!("  {cooldown}s"),
                    Style::default().fg(theme::TEXT_FAINT()),
                ));
            } else {
                let cost = state.row_cost(row);
                if !cost.is_empty() {
                    let hint = cost
                        .iter()
                        .map(|(item, amount)| format!("{amount} {}", item.label()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    spans.push(Span::styled(
                        format!("  {hint}"),
                        Style::default().fg(theme::TEXT_DIM()),
                    ));
                }
            }
            Line::from(spans)
        })
        .collect()
}

/// Whether the modal owns the keyboard right now.
pub fn is_open(state: &State) -> bool {
    state.event.is_some()
}

/// Whether the row under the cursor is a fight row, for the footer hint.
pub fn in_fight(state: &State) -> bool {
    matches!(
        state.selected(),
        Row::Event(EventRow::Attack(_)) | Row::Event(EventRow::Eat) | Row::Event(EventRow::Meds)
    )
}
