//! Full-screen daily cribbage board: your hand fanned across the bottom, the
//! opponent's as backs along the top, and between them the starter, the crib
//! and the cards on the current count. The player bars double as the peg
//! board, a track to `WINNING_SCORE`. The rail keeps this hand's pegging and the last
//! show, counted out the way a player says it aloud.
//!
//! This is the file that keeps cribbage's hidden information hidden. The
//! match state holds both hands, the crib, and the starter before it is cut
//! (see `cribbage.rs`), so faces are drawn for the viewer's own hand only,
//! the crib stays face down until the show, and a spectator gets no hand
//! faces at all. What the table makes public is drawn for everyone: the
//! starter once cut, every pegged card, and the show.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::{
    common::theme,
    lobby::daily::{
        board_ui::{draw_center_message, name_for, result_banner},
        cribbage::{self, DailyCribbageState, Phase, Table, Why, other},
        hand_ui::{self, CardSlots, Face, Tier},
        state::{CribbageDetail, DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
        std_deck::Card,
    },
};

/// status + two player bars + key hints around the table.
const CHROME_ROWS: u16 = 4;
const INFO_RAIL_WIDTH: u16 = 28;
/// Breathing room required around the table before the rail appears.
const INFO_RAIL_MIN_EXTRA: u16 = 6;
/// The most cards one count can hold (four aces and four twos make twelve).
const MAX_ON_COUNT: usize = 8;
/// Columns between the starter, the crib, and the count.
const GAP: u16 = 2;

/// opponent hand + gap + middle row + its label + gap + own hand + the
/// cursor marker row.
fn table_rows(tier: Tier) -> u16 {
    tier.card_h() * 3 + 4
}

/// The middle row is the widest: starter, crib, and a full count.
fn table_width(tier: Tier) -> u16 {
    tier.card_w() * 2 + GAP * 2 + tier.fan_width(MAX_ON_COUNT)
}

/// Where the viewer's hand starts inside the table.
fn hand_left(tier: Tier, count: usize) -> u16 {
    table_width(tier).saturating_sub(tier.fan_width(count)) / 2
}

/// The row of the viewer's hand inside the table.
fn hand_top(tier: Tier) -> u16 {
    tier.card_h() * 2 + 3
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    cribbage: &CribbageDetail,
) {
    let tier = if area.height >= table_rows(Tier::Full) + CHROME_ROWS
        && area.width >= table_width(Tier::Full)
    {
        Tier::Full
    } else {
        Tier::Compact
    };
    if area.width < table_width(tier) || area.height < table_rows(tier) + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }

    let state = &cribbage.state;
    let table = state.table();
    // A spectator holds no seat; show them the match from seat 0's side,
    // with both hands face down.
    let my_seat = state.seat_of(daily.user_id()).unwrap_or(0);

    let show_rail = area.width >= table_width(tier) + INFO_RAIL_WIDTH + INFO_RAIL_MIN_EXTRA;
    let area = if show_rail {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_RAIL_WIDTH)])
            .split(area);
        draw_info_rail(frame, cols[1], daily, board, state, &table);
        cols[0]
    } else {
        area
    };

    let top_pad = area.height.saturating_sub(table_rows(tier) + CHROME_ROWS) / 2;
    let rows = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(1),                // status
        Constraint::Length(1),                // opponent bar
        Constraint::Length(table_rows(tier)), // the table
        Constraint::Length(1),                // own bar
        Constraint::Min(0),                   // slack, pushing the hints down
        Constraint::Length(1),                // key hints
    ])
    .split(area);
    let (status_row, top_bar, table_row, bottom_bar, hint_row) =
        (rows[1], rows[2], rows[3], rows[4], rows[6]);

    let my_turn = detail.is_active()
        && detail.row.turn_user_id == Some(daily.user_id())
        && !cribbage.move_in_flight
        && !board.spectating;

    let table_rect = Rect {
        x: table_row.x + table_row.width.saturating_sub(table_width(tier)) / 2,
        y: table_row.y,
        width: table_width(tier),
        height: table_rows(tier),
    };
    let over_table = |row: Rect| Rect {
        x: table_rect.x,
        y: row.y,
        width: table_rect.width,
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, cribbage, &table))
            .alignment(Alignment::Center),
        status_row,
    );
    draw_player_bar(
        frame,
        over_table(top_bar),
        daily,
        board,
        detail,
        state,
        &table,
        other(my_seat),
    );
    frame.render_widget(
        Paragraph::new(table_lines(
            &table,
            my_seat,
            my_turn.then_some(board.cursor),
            &cribbage.marked,
            board.spectating,
            tier,
            middle_label(board, state, &table, my_seat),
        )),
        table_rect,
    );
    if my_turn {
        let count = table.hands[my_seat].len();
        board.card_slots.set(Some(CardSlots {
            rect: Rect {
                x: table_rect.x + hand_left(tier, count),
                y: table_rect.y + hand_top(tier),
                width: tier.fan_width(count),
                height: tier.card_h(),
            },
            step: tier.fan_step(),
            count,
        }));
    }
    draw_player_bar(
        frame,
        over_table(bottom_bar),
        daily,
        board,
        detail,
        state,
        &table,
        my_seat,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail, &table, my_seat)).alignment(Alignment::Center),
        hint_row,
    );
}

