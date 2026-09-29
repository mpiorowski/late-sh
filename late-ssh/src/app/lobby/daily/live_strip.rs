//! The #lounge live strip: one daily match painted at the top of the Home
//! chat card, for the room to see. The board (`live_board.rs`) sits in a
//! fixed column with the words beside it: the players, where the match
//! stands, what just happened, and how to watch. It is up only
//! while something just happened (`state.rs::refresh_live_strip`), so it
//! appearing is the news; it never draws `no games live`. Two fixed forms,
//! picked by the card's size and never by the match (`fit_live_strip`): the
//! eight board rows over a rule that parts them from the chat, or one row on
//! a card too small for that.

use std::cell::Cell;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::common::theme;
use crate::app::games::pool_core::{canvas::Rgb, rules::PoolRules};

use super::{
    checkers, connect4,
    live::{LiveBoard, LiveStripView, LiveView},
    live_board::{BOARD_ROWS, board_lines, canvas_background, live_compact_line},
    reversi,
};

/// The board rows plus the rule under them.
pub(crate) const LIVE_STRIP_HEIGHT: u16 = 1 + BOARD_ROWS;
/// The one-row form: the rule label, then the game and the players.
pub(crate) const LIVE_STRIP_COMPACT_HEIGHT: u16 = 1;
/// The board's column: wide enough for an eight-cell grid board at two
/// columns a cell, with a pool cloth fitted to the same width.
pub(crate) const BOARD_COLS: u16 = 21;
/// Narrower than this and the words beside the board have no room.
const MIN_FULL_WIDTH: u16 = 56;
/// Rows the messages keep under the full strip; under that the card takes
/// the one-row form, and under `MIN_COMPACT_HEIGHT` nothing.
const MESSAGE_ROWS_UNDER_FULL: u16 = 8;
const MIN_COMPACT_HEIGHT: u16 = LIVE_STRIP_COMPACT_HEIGHT + 4;
/// Columns between the board and the words.
const GAP: u16 = 2;

/// Which form the card fitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StripSize {
    Full,
    Compact,
}

/// Carve the strip off the top of a messages area: the form, its rect, and
/// what the messages keep. `None` when the area cannot spare a row.
pub(crate) fn fit_live_strip(area: Rect) -> Option<(StripSize, Rect, Rect)> {
    let size = if area.width >= MIN_FULL_WIDTH
        && area.height >= LIVE_STRIP_HEIGHT + MESSAGE_ROWS_UNDER_FULL
    {
        StripSize::Full
    } else if area.height >= MIN_COMPACT_HEIGHT {
        StripSize::Compact
    } else {
        return None;
    };
    let height = match size {
        StripSize::Full => LIVE_STRIP_HEIGHT,
        StripSize::Compact => LIVE_STRIP_COMPACT_HEIGHT,
    };
    let split = Layout::vertical([Constraint::Length(height), Constraint::Min(1)]).split(area);
    Some((size, split[0], split[1]))
}

