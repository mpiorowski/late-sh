//! A daily match on the live strip (`app/live/`): the board
//! (`live_board.rs`) in the picture column, and the words beside it: the
//! players, where the match stands, what just happened, and how to watch.
//! The strip's frame, its two forms and its rule are `app/live/ui.rs`.

use ratatui::{
    style::{Modifier, Style},
    text::Span,
};
use uuid::Uuid;

use crate::app::common::theme;
use crate::app::games::pool_core::{canvas::Rgb, rules::PoolRules};
use crate::app::live::ui::{
    HintPart, PICTURE_COLS, PICTURE_ROWS, StripBody, key_hint_spans, truncate_chars,
};

use super::{
    backgammon, battleship, checkers, connect4,
    gin::{GinMove, HandEnd, Pile},
    live::{LiveBoard, LiveGinEvent, LiveView, MatchStripView},
    live_board::{board_lines, live_compact_line, player_marks, top_seat},
    reversi,
    svc::DailyMatchItem,
};

/// What the strip paints for a match. The label glows while a cue is up or
/// a result is in.
pub(crate) fn body(budget: usize, strip: &MatchStripView<'_>, background: Rgb) -> StripBody {
    StripBody {
        picture: board_lines(PICTURE_COLS, &strip.view, background),
        words: word_rows(budget, strip),
        glow: glow(strip),
    }
}

pub(crate) fn glow(strip: &MatchStripView<'_>) -> bool {
    strip.view.aim.is_some() || strip.finish.is_some()
}