/// The whole table: the opponent's backs, the starter, the crib and the
/// count, then your hand with the cursor marker under it. The middle label
/// arrives pre-built because naming players needs the board, which this
/// stays free of.
pub(crate) fn table_lines(
    table: &Table,
    my_seat: usize,
    cursor: Option<usize>,
    marked: &[Card],
    spectating: bool,
    tier: Tier,
    middle: String,
) -> Vec<Line<'static>> {
    let width = table_width(tier);
    let mut lines = Vec::new();

    // Their hand: a count of backs, never a face.
    let theirs = vec![(Face::Back, hand_ui::back_style()); table.hands[other(my_seat)].len()];
    lines.extend(row(&theirs, tier, width));
    lines.push(Line::raw(""));

    lines.extend(middle_row(table, tier));
    lines.push(hand_ui::label(middle, width, theme::TEXT_FAINT()));
    lines.push(Line::raw(""));

    // Your hand, face up, unless you are only watching. While pegging, the
    // cards that would pass 31 are dimmed: they cannot be played.
    let held = table.held(my_seat);
    let pegging = matches!(table.phase, Phase::Peg(seat) if seat == my_seat);
    let mine: Vec<(Face, Style)> = held
        .iter()
        .enumerate()
        .map(|(index, &card)| {
            if spectating {
                return (Face::Back, hand_ui::back_style());
            }
            let mut style =
                hand_ui::face_style(card, cursor == Some(index), marked.contains(&card));
            if pegging && table.count + card.value() > cribbage::MAX_COUNT {
                style = style.add_modifier(Modifier::DIM);
            }
            (Face::Up(card), style)
        })
        .collect();
    lines.extend(row(&mine, tier, width));
    lines.push(match cursor {
        Some(index) if index < held.len() => {
            hand_ui::cursor_marker(hand_left(tier, held.len()), index, held.len(), tier)
        }
        Some(_) | None => Line::raw(""),
    });
    lines
}

/// A fanned row centred in the table, or blank rows when there is nothing
/// in it, so the table never changes height.
fn row(cards: &[(Face, Style)], tier: Tier, width: u16) -> Vec<Line<'static>> {
    if cards.is_empty() {
        return vec![Line::raw(""); tier.card_h() as usize];
    }
    hand_ui::centre(
        hand_ui::fan(cards, tier),
        tier.fan_width(cards.len()),
        width,
    )
}

/// Starter (face down until cut), the crib (a back while it holds cards),
/// then every card on the current count.
fn middle_row(table: &Table, tier: Tier) -> Vec<Line<'static>> {
    let starter = match table.starter {
        Some(card) => hand_ui::fan(
            &[(Face::Up(card), hand_ui::face_style(card, false, false))],
            tier,
        ),
        None => hand_ui::fan(&[(Face::Back, hand_ui::back_style())], tier),
    };
    let crib = match table.crib.is_empty() {
        true => hand_ui::fan(&[(Face::Empty, hand_ui::empty_style())], tier),
        false => hand_ui::fan(&[(Face::Back, hand_ui::back_style())], tier),
    };
    let on_count: Vec<(Face, Style)> = table
        .run
        .iter()
        .map(|(_, card)| (Face::Up(*card), hand_ui::face_style(*card, false, false)))
        .collect();
    let count = hand_ui::fan(&on_count, tier);
    (0..tier.card_h() as usize)
        .map(|index| {
            let mut spans = starter[index].spans.clone();
            spans.push(Span::raw(" ".repeat(GAP as usize)));
            spans.extend(crib[index].spans.clone());
            spans.push(Span::raw(" ".repeat(GAP as usize)));
            if let Some(line) = count.get(index) {
                spans.extend(line.spans.clone());
            }
            Line::from(spans)
        })
        .collect()
}

