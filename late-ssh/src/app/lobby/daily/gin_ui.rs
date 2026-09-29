//! Full-screen daily gin rummy board: your hand fanned across the bottom,
//! melds first and deadwood after, the opponent's as backs along the top,
//! and the stock and the discard pile between them. Under your hand, the
//! melds the rules found and what your deadwood comes to; the rail keeps
//! the discard pile, every hand's result, and the last hand laid down.
//!
//! This is the file that keeps gin's hidden information hidden. The match
//! state holds both hands and the whole stock (see `gin.rs`), so faces are
//! drawn for the viewer's own hand only and a spectator gets no hand faces
//! at all. What the table makes public is drawn for everyone: the discard
//! pile, a card taken from it, and both hands once a hand is laid down.

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
        gin::{self, DailyGinState, GinMove, HandEnd, Phase, Pile, Table, other},
        hand_ui::{self, CardSlots, Face, Tier},
        state::{DailyBoardState, DailyMatchDetail, DailyState, GinDetail, format_deadline},
        std_deck::Card,
    },
};

/// status + two player bars + key hints around the table.
const CHROME_ROWS: u16 = 4;
const INFO_RAIL_WIDTH: u16 = 28;
/// Breathing room required around the table before the rail appears.
const INFO_RAIL_MIN_EXTRA: u16 = 6;
/// A hand mid-turn: ten dealt plus the card just drawn.
const MAX_HELD: usize = gin::HAND + 1;
/// Discards shown fanned on the table; the rail keeps the whole pile.
const PILE_SHOWN: usize = 6;
/// Columns between the stock and the discard pile.
const GAP: u16 = 3;

/// opponent hand + gap + piles + their marker/label + gap + own hand + the
/// cursor marker + the meld line.
fn table_rows(tier: Tier) -> u16 {
    tier.card_h() * 3 + 5
}

fn table_width(tier: Tier) -> u16 {
    tier.fan_width(MAX_HELD).max(piles_width(tier, PILE_SHOWN))
}

fn piles_width(tier: Tier, shown: usize) -> u16 {
    tier.card_w() + GAP + tier.fan_width(shown.max(1))
}

/// Where the piles start inside the table: the widest they get, centred,
/// so they do not slide sideways as the pile grows.
fn piles_left(tier: Tier) -> u16 {
    table_width(tier).saturating_sub(piles_width(tier, PILE_SHOWN)) / 2
}

fn hand_left(tier: Tier, count: usize) -> u16 {
    table_width(tier).saturating_sub(tier.fan_width(count)) / 2
}

fn piles_top(tier: Tier) -> u16 {
    tier.card_h() + 1
}

fn hand_top(tier: Tier) -> u16 {
    tier.card_h() * 2 + 3
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    gin: &GinDetail,
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

    let state = &gin.state;
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
        && !gin.move_in_flight
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
        Paragraph::new(status_line(daily, board, detail, gin, &table, my_seat))
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
            gin.marked,
            board.spectating,
            tier,
        )),
        table_rect,
    );
    let slots = match table.phase {
        Phase::Draw(seat) if my_turn && seat == my_seat => Some(CardSlots {
            rect: Rect {
                x: table_rect.x + piles_left(tier),
                y: table_rect.y + piles_top(tier),
                width: piles_width(tier, table.discards.len().min(PILE_SHOWN)),
                height: tier.card_h(),
            },
            step: tier.card_w() + GAP,
            count: 2,
        }),
        Phase::Discard(seat) if my_turn && seat == my_seat => {
            let count = table.hands[my_seat].len();
            Some(CardSlots {
                rect: Rect {
                    x: table_rect.x + hand_left(tier, count),
                    y: table_rect.y + hand_top(tier),
                    width: tier.fan_width(count),
                    height: tier.card_h(),
                },
                step: tier.fan_step(),
                count,
            })
        }
        Phase::Draw(_) | Phase::Discard(_) | Phase::AwaitingDeal | Phase::Won(_) => None,
    };
    board.card_slots.set(slots);
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