pub(crate) fn draw_live_strip(
    frame: &mut Frame,
    area: Rect,
    size: StripSize,
    strip: &LiveStripView<'_>,
    hit: &Cell<Option<(Rect, Uuid)>>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // A finished match has left the lobby: nothing to open.
    if strip.finish.is_none() {
        hit.set(Some((area, strip.view.item.id)));
    }
    let lines = match size {
        StripSize::Full => live_strip_lines(area.width, strip, canvas_background()),
        StripSize::Compact => vec![live_strip_compact_line(area.width, strip)],
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// Always `LIVE_STRIP_HEIGHT` lines: the board in its column with the words
/// beside it, then the rule, which parts the strip from the chat below.
pub(crate) fn live_strip_lines(
    width: u16,
    strip: &LiveStripView<'_>,
    background: Rgb,
) -> Vec<Line<'static>> {
    let glow = strip.view.aim.is_some() || strip.finish.is_some();
    let mut lines = Vec::with_capacity(LIVE_STRIP_HEIGHT as usize);

    let mut board = board_lines(BOARD_COLS, &strip.view, background);
    board.truncate(BOARD_ROWS as usize);
    // Centre whatever the game drew in the fixed band.
    let top = (BOARD_ROWS as usize).saturating_sub(board.len()) / 2;
    let mut rows: Vec<Line<'static>> = (0..top).map(|_| Line::from("")).collect();
    rows.append(&mut board);
    while rows.len() < BOARD_ROWS as usize {
        rows.push(Line::from(""));
    }

    let budget = usize::from(width.saturating_sub(BOARD_COLS + GAP));
    let words = word_rows(budget, strip);
    for (row, words) in rows.into_iter().zip(words) {
        let mut spans = row.spans;
        let drawn: usize = spans.iter().map(|span| span.width()).sum();
        let pad = usize::from(BOARD_COLS + GAP).saturating_sub(drawn);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.extend(words);
        lines.push(Line::from(spans));
    }
    lines.push(rule_line(width, glow));
    lines
}

/// `── live ────`, the label lit while a cue is up or a result is in.
fn rule_line(width: u16, glow: bool) -> Line<'static> {
    let label_style = if glow {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::ITALIC)
    };
    let label = "live";
    let used = 3 + label.chars().count() + 1;
    let trail = usize::from(width).saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled("── ".to_string(), Style::default().fg(theme::BORDER_DIM())),
        Span::styled(label.to_string(), label_style),
        Span::raw(" "),
        Span::styled("─".repeat(trail), Style::default().fg(theme::BORDER_DIM())),
    ])
}

