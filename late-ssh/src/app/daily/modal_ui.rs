//! Lobby modal (daily games): your matches + the open lobby in one
//! scrollable list. All daily-games interaction happens here; the sidebar
//! panel is passive.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    common::theme,
    daily::{
        games::DailyGame,
        state::{ChallengeDraft, DailyState, format_deadline},
        svc::{DailyChallengeItem, DailyMatchItem},
    },
    games::chess_core::types::ChessColor,
};

// Near-fullscreen: daily games are a primary destination, not a peek. A
// margin keeps the screen behind visible so it still reads as a modal; caps
// keep line lengths sane on very large terminals.
const MODAL_MAX_WIDTH: u16 = 100;
const MODAL_MAX_HEIGHT: u16 = 40;
const MODAL_H_MARGIN: u16 = 8;
const MODAL_V_MARGIN: u16 = 4;

pub(crate) fn draw(frame: &mut Frame, area: Rect, daily: &DailyState) {
    let width = area
        .width
        .saturating_sub(MODAL_H_MARGIN)
        .min(MODAL_MAX_WIDTH);
    let height = area
        .height
        .saturating_sub(MODAL_V_MARGIN)
        .min(MODAL_MAX_HEIGHT);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Lobby ")
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
        Constraint::Fill(1),   // list
        Constraint::Length(1), // status / prompt
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_list(frame, layout[0], daily);
    draw_status(frame, layout[1], daily);
    draw_footer(frame, layout[2], daily);

    if let Some(draft) = &daily.challenge_draft {
        draw_draft_overlay(frame, popup, draft);
    }
}

