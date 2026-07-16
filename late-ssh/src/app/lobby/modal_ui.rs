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
    games::chess_core::types::ChessColor,
    lobby::daily::{
        games::DailyGame,
        state::{ChallengeDraft, DailyState, format_deadline, result_phrase},
        svc::{DailyChallengeItem, DailyFinishedItem, DailyMatchItem, DailyOutcome},
    },
    lobby::house::{state::HouseState, tables::HouseTable},
    lobby::state::{LobbyEntry, LobbyState},
};

// Near-fullscreen: daily games are a primary destination, not a peek. A
// margin keeps the screen behind visible so it still reads as a modal; caps
// keep line lengths sane on very large terminals.
const MODAL_MAX_WIDTH: u16 = 100;
const MODAL_MAX_HEIGHT: u16 = 40;
const MODAL_H_MARGIN: u16 = 8;
const MODAL_V_MARGIN: u16 = 4;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    lobby: &LobbyState,
    daily: &DailyState,
    house: &HouseState,
) {
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

    draw_list(frame, layout[0], lobby, daily, house);
    draw_status(frame, layout[1], lobby, daily);
    draw_footer(frame, layout[2], lobby, daily);

    if let Some(draft) = &daily.challenge_draft {
        draw_draft_overlay(frame, popup, draft);
    }
}