/// `starter · your crib · count 22`. A spectator reads names; a player
/// reads "you" and "they".
fn middle_label(
    board: &DailyBoardState,
    state: &DailyCribbageState,
    table: &Table,
    my_seat: usize,
) -> String {
    let crib = if board.spectating {
        format!("{}'s crib", name_for(board, state.user_of(table.dealer)))
    } else if table.dealer == my_seat {
        "your crib".to_string()
    } else {
        "their crib".to_string()
    };
    let count = match table.phase {
        Phase::Discard(_) => "two each to the crib".to_string(),
        Phase::Peg(_) => format!("count {}", table.count),
        Phase::AwaitingDeal => "dealing".to_string(),
        Phase::Won(_) => "game".to_string(),
    };
    format!("starter · {crib} · {count}")
}

/// `you` for the viewer, the username for anyone else.
fn who(
    daily: &DailyState,
    board: &DailyBoardState,
    state: &DailyCribbageState,
    seat: usize,
) -> String {
    let user_id = state.user_of(seat);
    if user_id == daily.user_id() {
        "you".to_string()
    } else {
        name_for(board, user_id)
    }
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    cribbage: &CribbageDetail,
    table: &Table,
) -> Line<'static> {
    if board.resign_confirm {
        return Line::from(Span::styled(
            "Resign this match? Press r again to confirm.",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let amber = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut spans = Vec::new();
    if !detail.is_active() {
        let (heading, subtitle, color) = result_banner(daily, board, detail);
        spans.push(Span::styled(
            format!("{heading} · {subtitle}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        return Line::from(spans);
    }
    let my_turn = detail.row.turn_user_id == Some(daily.user_id()) && !board.spectating;
    match (cribbage.move_in_flight, my_turn, table.phase) {
        (true, _, Phase::AwaitingDeal) => spans.push(Span::styled("Dealing the next hand…", amber)),
        (true, _, _) => spans.push(Span::styled("Card away…", amber)),
        (false, true, Phase::Discard(_)) => {
            let text = match cribbage.marked.len() {
                0 => "Pick two cards for the crib".to_string(),
                1 => "Pick one more for the crib".to_string(),
                _ => "Space on a picked card sends them".to_string(),
            };
            spans.push(Span::styled(text, amber));
        }
        (false, true, _) => spans.push(Span::styled(
            format!("Your card · count {}", table.count),
            amber,
        )),
        (false, false, _) => spans.push(Span::styled(
            format!(
                "Waiting for {}",
                detail.row.turn_user_id.map_or_else(
                    || "the deal".to_string(),
                    |user_id| name_for(board, user_id)
                )
            ),
            dim.add_modifier(Modifier::BOLD),
        )),
    }
    if let Some(deadline) = detail.row.turn_deadline_at {
        spans.push(Span::styled(
            format!("   {} on the clock", format_deadline(deadline, Utc::now())),
            dim,
        ));
    }
    Line::from(spans)
}

/// `● mira · crib  47 ━━━━━━━●┄┄┄┄┄┄ 61`: the player bar is the peg board.
#[allow(clippy::too_many_arguments)]
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyCribbageState,
    table: &Table,
    seat: usize,
) {
    if rect.height == 0 {
        return;
    }
    let user_id = state.user_of(seat);
    let on_turn = detail.is_active() && detail.row.turn_user_id == Some(user_id);
    let dot = if on_turn {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_FAINT()
    };
    let score = table.scores[seat];
    let mut left = vec![
        Span::styled("\u{25CF} ", Style::default().fg(dot)),
        Span::styled(
            who(daily, board, state, seat),
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if table.dealer == seat {
        left.push(Span::styled(
            " · crib",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    left.push(Span::styled(
        format!("  {score:>3} "),
        Style::default().fg(if score >= cribbage::WINNING_SCORE {
            theme::SUCCESS()
        } else {
            theme::TEXT()
        }),
    ));
    let deadline = on_turn
        .then_some(detail.row.turn_deadline_at)
        .flatten()
        .map(|at| format_deadline(at, Utc::now()));
    let cols = Layout::horizontal([Constraint::Min(0), Constraint::Length(9)]).split(rect);
    let used: usize = left.iter().map(|span| span.content.chars().count()).sum();
    let track = (cols[0].width as usize).saturating_sub(used + 5);
    if track >= 10 {
        let pegged = (track * score.min(cribbage::WINNING_SCORE) as usize)
            / cribbage::WINNING_SCORE as usize;
        left.push(Span::styled(
            "━".repeat(pegged),
            Style::default().fg(theme::AMBER_DIM()),
        ));
        left.push(Span::styled("●", Style::default().fg(theme::AMBER())));
        left.push(Span::styled(
            "┄".repeat(track - pegged),
            Style::default().fg(theme::BORDER_DIM()),
        ));
        left.push(Span::styled(
            format!(" {}", cribbage::WINNING_SCORE),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(left)), cols[0]);
    if let Some(deadline) = deadline {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{deadline} "),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Right),
            cols[1],
        );
    }
}

fn key_line(
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    table: &Table,
    my_seat: usize,
) -> Line<'static> {
    let mut spans = Vec::new();
    let hint = |spans: &mut Vec<Span<'static>>, key: &str, desc: &str| {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
        spans.push(Span::styled(
            format!(" {desc}   "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    };
    if board.spectating {
        spans.push(Span::styled(
            "watching · hands hidden   ".to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    } else if detail.is_active() {
        hint(&mut spans, "arrows/wasd", "choose card");
        match table.phase {
            Phase::Discard(seat) if seat == my_seat => {
                hint(&mut spans, "Space/Enter", "pick for crib")
            }
            Phase::Discard(_) | Phase::Peg(_) | Phase::AwaitingDeal | Phase::Won(_) => {
                hint(&mut spans, "Space/Enter", "play")
            }
        }
        hint(&mut spans, "r", "resign");
    }
    if board.shows_chat(detail) {
        hint(&mut spans, "i", "chat");
    }
    hint(&mut spans, "Esc", "back to lobby");
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

/// The rail: who deals, the last show counted out, and this hand's pegging.
fn draw_info_rail(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    state: &DailyCribbageState,
    table: &Table,
) {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let heading = |text: &str| {
        Line::from(Span::styled(
            text.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
    };
    let mut lines = vec![
        Line::from(Span::styled(
            "Cribbage".to_string(),
            dim.add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            format!(
                "hand {} · {} deals",
                table.hand + 1,
                who(daily, board, state, table.dealer)
            ),
            faint,
        )),
        Line::raw(""),
    ];

    if let Some(show) = &table.last_show {
        lines.push(heading(if show.hand == table.hand {
            "The show"
        } else {
            "Last show"
        }));
        let mut starter = vec![Span::styled("starter ".to_string(), faint)];
        starter.extend(hand_ui::card_spans(&[show.starter]));
        lines.push(Line::from(starter));
        for part in &show.parts {
            let owner = match part.part {
                cribbage::ShowPart::Crib => {
                    format!("{}'s crib", who(daily, board, state, part.seat))
                }
                cribbage::ShowPart::PoneHand | cribbage::ShowPart::DealerHand => {
                    who(daily, board, state, part.seat)
                }
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{owner:<20}"), dim),
                Span::styled(
                    format!("{:>3}", part.count.total()),
                    Style::default().fg(theme::TEXT()),
                ),
            ]));
            let mut cards = vec![Span::raw("  ")];
            cards.extend(hand_ui::card_spans(&part.cards));
            lines.push(Line::from(cards));
            let breakdown = count_breakdown(part.count);
            if !breakdown.is_empty() {
                lines.push(Line::from(Span::styled(format!("  {breakdown}"), faint)));
            }
        }
        lines.push(Line::raw(""));
    }

    if !table.pegged.is_empty() {
        lines.push(heading("Pegging"));
        let cards: Vec<Card> = table.pegged.iter().map(|(_, card)| *card).collect();
        let mut played = vec![Span::raw("  ")];
        played.extend(hand_ui::card_spans(&cards));
        lines.push(Line::from(played));
        for peg in table
            .log
            .iter()
            .filter(|peg| peg.hand == table.hand && !matches!(peg.why, Why::Show(_)))
        {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<10}", who(daily, board, state, peg.seat)), dim),
                Span::styled(format!("{:<14}", peg.why.label()), faint),
                Span::styled(
                    format!("{:>2}", peg.points),
                    Style::default().fg(theme::TEXT()),
                ),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// `15s 4 · pairs 2 · runs 3 · nobs 1`, naming only what scored.
fn count_breakdown(count: cribbage::Count) -> String {
    [
        ("15s", count.fifteens),
        ("pairs", count.pairs),
        ("runs", count.runs),
        ("flush", count.flush),
        ("nobs", count.nobs),
    ]
    .into_iter()
    .filter(|(_, points)| *points > 0)
    .map(|(name, points)| format!("{name} {points}"))
    .collect::<Vec<_>>()
    .join(" · ")
}

#[cfg(test)]
#[path = "cribbage_ui_test.rs"]
mod cribbage_ui_test;