/// The whole table: the opponent's backs, the stock and the discard pile,
/// then your hand, the cursor marker, and your melds.
pub(crate) fn table_lines(
    table: &Table,
    my_seat: usize,
    cursor: Option<usize>,
    marked: Option<Card>,
    spectating: bool,
    tier: Tier,
) -> Vec<Line<'static>> {
    let width = table_width(tier);
    let mut lines = Vec::new();

    // Their hand: a count of backs, never a face.
    let theirs = vec![(Face::Back, hand_ui::back_style()); table.hands[other(my_seat)].len()];
    lines.extend(row(&theirs, tier, width));
    lines.push(Line::raw(""));

    let drawing = matches!(table.phase, Phase::Draw(seat) if seat == my_seat);
    let pad = " ".repeat(piles_left(tier) as usize);
    for line in piles(table, tier) {
        let mut spans = vec![Span::raw(pad.clone())];
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }
    lines.push(match cursor {
        // Drawing: the marker picks a pile.
        Some(pile) if drawing => {
            let column = piles_left(tier)
                + match pile {
                    0 => tier.card_w() / 2,
                    _ => tier.card_w() + GAP + tier.card_w() / 2,
                };
            Line::from(Span::styled(
                format!("{}▲", " ".repeat(column as usize)),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ))
        }
        Some(_) | None => hand_ui::label(
            format!(
                "stock {} · {} discarded",
                table.stock_remaining(),
                table.discards.len()
            ),
            width,
            theme::TEXT_FAINT(),
        ),
    });
    lines.push(Line::raw(""));

    // Your hand, face up and melds first, unless you are only watching.
    let held = table.held(my_seat);
    let mine: Vec<(Face, Style)> = held
        .iter()
        .enumerate()
        .map(|(index, &card)| match spectating {
            true => (Face::Back, hand_ui::back_style()),
            false => (
                Face::Up(card),
                hand_ui::face_style(
                    card,
                    cursor == Some(index) && !drawing,
                    marked == Some(card),
                ),
            ),
        })
        .collect();
    lines.extend(row(&mine, tier, width));
    lines.push(match cursor {
        Some(index) if !drawing && index < held.len() => {
            hand_ui::cursor_marker(hand_left(tier, held.len()), index, held.len(), tier)
        }
        Some(_) | None => Line::raw(""),
    });
    lines.push(match spectating {
        true => Line::raw(""),
        false => meld_line(&table.hands[my_seat], width),
    });
    lines
}

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

/// The stock as a back (an empty outline once it has run down), then the
/// newest discards fanned with the top card whole.
fn piles(table: &Table, tier: Tier) -> Vec<Line<'static>> {
    let stock = match table.stock_remaining() {
        0 => hand_ui::fan(&[(Face::Empty, hand_ui::empty_style())], tier),
        _ => hand_ui::fan(&[(Face::Back, hand_ui::back_style())], tier),
    };
    let shown = &table.discards[table.discards.len().saturating_sub(PILE_SHOWN)..];
    let pile: Vec<(Face, Style)> = match shown.is_empty() {
        true => vec![(Face::Empty, hand_ui::empty_style())],
        false => shown
            .iter()
            .map(|card| (Face::Up(*card), hand_ui::face_style(*card, false, false)))
            .collect(),
    };
    let pile = hand_ui::fan(&pile, tier);
    (0..tier.card_h() as usize)
        .map(|index| {
            let mut spans = stock[index].spans.clone();
            spans.push(Span::raw(" ".repeat(GAP as usize)));
            spans.extend(pile[index].spans.clone());
            Line::from(spans)
        })
        .collect()
}

/// `7♥ 8♥ 9♥ │ 7♦ 7♣ 7♠ · deadwood 2♠ = 2`, or `gin` with nothing loose.
fn meld_line(hand: &[Card], width: u16) -> Line<'static> {
    let melding = gin::best_melding(hand);
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut spans = Vec::new();
    for (index, meld) in melding.melds.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" │ ", faint));
        }
        spans.extend(hand_ui::card_spans(meld));
    }
    let deadwood = melding.deadwood_points();
    if !melding.melds.is_empty() {
        spans.push(Span::styled(" · ", faint));
    }
    match deadwood {
        0 => spans.push(Span::styled(
            "no deadwood",
            Style::default().fg(theme::SUCCESS()),
        )),
        points => spans.push(Span::styled(format!("deadwood {points}"), faint)),
    }
    let drawn: usize = spans.iter().map(|span| span.content.chars().count()).sum();
    let mut line = vec![Span::raw(
        " ".repeat((width as usize).saturating_sub(drawn) / 2),
    )];
    line.extend(spans);
    Line::from(line)
}

