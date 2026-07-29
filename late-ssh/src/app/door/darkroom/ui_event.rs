//! The event modal and the fight panel. Rendering only: everything here reads
//! session state and returns lines, and nothing it does advances a clock.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::common::theme;

use super::event::{Phase, Row as EventRow};
use super::state::{Row, State};

/// Draw the modal over whatever panel is behind it.
pub fn draw(frame: &mut Frame, area: Rect, state: &State) {
    let Some(active) = state.event.as_ref() else {
        return;
    };
    let width = area.width.saturating_sub(8).clamp(20, 64);
    let height = area.height.saturating_sub(4).clamp(10, 20);
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

    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(body_lines(state)).wrap(Wrap { trim: true }),
        rows[0],
    );
    frame.render_widget(Paragraph::new(action_lines(state)), rows[2]);
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
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}  ", combat.chara),
                    Style::default().fg(theme::ERROR()),
                ),
                Span::styled(
                    format!("{}/{}", fight.enemy_hp, fight.enemy_max),
                    Style::default().fg(theme::TEXT()),
                ),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("@  ", Style::default().fg(theme::AMBER())),
                Span::styled(format!("{hp}/{max}"), Style::default().fg(theme::TEXT())),
            ]));
            if let Some(last) = &fight.last_hit {
                lines.push(Line::from(Span::styled(
                    last.clone(),
                    Style::default().fg(theme::TEXT_DIM()),
                )));
            }
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