/// The one-row form, after the rule label: the game and the players, or the
/// result.
pub(crate) fn compact_spans(rest: u16, strip: &MatchStripView<'_>) -> Vec<Span<'static>> {
    match strip.finish {
        Some(headline) => vec![Span::styled(
            truncate_chars(headline, usize::from(rest)),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        )],
        None => live_compact_line(rest, &strip.view).spans,
    }
}

/// The words beside the board, one entry per board row: the players, where
/// the match stands, what just happened, then how to watch.
fn word_rows(budget: usize, strip: &MatchStripView<'_>) -> Vec<Vec<Span<'static>>> {
    let view = &strip.view;
    let mut rows: Vec<Vec<Span<'static>>> = (0..PICTURE_ROWS).map(|_| Vec::new()).collect();
    if budget == 0 {
        return rows;
    }
    rows[1] = players_spans(budget, view);
    rows[2] = vec![Span::styled(
        truncate_chars(&standing_text(view), budget),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    rows[3] = vec![event_span(budget, strip)];
    rows[6] = match strip.finish {
        Some(_) => key_hint_spans(
            budget,
            &[
                HintPart::Text("press "),
                HintPart::Key("ctrl+g"),
                HintPart::Text(" to play"),
            ],
        ),
        None => key_hint_spans(
            budget,
            &[
                HintPart::Text("press "),
                HintPart::Key("o"),
                HintPart::Text(" or click to watch"),
            ],
        ),
    };
    rows
}

fn name(username: &Option<String>) -> String {
    username.clone().unwrap_or_else(|| "player".to_string())
}

/// `eggy · weslin`, the player on the move in amber.
fn players_spans(budget: usize, view: &LiveView<'_>) -> Vec<Span<'static>> {
    let item = view.item;
    let marks = player_marks(view);
    let mark_width = match &marks {
        Some(marks) => marks[0].width(),
        None => 0,
    };
    let each = (budget.saturating_sub(3) / 2).saturating_sub(mark_width);
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
    let challenger = styled(item.challenger_id, &item.challenger_username);
    let opponent = styled(item.opponent_id, &item.opponent_username);
    let dot = Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT()));
    match marks {
        Some([challenger_mark, opponent_mark]) => {
            vec![challenger_mark, challenger, dot, opponent_mark, opponent]
        }
        None => vec![challenger, dot, opponent],
    }
}

/// A per-seat pair in the words' order: challenger, then opponent.
fn by_player<T: Copy>(item: &DailyMatchItem, seat0_id: Uuid, pair: [T; 2]) -> [T; 2] {
    let top = top_seat(item, seat0_id);
    [pair[top], pair[1 - top]]
}

/// The name in a seat.
fn seat_name(item: &DailyMatchItem, seat0_id: Uuid, seat: usize) -> &Option<String> {
    if seat == top_seat(item, seat0_id) {
        &item.challenger_username
    } else {
        &item.opponent_username
    }
}

/// `Chess · move 12`, `Snooker · 34-12`, `Eight-Ball · shot 5`,
/// `Backgammon · pips 167-160`. Every pair reads challenger first, as the
/// players do.
fn standing_text(view: &LiveView<'_>) -> String {
    let item = view.item;
    let game = item.game.display_name();
    match view.board {
        LiveBoard::Pool {
            rules: PoolRules::Snooker,
            scores,
            seat0_id,
            ..
        } => {
            let [a, b] = by_player(item, *seat0_id, *scores);
            format!("{game} · {a}-{b}")
        }
        LiveBoard::Pool { .. } => format!("{game} · shot {}", item.move_count),
        LiveBoard::Backgammon {
            white_id, board, ..
        } => {
            let white = board.pip_count(backgammon::Color::White);
            let red = board.pip_count(backgammon::Color::Red);
            let (a, b) = if *white_id == item.challenger_id {
                (white, red)
            } else {
                (red, white)
            };
            format!("{game} · pips {a}-{b}")
        }
        LiveBoard::Briscola {
            seat0_id, points, ..
        } => {
            let [a, b] = by_player(item, *seat0_id, *points);
            format!("{game} · {a}-{b}")
        }
        LiveBoard::Cribbage {
            seat0_id,
            scores,
            hand,
            ..
        }
        | LiveBoard::GinRummy {
            seat0_id,
            scores,
            hand,
            ..
        } => {
            let [a, b] = by_player(item, *seat0_id, *scores);
            format!("{game} · {a}-{b} · hand {hand}")
        }
        LiveBoard::Chess { .. }
        | LiveBoard::Battleship { .. }
        | LiveBoard::ConnectFour { .. }
        | LiveBoard::Reversi { .. }
        | LiveBoard::Checkers { .. } => format!("{game} · move {}", item.move_count),
    }
}

/// What just happened: the result, the shooter lining up, or the last move.
fn event_span(budget: usize, strip: &MatchStripView<'_>) -> Span<'static> {
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
        LiveBoard::Battleship {
            side0_id,
            last: Some((shooter, shot)),
            ..
        } => {
            let verb = if shot.hit { "hit" } else { "missed" };
            format!(
                "{} {verb} {}",
                name(seat_name(view.item, *side0_id, *shooter)),
                battleship::cell_label(shot.cell as usize)
            )
        }
        LiveBoard::Briscola {
            seat0_id,
            last: Some((seat, card)),
            ..
        } => format!(
            "{} played {}",
            name(seat_name(view.item, *seat0_id, *seat)),
            card.label()
        ),
        LiveBoard::GinRummy {
            seat0_id,
            last: LiveGinEvent::Move { seat, played },
            ..
        } => {
            let what = match played {
                GinMove::Draw(Pile::Stock) => "drew from the stock".to_string(),
                GinMove::Draw(Pile::Discard) => "took from the pile".to_string(),
                GinMove::Discard { card, knock: false } => format!("threw {}", card.label()),
                GinMove::Discard { card, knock: true } => format!("knocked on {}", card.label()),
            };
            format!("{} {what}", name(seat_name(view.item, *seat0_id, *seat)))
        }
        LiveBoard::GinRummy {
            seat0_id,
            last: LiveGinEvent::HandOver { end, scored },
            ..
        } => {
            let scorer = |seat: usize| name(seat_name(view.item, *seat0_id, seat));
            match (end, scored) {
                (HandEnd::Gin, Some((seat, points))) => {
                    format!("{} went gin · +{points}", scorer(*seat))
                }
                (HandEnd::Knock, Some((seat, points))) => {
                    format!("{} knocked · +{points}", scorer(*seat))
                }
                (HandEnd::Undercut, Some((seat, points))) => {
                    format!("{} undercut · +{points}", scorer(*seat))
                }
                (HandEnd::Dead, _) | (_, None) => "dead hand, no score".to_string(),
            }
        }
        LiveBoard::Battleship { last: None, .. }
        | LiveBoard::Briscola { last: None, .. }
        | LiveBoard::GinRummy {
            last: LiveGinEvent::Dealt,
            ..
        }
        | LiveBoard::Backgammon { .. }
        | LiveBoard::Cribbage { .. } => {
            if view.item.move_count == 0 {
                FIRST_MOVE.to_string()
            } else {
                format!("{} to move", name(on_turn(view)))
            }
        }
    }
}

#[cfg(test)]
#[path = "live_strip_test.rs"]
mod live_strip_test;