fn who(daily: &DailyState, board: &DailyBoardState, state: &DailyGinState, seat: usize) -> String {
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
    gin: &GinDetail,
    table: &Table,
    my_seat: usize,
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
    match (gin.move_in_flight, my_turn, table.phase) {
        (true, _, Phase::AwaitingDeal) => spans.push(Span::styled("Dealing the next hand…", amber)),
        (true, _, _) => spans.push(Span::styled("Card away…", amber)),
        (false, true, Phase::Draw(_)) => {
            spans.push(Span::styled("Your draw · the stock or the discard", amber))
        }
        (false, true, _) => {
            let held = table.held(my_seat);
            match gin.marked.or_else(|| held.get(board.cursor).copied()) {
                Some(card) => {
                    let left = gin::deadwood_after_discard(&held, card);
                    let verb = match gin.marked == Some(card) {
                        true => "Space again throws",
                        false => "Throw",
                    };
                    spans.push(Span::styled(format!("{verb} {}", card.label()), amber));
                    let knock = match (left, gin.marked == Some(card)) {
                        (0, true) => " · g goes gin",
                        (0, false) => " · gin",
                        (1..=gin::MAX_KNOCK, true) => " · g knocks",
                        (1..=gin::MAX_KNOCK, false) => " · can knock",
                        _ => "",
                    };
                    spans.push(Span::styled(format!(" · {left} deadwood left{knock}"), dim));
                }
                None => spans.push(Span::styled("Pick a card to throw", amber)),
            }
        }
        (false, false, phase) => {
            let waiting = detail.row.turn_user_id.map_or_else(
                || "the deal".to_string(),
                |user_id| name_for(board, user_id),
            );
            let text = match phase {
                Phase::Discard(_) => format!("{waiting} is choosing a discard"),
                Phase::Draw(_) | Phase::AwaitingDeal | Phase::Won(_) => {
                    format!("Waiting for {waiting}")
                }
            };
            spans.push(Span::styled(text, dim.add_modifier(Modifier::BOLD)));
        }
    }
    if let Some(deadline) = detail.row.turn_deadline_at {
        spans.push(Span::styled(
            format!("   {} on the clock", format_deadline(deadline, Utc::now())),
            dim,
        ));
    }
    if let Some((seat, played)) = table.last {
        let mover = who(daily, board, &gin.state, seat);
        // A stock draw names no card: only the drawer may see it.
        let what = match (played, table.taken) {
            (GinMove::Draw(Pile::Stock), _) => "drew".to_string(),
            (GinMove::Draw(Pile::Discard), Some(card)) => format!("took {}", card.label()),
            (GinMove::Draw(Pile::Discard), None) => "took the discard".to_string(),
            (GinMove::Discard { card, .. }, _) => format!("threw {}", card.label()),
        };
        spans.push(Span::styled(format!("   last {mover} {what}"), dim));
    }
    Line::from(spans)
}

/// `● mira · deals   64 of 100`, with the running deadline on the mover's
/// bar.
#[allow(clippy::too_many_arguments)]
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyGinState,
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
            " · deals",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    left.push(Span::styled(
        format!("   {score} of {}", gin::TARGET_SCORE),
        Style::default().fg(if score >= gin::TARGET_SCORE {
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
        match table.phase {
            Phase::Draw(seat) if seat == my_seat => {
                hint(&mut spans, "arrows/wasd", "choose pile");
                hint(&mut spans, "Space/Enter", "draw");
            }
            Phase::Discard(seat) if seat == my_seat => {
                hint(&mut spans, "arrows/wasd", "choose card");
                hint(&mut spans, "Space/Enter", "pick, then throw");
                hint(&mut spans, "g", "knock");
            }
            Phase::Draw(_) | Phase::Discard(_) | Phase::AwaitingDeal | Phase::Won(_) => {}
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

/// The rail: who deals, every hand's result, the last hand laid down, and
/// the discard pile (all public, and a move a day is too slow to remember).
fn draw_info_rail(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    state: &DailyGinState,
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
            "Gin rummy".to_string(),
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

    if !table.results.is_empty() {
        lines.push(heading("Hands"));
        for result in &table.results {
            let text = match (result.end, result.scored) {
                (HandEnd::Dead, _) | (_, None) => "dead hand".to_string(),
                (end, Some((seat, points))) => {
                    let how = match end {
                        HandEnd::Gin => "gin",
                        HandEnd::Knock => "knock",
                        HandEnd::Undercut => "undercut",
                        HandEnd::Dead => "dead",
                    };
                    format!("{} {how} +{points}", who(daily, board, state, seat))
                }
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{:>2} ", result.hand + 1), faint),
                Span::styled(text, dim),
            ]));
        }
        lines.push(Line::raw(""));
    }

    if let Some(result) = table
        .results
        .last()
        .filter(|result| !result.reveals.is_empty())
    {
        lines.push(heading("Last hand laid down"));
        for reveal in &result.reveals {
            lines.push(Line::from(Span::styled(
                format!(
                    "{} · deadwood {}",
                    who(daily, board, state, reveal.seat),
                    reveal.deadwood_points()
                ),
                dim,
            )));
            for meld in &reveal.melds {
                let mut spans = vec![Span::raw("  ")];
                spans.extend(hand_ui::card_spans(meld));
                lines.push(Line::from(spans));
            }
            if !reveal.laid_off.is_empty() {
                let mut spans = vec![Span::styled("  laid off ".to_string(), faint)];
                spans.extend(hand_ui::card_spans(&reveal.laid_off));
                lines.push(Line::from(spans));
            }
            if !reveal.deadwood.is_empty() {
                let mut spans = vec![Span::styled("  loose ".to_string(), faint)];
                spans.extend(hand_ui::card_spans(&reveal.deadwood));
                lines.push(Line::from(spans));
            }
        }
        lines.push(Line::raw(""));
    }

    lines.push(heading("Discards"));
    let mut pile = vec![Span::raw("  ")];
    pile.extend(hand_ui::card_spans(&table.discards));
    lines.push(Line::from(pile));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
#[path = "gin_ui_test.rs"]
mod gin_ui_test;