fn draw_list(frame: &mut Frame, area: Rect, daily: &DailyState) {
    let matches = daily.my_matches();
    let lobby = daily.lobby();
    let width = area.width as usize;

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(section_line(width, "your matches"));
    if matches.is_empty() {
        lines.push(empty_line("no active matches"));
    }
    for (idx, item) in matches.iter().enumerate() {
        lines.push(match_line(daily, item, daily.selected == idx));
    }
    lines.push(Line::raw(""));
    lines.push(section_line(width, "lobby"));
    if lobby.is_empty() {
        lines.push(empty_line("no open challenges · post one with c"));
    }
    for (idx, challenge) in lobby.iter().enumerate() {
        lines.push(challenge_line(
            daily,
            challenge,
            daily.selected == matches.len() + idx,
        ));
    }

    // Keep the selected row in view on small terminals: scroll whole lines.
    let budget = area.height as usize;
    if lines.len() > budget {
        let selected_line = selected_line_index(daily, matches.len());
        let skip = selected_line.saturating_sub(budget.saturating_sub(1));
        lines.drain(..skip);
        lines.truncate(budget);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Line index of the selected entry inside the built list (headers offset).
fn selected_line_index(daily: &DailyState, match_count: usize) -> usize {
    if daily.selected < match_count {
        1 + daily.selected
    } else {
        // matches header + rows (or empty row) + blank + lobby header
        let match_rows = match_count.max(1);
        1 + match_rows + 2 + (daily.selected - match_count)
    }
}

fn match_line(daily: &DailyState, item: &DailyMatchItem, selected: bool) -> Line<'static> {
    let (_, opponent) = daily.opponent_of(item);
    let opponent = opponent.unwrap_or_else(|| "player".to_string());
    let my_turn = daily.my_turn(item);
    let deadline = item
        .turn_deadline_at
        .map(|at| format_deadline(at, Utc::now()))
        .unwrap_or_else(|| "--".to_string());
    let progress = match item.game {
        DailyGame::Chess => {
            let color = if item.white_id == Some(daily.user_id()) {
                ChessColor::White
            } else {
                ChessColor::Black
            };
            format!(
                "{} · {} moves",
                color.label().to_lowercase(),
                item.move_count
            )
        }
        DailyGame::Battleship => format!("{} shots", item.move_count),
        DailyGame::ConnectFour => format!("{} drops", item.move_count),
    };

    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        format!("{opponent:<16}"),
        Style::default().fg(if my_turn {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT()
        }),
    ));
    spans.push(Span::styled(
        format!("{:<12}", item.game.label()),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::styled(
        format!("{progress:<18}"),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    if my_turn {
        spans.push(Span::styled(
            format!("your turn · {deadline}"),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        spans.push(Span::styled(
            format!("waiting · {deadline}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    Line::from(spans)
}

fn challenge_line(
    daily: &DailyState,
    challenge: &DailyChallengeItem,
    selected: bool,
) -> Line<'static> {
    let mine = challenge.challenger_id == daily.user_id();
    let poster = if mine {
        "you".to_string()
    } else {
        challenge
            .challenger_username
            .clone()
            .unwrap_or_else(|| "player".to_string())
    };
    let target = match (challenge.target_user_id, &challenge.target_username) {
        (Some(id), name) if id == daily.user_id() => {
            let _ = name;
            Some("you".to_string())
        }
        (Some(_), Some(name)) => Some(format!("@{name}")),
        (Some(_), None) => Some("@player".to_string()),
        (None, _) => None,
    };

    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        format!("{poster:<16}"),
        Style::default().fg(if mine {
            theme::TEXT_DIM()
        } else {
            theme::TEXT()
        }),
    ));
    spans.push(Span::styled(
        format!("{:<12}", challenge.game.label()),
        Style::default().fg(theme::TEXT()),
    ));
    match target {
        Some(target) => spans.push(Span::styled(
            format!("{:<18}", format!("challenges {target}")),
            Style::default().fg(theme::AMBER_DIM()),
        )),
        None => spans.push(Span::styled(
            format!("{:<18}", "open challenge"),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    }
    spans.push(Span::styled(
        format!("{} chips to the winner", challenge.game.win_payout()),
        Style::default().fg(theme::AMBER_DIM()),
    ));
    if mine {
        spans.push(Span::styled(
            "   yours · x cancel",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ));
    }
    Line::from(spans)
}

fn draw_status(frame: &mut Frame, area: Rect, daily: &DailyState) {
    let line = if daily.confirm_claim.is_some() {
        Line::from(Span::styled(
            "claim this challenge and start the match? enter confirm · esc back",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
        .centered()
    } else {
        Line::from(Span::styled(
            format!(
                "entries {}/{} · 24h per move · winner takes the chip payout",
                daily.entry_count(),
                daily.entry_cap()
            ),
            Style::default().fg(theme::TEXT_FAINT()),
        ))
        .centered()
    };
    frame.render_widget(Paragraph::new(line), area);
}

// The challenge picker overlay: a small modal over the Lobby list, one row
// per roster game with its prize. The height follows the roster, so new
// games grow the box instead of fighting the status line for width.
// Directed drafts swap to a username step.
const DRAFT_WIDTH: u16 = 48;

fn draw_draft_overlay(frame: &mut Frame, popup: Rect, draft: &ChallengeDraft) {
    // A leading blank row + the body + a blank row before the key hints.
    let body_rows = if draft.username.is_some() {
        5
    } else {
        DailyGame::ALL.len() as u16 + 3
    };
    let width = DRAFT_WIDTH.min(popup.width);
    let height = (body_rows + 2).min(popup.height);
    let rect = centered_rect(width, height, popup);
    frame.render_widget(Clear, rect);

    let title = if draft.username.is_some() {
        " challenge a player "
    } else {
        " new challenge "
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<Line<'static>> = vec![Line::raw("")];
    match &draft.username {
        None => {
            for (idx, game) in DailyGame::ALL.into_iter().enumerate() {
                let selected = idx == draft.selected;
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(selected),
                    Span::styled(
                        format!("{:<14}", game.label()),
                        Style::default().fg(if selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT()
                        }),
                    ),
                    Span::styled(
                        format!("{:>4} chips to the winner", game.win_payout()),
                        Style::default().fg(if selected {
                            theme::AMBER_DIM()
                        } else {
                            theme::TEXT_FAINT()
                        }),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            let post = if draft.directed { " next" } else { " post" };
            lines.push(Line::from(vec![
                Span::raw(" "),
                key("j/k"),
                text(" choose"),
                gap(),
                key("enter"),
                text(post),
                gap(),
                key("esc"),
                text(" back"),
            ]));
        }
        Some(buffer) => {
            lines.push(Line::from(Span::styled(
                format!(
                    "   {} · {} chips to the winner",
                    draft.game().label(),
                    draft.game().win_payout()
                ),
                Style::default().fg(theme::TEXT_DIM()),
            )));
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    "@",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(buffer.clone(), Style::default().fg(theme::TEXT_BRIGHT())),
                Span::styled("█", Style::default().fg(theme::AMBER_GLOW())),
            ]));
            lines.push(Line::from(vec![
                Span::raw(" "),
                key("enter"),
                text(" send"),
                gap(),
                key("esc"),
                text(" back"),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_footer(frame: &mut Frame, area: Rect, daily: &DailyState) {
    let mut spans = vec![
        key("j/k"),
        text(" move"),
        gap(),
        key("enter"),
        text(" open / claim"),
        gap(),
        key("c"),
        text(" challenge"),
        gap(),
        key("C"),
        text(" directed"),
        gap(),
    ];
    if matches!(
        daily.selected_entry(),
        Some(crate::app::daily::state::DailyModalEntry::Challenge(challenge))
            if challenge.challenger_id == daily.user_id()
    ) {
        spans.push(key("x"));
        spans.push(text(" cancel"));
        spans.push(gap());
    }
    spans.push(key("esc"));
    spans.push(text(" close"));
    frame.render_widget(Paragraph::new(Line::from(spans)).centered(), area);
}

fn marker_span(selected: bool) -> Span<'static> {
    if selected {
        Span::styled(
            "► ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("  ")
    }
}

fn section_line(width: usize, label: &str) -> Line<'static> {
    let used = 3 + label.chars().count() + 1;
    let trail = width.saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled("── ".to_string(), Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ),
        Span::raw(" "),
        Span::styled("─".repeat(trail), Style::default().fg(theme::BORDER_DIM())),
    ])
}

fn empty_line(message: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {message}"),
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ))
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
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
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