/// One row: the rule label, then the game and the players, or the result.
pub(crate) fn live_strip_compact_line(width: u16, strip: &LiveStripView<'_>) -> Line<'static> {
    let glow = strip.view.aim.is_some() || strip.finish.is_some();
    let mut rule = rule_line(width, glow);
    rule.spans.truncate(3);
    let used: usize = rule.spans.iter().map(|span| span.width()).sum();
    let rest = width.saturating_sub(used as u16);
    let mut spans = rule.spans;
    match strip.finish {
        Some(headline) => spans.push(Span::styled(
            truncate_chars(headline, usize::from(rest)),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        )),
        None => spans.extend(live_compact_line(rest, &strip.view).spans),
    }
    Line::from(spans)
}

/// The words beside the board, one entry per board row: the players, where
/// the match stands, what just happened, then how to watch.
fn word_rows(budget: usize, strip: &LiveStripView<'_>) -> Vec<Vec<Span<'static>>> {
    let view = &strip.view;
    let mut rows: Vec<Vec<Span<'static>>> = (0..BOARD_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = players_spans(budget, view);
    rows[2] = vec![Span::styled(
        truncate_chars(&standing_text(view), budget),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    rows[3] = vec![event_span(budget, strip)];
    rows[6] = vec![Span::styled(
        truncate_chars(
            if strip.finish.is_some() {
                "ctrl+g to play"
            } else {
                "o or click to watch"
            },
            budget,
        ),
        Style::default().fg(theme::TEXT_FAINT()),
    )];
    rows
}

fn name(username: &Option<String>) -> String {
    username.clone().unwrap_or_else(|| "player".to_string())
}

/// `eggy · weslin`, the player on the move in amber.
fn players_spans(budget: usize, view: &LiveView<'_>) -> Vec<Span<'static>> {
    let item = view.item;
    let each = budget.saturating_sub(3) / 2;
    let styled = |user_id: Uuid, username: &Option<String>| {
        let style = if item.turn_user_id == Some(user_id) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        Span::styled(truncate_chars(&name(username), each), style)
    };
    vec![
        styled(item.challenger_id, &item.challenger_username),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
        styled(item.opponent_id, &item.opponent_username),
    ]
}

/// `Chess · move 12`, `Snooker · 34-12`, `Eight-Ball · shot 5`.
fn standing_text(view: &LiveView<'_>) -> String {
    let game = view.item.game.display_name();
    match view.board {
        LiveBoard::Pool {
            rules: PoolRules::Snooker,
            scores,
            ..
        } => format!("{game} · {}-{}", scores[0], scores[1]),
        LiveBoard::Pool { .. } => format!("{game} · shot {}", view.item.move_count),
        LiveBoard::Chess { .. }
        | LiveBoard::Battleship { .. }
        | LiveBoard::ConnectFour { .. }
        | LiveBoard::Reversi { .. }
        | LiveBoard::Checkers { .. }
        | LiveBoard::Backgammon { .. }
        | LiveBoard::Briscola { .. } => format!("{game} · move {}", view.item.move_count),
    }
}

/// What just happened: the result, the shooter lining up, or the last move.
fn event_span(budget: usize, strip: &LiveStripView<'_>) -> Span<'static> {
    if let Some(headline) = strip.finish {
        return Span::styled(
            truncate_chars(headline, budget),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        );
    }
    let view = &strip.view;
    if view.aim.is_some() {
        return Span::styled(
            truncate_chars(
                &format!("{} is lining up a shot", name(on_turn(view))),
                budget,
            ),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        );
    }
    Span::styled(
        truncate_chars(&last_event_text(view), budget),
        Style::default().fg(theme::TEXT()),
    )
}

fn on_turn<'a>(view: &'a LiveView<'_>) -> &'a Option<String> {
    let item = view.item;
    if item.turn_user_id == Some(item.opponent_id) {
        &item.opponent_username
    } else {
        &item.challenger_username
    }
}

/// The player who is not on the move: the last mover in a game whose turns
/// strictly alternate (chess, connect four, checkers). Reversi passes and
/// pool keeps the shooter after a pot, so those name nobody.
fn last_mover<'a>(view: &'a LiveView<'_>) -> &'a Option<String> {
    let item = view.item;
    if item.turn_user_id == Some(item.challenger_id) {
        &item.opponent_username
    } else {
        &item.challenger_username
    }
}

/// `a1`, from `rank * 8 + file`.
fn square(index: usize) -> String {
    let file = char::from(b'a' + (index % 8) as u8);
    format!("{file}{}", index / 8 + 1)
}

fn last_event_text(view: &LiveView<'_>) -> String {
    const FIRST_MOVE: &str = "waiting for the first move";
    match view.board {
        LiveBoard::Chess {
            last: Some((from, to)),
            ..
        } => format!(
            "{} played {}{}",
            name(last_mover(view)),
            square(*from),
            square(*to)
        ),
        LiveBoard::ConnectFour {
            last: Some((_, col)),
            ..
        } => format!(
            "{} dropped in {}",
            name(last_mover(view)),
            connect4::column_label(*col)
        ),
        LiveBoard::Checkers {
            last: Some((row, col)),
            ..
        } => format!(
            "{} played {}",
            name(last_mover(view)),
            checkers::cell_label(*row, *col)
        ),
        LiveBoard::Reversi {
            last: Some((row, col)),
            ..
        } => format!("last move {}", reversi::cell_label(*row, *col)),
        LiveBoard::Pool {
            last: Some(last), ..
        } => last.clone(),
        LiveBoard::Pool { last: None, .. } => "waiting for the break".to_string(),
        LiveBoard::Chess { last: None, .. }
        | LiveBoard::ConnectFour { last: None, .. }
        | LiveBoard::Checkers { last: None, .. }
        | LiveBoard::Reversi { last: None, .. } => FIRST_MOVE.to_string(),
        LiveBoard::Battleship { .. }
        | LiveBoard::Backgammon { .. }
        | LiveBoard::Briscola { .. } => {
            if view.item.move_count == 0 {
                FIRST_MOVE.to_string()
            } else {
                format!("{} to move", name(on_turn(view)))
            }
        }
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "live_strip_test.rs"]
mod live_strip_test;