fn draw_list(
    frame: &mut Frame,
    area: Rect,
    lobby_state: &LobbyState,
    daily: &DailyState,
    house: &HouseState,
) {
    let finished = daily.my_finished();
    let matches = daily.my_matches();
    let lobby = daily.lobby();
    let live = daily.live_games();
    let width = area.width as usize;

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(section_line(width, "your matches"));
    if finished.is_empty() && matches.is_empty() {
        lines.push(empty_line("no active matches"));
    }
    // Unseen results first: transient news rows that clear once looked at.
    for (idx, item) in finished.iter().enumerate() {
        lines.push(finished_line(daily, item, lobby_state.selected == idx));
    }
    for (idx, item) in matches.iter().enumerate() {
        lines.push(match_line(
            daily,
            item,
            lobby_state.selected == finished.len() + idx,
        ));
    }
    lines.push(Line::raw(""));
    lines.push(section_line(width, "lobby"));
    if lobby.is_empty() {
        lines.push(empty_line("no open challenges · post one with c"));
    }
    let lobby_base = finished.len() + matches.len();
    for (idx, challenge) in lobby.iter().enumerate() {
        lines.push(challenge_line(
            daily,
            challenge,
            lobby_state.selected == lobby_base + idx,
        ));
    }
    // Other people's games in progress: watch-only, header hidden when none.
    if !live.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section_line(width, "live games"));
        for (idx, item) in live.iter().enumerate() {
            lines.push(spectate_line(
                item,
                lobby_state.selected == lobby_base + lobby.len() + idx,
            ));
        }
    }
    // The fixed house tables: always present (stable chrome), one row per
    // roster variant, live occupancy from the singleton services.
    lines.push(Line::raw(""));
    lines.push(section_line(width, "house tables"));
    let house_base = lobby_base + lobby.len() + live.len();
    for (idx, table) in HouseTable::ALL.into_iter().enumerate() {
        lines.push(house_line(
            table,
            house.registry().occupancy(table).label(),
            lobby_state.selected == house_base + idx,
        ));
    }

    // Keep the selected row in view on small terminals: scroll whole lines.
    let budget = area.height as usize;
    if lines.len() > budget {
        let selected_line =
            selected_line_index(lobby_state.selected, lobby_base, lobby.len(), live.len());
        let skip = selected_line.saturating_sub(budget.saturating_sub(1));
        lines.drain(..skip);
        lines.truncate(budget);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// A fixed house table: name, pitch, live occupancy. Enter opens the table.
fn house_line(table: HouseTable, occupancy: String, selected: bool) -> Line<'static> {
    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        col(table.display_name(), NAME_COL),
        Style::default().fg(if selected {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT()
        }),
    ));
    spans.push(Span::styled(
        col(table.tagline(), GAME_COL + DETAIL_COL),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    let occupied = !occupancy.starts_with("empty");
    spans.push(Span::styled(
        occupancy,
        Style::default().fg(if occupied {
            theme::AMBER()
        } else {
            theme::TEXT_FAINT()
        }),
    ));
    Line::from(spans)
}

/// Line index of the selected entry inside the built list (headers offset).
/// `mine_count` is the "your matches" section's row count: unseen results
/// plus active matches.
fn selected_line_index(
    selected: usize,
    mine_count: usize,
    lobby_count: usize,
    live_count: usize,
) -> usize {
    if selected < mine_count {
        return 1 + selected;
    }
    // section header + rows (or empty row) + blank + lobby header
    let mine_rows = mine_count.max(1);
    let lobby_base = 1 + mine_rows + 2;
    let after_mine = selected - mine_count;
    if after_mine < lobby_count {
        return lobby_base + after_mine;
    }
    // lobby rows (or empty row) + blank + live-games header (only when the
    // live section is drawn at all)
    let lobby_rows = lobby_count.max(1);
    let live_base = lobby_base + lobby_rows + 2;
    let after_lobby = after_mine - lobby_count;
    if after_lobby < live_count {
        return live_base + after_lobby;
    }
    // live rows (when present) + blank + house-tables header
    let house_base = if live_count > 0 {
        live_base + live_count + 2
    } else {
        lobby_base + lobby_rows + 2
    };
    house_base + (after_lobby - live_count)
}

// List rows are a fixed grid: marker, opponent, game, detail, status. `{:<N}`
// alone pads but never truncates, so a long username would swallow the gap
// and run columns together; `col` truncates with an ellipsis and always
// leaves at least one space before the next column.
const NAME_COL: usize = 16;
const GAME_COL: usize = 12;
const DETAIL_COL: usize = 25;

fn col(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < width {
        return format!("{text:<width$}");
    }
    let mut out: String = chars.into_iter().take(width.saturating_sub(2)).collect();
    out.push('…');
    out.push(' ');
    out
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
        DailyGame::Reversi | DailyGame::Checkers => format!("{} moves", item.move_count),
    };

    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        col(&opponent, NAME_COL),
        Style::default().fg(if my_turn {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT()
        }),
    ));
    spans.push(Span::styled(
        col(item.game.label(), GAME_COL),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::styled(
        col(&progress, DETAIL_COL),
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

/// An unseen result: who you played, how it ended, how to clear it. Lingers
/// until acknowledged (enter the board and leave, or `x`).
fn finished_line(daily: &DailyState, item: &DailyFinishedItem, selected: bool) -> Line<'static> {
    let (_, opponent) = item.opponent_of(daily.user_id());
    let opponent = opponent.unwrap_or_else(|| "player".to_string());
    let (outcome, style) = match item.outcome_for(daily.user_id()) {
        DailyOutcome::Won => (
            format!("you won · {}", result_phrase(&item.result)),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ),
        DailyOutcome::Lost => (
            format!("you lost · {}", result_phrase(&item.result)),
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ),
        DailyOutcome::Draw => (
            "draw".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
    };

    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        col(&opponent, NAME_COL),
        Style::default().fg(theme::TEXT_BRIGHT()),
    ));
    spans.push(Span::styled(
        col(item.game.label(), GAME_COL),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::styled(col(&outcome, DETAIL_COL), style));
    spans.push(Span::styled(
        "enter view · x dismiss",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ));
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
        col(&poster, NAME_COL),
        Style::default().fg(if mine {
            theme::TEXT_DIM()
        } else {
            theme::TEXT()
        }),
    ));
    spans.push(Span::styled(
        col(challenge.game.label(), GAME_COL),
        Style::default().fg(theme::TEXT()),
    ));
    match target {
        Some(target) => spans.push(Span::styled(
            col(&format!("challenges {target}"), DETAIL_COL),
            Style::default().fg(theme::AMBER_DIM()),
        )),
        None => spans.push(Span::styled(
            col("open challenge", DETAIL_COL),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    }
    spans.push(Span::styled(
        format!("{} chips", challenge.game.win_payout()),
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

/// A game in progress between two other people: neutral third-party view,
/// no "you", no deadline pressure — just who's playing and whose move it is.
fn spectate_line(item: &DailyMatchItem, selected: bool) -> Line<'static> {
    let challenger = item
        .challenger_username
        .clone()
        .unwrap_or_else(|| "player".to_string());
    let opponent = item
        .opponent_username
        .clone()
        .unwrap_or_else(|| "player".to_string());
    let progress = match item.game {
        DailyGame::Chess => format!("{} moves", item.move_count),
        DailyGame::Battleship => format!("{} shots", item.move_count),
        DailyGame::ConnectFour => format!("{} drops", item.move_count),
        DailyGame::Reversi | DailyGame::Checkers => format!("{} moves", item.move_count),
    };
    // The versus pair is two usernames wide, so it takes the name and game
    // columns together and the game rides with the progress instead.
    let detail = format!("{} · {}", item.game.label(), progress);
    let to_move = match item.turn_user_id {
        Some(id) if id == item.challenger_id => format!("{challenger} to move"),
        Some(id) if id == item.opponent_id => format!("{opponent} to move"),
        _ => "in progress".to_string(),
    };
    let deadline = item
        .turn_deadline_at
        .map(|at| format!(" · {}", format_deadline(at, Utc::now())))
        .unwrap_or_default();

    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        col(&format!("{challenger} v {opponent}"), NAME_COL + GAME_COL),
        Style::default().fg(theme::TEXT()),
    ));
    spans.push(Span::styled(
        col(&detail, DETAIL_COL),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::styled(
        format!("{to_move}{deadline}"),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    Line::from(spans)
}

fn draw_status(frame: &mut Frame, area: Rect, lobby: &LobbyState, daily: &DailyState) {
    let line = if lobby.confirm_claim.is_some() {
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
                        format!("{:>4} chips", game.win_payout()),
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
                    "   {} · {} chips",
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

fn draw_footer(frame: &mut Frame, area: Rect, lobby: &LobbyState, daily: &DailyState) {
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
    match lobby.selected_entry(daily) {
        Some(LobbyEntry::Challenge(challenge)) if challenge.challenger_id == daily.user_id() => {
            spans.push(key("x"));
            spans.push(text(" cancel"));
            spans.push(gap());
        }
        Some(LobbyEntry::Finished(_)) => {
            spans.push(key("x"));
            spans.push(text(" dismiss"));
            spans.push(gap());
        }
        _ => {}
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

#[cfg(test)]
mod tests {
    use super::col;

    #[test]
    fn col_pads_short_text_to_width() {
        assert_eq!(col("chess", 8), "chess   ");
    }

    #[test]
    fn col_always_leaves_a_gap_before_the_next_column() {
        // Exactly width chars would swallow the separator; the longest
        // fitting content is width - 1 chars plus one space.
        assert_eq!(col("12345678", 8), "123456… ");
        assert_eq!(col("1234567", 8), "1234567 ");
        assert_eq!(col("challenges @kirii.md", 18), "challenges @kiri… ");
    }

    #[test]
    fn col_counts_chars_not_bytes() {
        assert_eq!(col("héllo", 8), "héllo   ");
    }
}
