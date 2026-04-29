use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{
    common::theme,
    games::{
        cards::{AsciiCardTheme, PlayingCard},
        ui::{draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint},
    },
    rooms::blackjack::state::{
        BlackjackSeat, BlackjackSnapshot, Outcome, Phase, SeatPhase, State,
    },
};

const FANCY_MIN_HEIGHT: u16 = 22;
const FANCY_MIN_WIDTH: u16 = 60;
const SEAT_PANEL_WIDTH: u16 = 12;
const SEAT_PANEL_HEIGHT: u16 = 7;
const DEALER_BLOCK_HEIGHT: u16 = 9;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_sidebar: bool) {
    let snapshot = state.snapshot();
    draw_game_snapshot(
        frame,
        area,
        &snapshot,
        state.seat_index(),
        state.can_act(),
        show_sidebar,
    );
}

fn draw_game_snapshot(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
    show_sidebar: bool,
) {
    if area.height >= FANCY_MIN_HEIGHT && area.width >= FANCY_MIN_WIDTH {
        draw_table_fancy(
            frame,
            area,
            snapshot,
            user_seat_index,
            user_is_active,
        );
    } else {
        draw_table_compact(
            frame,
            area,
            snapshot,
            user_seat_index,
            user_is_active,
            show_sidebar,
        );
    }
}

// ──────────────── Fancy table layout ────────────────

fn draw_table_fancy(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
) {
    let block = Block::default()
        .title(table_title(snapshot, user_seat_index))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::vertical([
        Constraint::Length(DEALER_BLOCK_HEIGHT),
        Constraint::Length(1),                  // felt divider
        Constraint::Length(SEAT_PANEL_HEIGHT),
        Constraint::Min(1),                     // status line(s)
        Constraint::Length(1),                  // info bar
        Constraint::Length(1),                  // key hints
    ])
    .split(inner);

    draw_dealer_block(frame, chunks[0], snapshot);
    draw_felt_divider(frame, chunks[1]);
    draw_seats_strip(frame, chunks[2], snapshot, user_seat_index);
    draw_status_line(frame, chunks[3], snapshot);
    draw_info_bar(frame, chunks[4], snapshot, user_seat_index);
    draw_key_bar(frame, chunks[5], snapshot.phase, user_seat_index.is_some(), user_is_active);

    if let Some((title, subtitle)) = &snapshot.outcome_banner {
        let color = match snapshot.last_outcome {
            Some(Outcome::PlayerBlackjack | Outcome::PlayerWin | Outcome::Push) => theme::SUCCESS(),
            Some(Outcome::DealerWin) | None => theme::ERROR(),
        };
        draw_game_overlay(frame, inner, title.as_str(), subtitle.as_str(), color);
    }
}

fn table_title(snapshot: &BlackjackSnapshot, user_seat_index: Option<usize>) -> String {
    let seated = snapshot
        .seats
        .iter()
        .filter(|s| s.user_id.is_some())
        .count();
    let max = snapshot.seats.len();
    let seat_label = match user_seat_index {
        Some(i) => format!("seat {}", i + 1),
        None => "viewer".to_string(),
    };
    format!(
        " Blackjack · {seated}/{max} seated · {seat_label} · Bal {bal} ",
        bal = snapshot.balance
    )
}

fn draw_dealer_block(frame: &mut Frame, area: Rect, snapshot: &BlackjackSnapshot) {
    if area.height < 4 {
        return;
    }

    let theme_card = AsciiCardTheme::Outline;
    let card_h = theme_card.card_height() as u16;

    let label_area = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
    let cards_area = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: card_h,
    };
    let total_y = cards_area.y + card_h;
    let total_area = Rect {
        x: area.x,
        y: total_y.min(area.y + area.height - 1),
        width: area.width,
        height: 1,
    };

    let label = Line::from(vec![
        Span::styled(
            "── DEALER ──",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(label).alignment(Alignment::Center),
        label_area,
    );

    draw_dealer_cards(frame, cards_area, snapshot, theme_card);

    let total_text = format_dealer_total(snapshot);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            total_text,
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        total_area,
    );
}

fn draw_dealer_cards(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    card_theme: AsciiCardTheme,
) {
    let card_w = card_width(card_theme) as u16;
    let card_h = card_theme.card_height() as u16;
    let cards = &snapshot.dealer_hand;
    let total_cards = cards.len().max(2);
    let gap: u16 = 2;
    let total_w = (card_w * total_cards as u16) + gap * (total_cards as u16).saturating_sub(1);
    let start_x = area.x + (area.width.saturating_sub(total_w)) / 2;

    for (idx, card) in cards.iter().enumerate() {
        let x = start_x + (card_w + gap) * idx as u16;
        let card_area = Rect {
            x,
            y: area.y,
            width: card_w.min(area.x + area.width - x),
            height: card_h,
        };
        let lines = if idx == 1 && !snapshot.dealer_revealed {
            card_theme.render_back_lines()
        } else {
            card_theme.render_face_lines(*card)
        };
        render_card_lines(frame, card_area, &lines, card_color(*card));
    }

    // If only one card in hand (pre-deal), still draw two empty placeholders for shape.
    if cards.is_empty() {
        for idx in 0..2 {
            let x = start_x + (card_w + gap) * idx as u16;
            let card_area = Rect {
                x,
                y: area.y,
                width: card_w,
                height: card_h,
            };
            let lines = card_theme.render_empty_lines();
            render_card_lines(frame, card_area, &lines, theme::TEXT_DIM());
        }
    }
}

fn format_dealer_total(snapshot: &BlackjackSnapshot) -> String {
    if snapshot.dealer_hand.is_empty() {
        return "waiting…".to_string();
    }
    if !snapshot.dealer_revealed {
        let first = snapshot
            .dealer_hand
            .first()
            .map(|c| c.rank.label())
            .unwrap_or("?");
        return format!("showing: {first} + ?");
    }
    snapshot
        .dealer_score
        .map(|score| format!("total: {}", score.total))
        .unwrap_or_else(|| "total: —".to_string())
}

fn draw_felt_divider(frame: &mut Frame, area: Rect) {
    if area.height == 0 || area.width < 4 {
        return;
    }
    let pattern = "─ ".repeat(area.width as usize / 2);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            pattern,
            Style::default().fg(theme::AMBER_DIM()),
        )))
        .alignment(Alignment::Center),
        area,
    );
}

fn draw_seats_strip(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
) {
    if area.height == 0 || snapshot.seats.is_empty() {
        return;
    }

    let n = snapshot.seats.len() as u16;
    let panel_w = SEAT_PANEL_WIDTH;
    let total_w = panel_w * n + (n.saturating_sub(1)) * 2;
    let start_x = area.x + (area.width.saturating_sub(total_w)) / 2;

    for seat in &snapshot.seats {
        let x = start_x + (panel_w + 2) * seat.index as u16;
        if x + panel_w > area.x + area.width {
            break;
        }
        let panel_area = Rect {
            x,
            y: area.y,
            width: panel_w,
            height: area.height,
        };
        draw_seat_panel(frame, panel_area, seat, user_seat_index, snapshot.phase);
    }
}

fn draw_seat_panel(
    frame: &mut Frame,
    area: Rect,
    seat: &BlackjackSeat,
    user_seat_index: Option<usize>,
    phase: Phase,
) {
    let is_you = Some(seat.index) == user_seat_index;
    let is_active = seat.phase == SeatPhase::Playing;

    let title_text = format!(" Seat {} ", seat.index + 1);
    let border_color = if is_you {
        theme::SUCCESS()
    } else if is_active {
        theme::AMBER()
    } else if seat.user_id.is_some() {
        theme::TEXT()
    } else {
        theme::BORDER_DIM()
    };

    let block = Block::default()
        .title(title_text)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::new();

    // Row 1: identity
    let identity = if is_you {
        Span::styled(
            "▶ YOU",
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        )
    } else if seat.user_id.is_some() {
        Span::styled(
            "player",
            Style::default().fg(theme::TEXT()),
        )
    } else {
        Span::styled(
            "open",
            Style::default().fg(theme::TEXT_DIM()),
        )
    };
    lines.push(Line::from(identity).alignment(Alignment::Center));

    // Row 2: cards (compact)
    let card_line = if seat.hand.is_empty() {
        if seat.user_id.is_none() {
            Line::from(Span::styled(
                "press s",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ))
        } else {
            Line::from(Span::styled(
                "·  ·",
                Style::default().fg(theme::TEXT_DIM()),
            ))
        }
    } else {
        Line::from(compact_card_spans(&seat.hand))
    };
    lines.push(card_line.alignment(Alignment::Center));

    // Row 3: total / status callout
    let status_line = seat_status_line(seat, phase);
    lines.push(status_line.alignment(Alignment::Center));

    // Row 4: bet
    let bet_line = match seat.bet_amount {
        Some(amount) => Line::from(Span::styled(
            format!("bet {amount}"),
            Style::default().fg(theme::AMBER()),
        )),
        None if seat.user_id.is_some() => Line::from(Span::styled(
            "no bet",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        None => Line::from(""),
    };
    lines.push(bet_line.alignment(Alignment::Center));

    // Row 5: outcome chip if any
    if let Some(outcome) = seat.last_outcome {
        let (label, color) = match outcome {
            Outcome::PlayerBlackjack => ("BLACKJACK", theme::SUCCESS()),
            Outcome::PlayerWin => ("WIN", theme::SUCCESS()),
            Outcome::Push => ("PUSH", theme::TEXT_DIM()),
            Outcome::DealerWin => ("LOSS", theme::ERROR()),
        };
        lines.push(
            Line::from(Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
        );
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn compact_card_spans(cards: &[PlayingCard]) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(cards.len() * 2);
    for (i, card) in cards.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let text = format!("{}{}", card.rank.label(), card.suit.symbol());
        spans.push(Span::styled(text, Style::default().fg(card_color(*card))));
    }
    spans
}

fn seat_status_line(seat: &BlackjackSeat, phase: Phase) -> Line<'static> {
    match (seat.user_id, &seat.score, seat.phase) {
        (None, _, _) => Line::from(Span::styled(
            "to sit",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        (Some(_), _, SeatPhase::Seated) if phase == Phase::Betting => Line::from(Span::styled(
            "place bet",
            Style::default().fg(theme::AMBER()),
        )),
        (Some(_), _, SeatPhase::BetPending) => Line::from(Span::styled(
            "betting…",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        (Some(_), _, SeatPhase::Ready) => Line::from(Span::styled(
            "ready",
            Style::default().fg(theme::SUCCESS()),
        )),
        (Some(_), _, SeatPhase::Playing) => Line::from(Span::styled(
            "your turn",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
        (Some(_), Some(score), SeatPhase::Stood) => Line::from(Span::styled(
            format!("stood {}", score.total),
            Style::default().fg(theme::TEXT()),
        )),
        (Some(_), Some(score), _) => Line::from(Span::styled(
            format!("tot {}", score.total),
            Style::default().fg(theme::TEXT_BRIGHT()),
        )),
        (Some(_), None, _) => Line::from(Span::styled(
            "—",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    }
}

fn draw_status_line(frame: &mut Frame, area: Rect, snapshot: &BlackjackSnapshot) {
    if area.height == 0 {
        return;
    }
    let line = Line::from(vec![
        Span::styled("· ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            snapshot.status_message.clone(),
            Style::default().fg(theme::TEXT()),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).alignment(Alignment::Center),
        area,
    );
}

fn draw_info_bar(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());
    let success = Style::default().fg(theme::SUCCESS());

    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled("Bal ", dim));
    spans.push(Span::styled(snapshot.balance.to_string(), success));

    if user_seat_index.is_some() {
        spans.push(Span::raw("  ·  "));
        spans.push(Span::styled("chip ", dim));
        spans.push(Span::styled(
            selected_chip_amount(snapshot).to_string(),
            amber,
        ));

        spans.push(Span::raw("  ·  "));
        spans.push(Span::styled("stake ", dim));
        spans.push(Span::styled(render_chip_stack(&snapshot.stake_chips), amber));

        if let Some(amount) = snapshot.current_bet_amount {
            spans.push(Span::raw("  ·  "));
            spans.push(Span::styled("locked ", dim));
            spans.push(Span::styled(amount.to_string(), amber));
        }
    }

    spans.push(Span::raw("  ·  "));
    spans.push(Span::styled("phase ", dim));
    spans.push(Span::styled(snapshot.phase.label().to_string(), Style::default().fg(theme::TEXT_BRIGHT())));

    if let Some(secs) = snapshot.betting_countdown_secs {
        spans.push(Span::raw("  ·  "));
        spans.push(Span::styled("deal ", dim));
        spans.push(Span::styled(format!("{secs}s"), amber));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        area,
    );
}

fn draw_key_bar(
    frame: &mut Frame,
    area: Rect,
    phase: Phase,
    is_seated: bool,
    is_active: bool,
) {
    if area.height == 0 {
        return;
    }
    let line = key_line(phase, is_seated, is_active);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

// ──────────────── Compact fallback (small terminals) ────────────────

fn draw_table_compact(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
    show_sidebar: bool,
) {
    let is_seated = user_seat_index.is_some();
    let info_lines = vec![
        info_tagline("Blackjack table. Sit, bet, draw, settle, repeat."),
        Line::from(""),
        info_label_value("Balance", snapshot.balance.to_string(), theme::SUCCESS()),
        info_label_value(
            "Seat",
            user_seat_index
                .map(|index| (index + 1).to_string())
                .unwrap_or_else(|| "viewer".to_string()),
            if is_seated {
                theme::SUCCESS()
            } else {
                theme::TEXT_DIM()
            },
        ),
        info_label_value(
            "Locked",
            snapshot
                .current_bet_amount
                .map(render_amount_as_chips)
                .unwrap_or_else(|| "none".to_string()),
            theme::AMBER_GLOW(),
        ),
        info_label_value(
            "Stake",
            render_chip_stack(&snapshot.stake_chips),
            theme::AMBER(),
        ),
        info_label_value(
            "Chip",
            format!("{} chip", selected_chip_amount(snapshot)),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "Phase",
            snapshot.phase.label().to_string(),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "Deal",
            snapshot
                .betting_countdown_secs
                .map(|secs| format!("{secs}s"))
                .unwrap_or_else(|| "auto".to_string()),
            theme::AMBER(),
        ),
        Line::from(""),
        key_line(snapshot.phase, is_seated, user_is_active),
    ];

    let inner = draw_game_frame(frame, area, "Blackjack", info_lines, show_sidebar);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(2),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(vec![
            render_seats_compact(snapshot, user_seat_index),
            render_chip_rack_compact(snapshot, is_seated),
        ])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme::BORDER_DIM())),
        ),
        rows[0],
    );

    let dealer_cards = render_cards_compact(&snapshot.dealer_hand, snapshot.dealer_revealed);
    let dealer_total = snapshot
        .dealer_score
        .map(|score| score.total.to_string())
        .unwrap_or_else(|| "—".to_string());

    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![
            Span::styled("Dealer: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(dealer_cards, Style::default().fg(theme::TEXT_BRIGHT())),
            Span::raw(format!("   ({dealer_total})")),
        ])]),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(render_seat_hands_compact(snapshot, user_seat_index)),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(snapshot.status_message.as_str()).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme::BORDER_DIM())),
        ),
        rows[3],
    );

    if let Some((title, subtitle)) = &snapshot.outcome_banner {
        let color = match snapshot.last_outcome {
            Some(Outcome::PlayerBlackjack | Outcome::PlayerWin | Outcome::Push) => theme::SUCCESS(),
            Some(Outcome::DealerWin) | None => theme::ERROR(),
        };
        draw_game_overlay(frame, inner, title.as_str(), subtitle.as_str(), color);
    }
}

// ──────────────── Shared helpers ────────────────

fn key_line(phase: Phase, is_seated: bool, is_active: bool) -> Line<'static> {
    if !is_seated {
        return key_hint("s/Enter sit · Esc back", "");
    }
    match phase {
        Phase::Betting => key_hint(
            "[ ] chip · Space throw · Enter lock · L leave · Esc back",
            "",
        ),
        Phase::BetPending => key_hint("waiting — bet in flight", ""),
        Phase::PlayerTurn if is_active => key_hint(
            "H/Space hit · S stand · L leave · Esc auto-stand",
            "",
        ),
        Phase::PlayerTurn => key_hint("watching others · L leave seat · Esc back", ""),
        Phase::DealerTurn => key_hint("dealer resolving…", ""),
        Phase::Settling => key_hint("N/Enter next hand · L leave · Esc back", ""),
    }
}

fn render_seats_compact(snapshot: &BlackjackSnapshot, user_seat_index: Option<usize>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "Seats: ",
        Style::default().fg(theme::TEXT_DIM()),
    )];
    for seat in &snapshot.seats {
        if seat.index > 0 {
            spans.push(Span::raw(" "));
        }
        let label = match seat.user_id {
            Some(_) if Some(seat.index) == user_seat_index => {
                format!("[{} You]", seat.index + 1)
            }
            Some(_) if seat.phase == SeatPhase::Playing => format!("[{} Play]", seat.index + 1),
            Some(_) => format!("[{} Taken]", seat.index + 1),
            None => format!("[{} Open]", seat.index + 1),
        };
        let style = match seat.user_id {
            Some(_) if Some(seat.index) == user_seat_index => Style::default().fg(theme::SUCCESS()),
            Some(_) if seat.phase == SeatPhase::Playing => Style::default().fg(theme::AMBER()),
            Some(_) => Style::default().fg(theme::TEXT()),
            None => Style::default().fg(theme::TEXT_DIM()),
        };
        spans.push(Span::styled(label, style));
    }
    Line::from(spans)
}

fn render_chip_rack_compact(snapshot: &BlackjackSnapshot, is_seated: bool) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "Rack: ",
        Style::default().fg(theme::TEXT_DIM()),
    )];
    for (index, amount) in snapshot.chip_denominations.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let selected =
            is_seated && snapshot.phase == Phase::Betting && index == snapshot.selected_chip_index;
        let style = if selected {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
        } else {
            Style::default().fg(theme::AMBER_DIM())
        };
        spans.push(Span::styled(format!("({amount})"), style));
    }
    spans.push(Span::styled(
        "  Stake: ",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::styled(
        render_chip_stack(&snapshot.stake_chips),
        Style::default().fg(theme::AMBER()),
    ));
    Line::from(spans)
}

fn render_seat_hands_compact(
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
) -> Vec<Line<'static>> {
    snapshot
        .seats
        .iter()
        .map(|seat| {
            let label = if Some(seat.index) == user_seat_index {
                format!("Seat {} You", seat.index + 1)
            } else if seat.phase == SeatPhase::Playing {
                format!("Seat {} Play", seat.index + 1)
            } else {
                format!("Seat {}", seat.index + 1)
            };
            let label_style = if Some(seat.index) == user_seat_index {
                Style::default().fg(theme::SUCCESS())
            } else if seat.phase == SeatPhase::Playing {
                Style::default().fg(theme::AMBER())
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            let hand = if seat.hand.is_empty() {
                "—".to_string()
            } else {
                render_cards_compact(&seat.hand, true)
            };
            let total = seat
                .score
                .map(|score| score.total.to_string())
                .unwrap_or_else(|| "—".to_string());
            let bet = seat
                .bet_amount
                .map(|bet| bet.to_string())
                .unwrap_or_else(|| "—".to_string());
            let result = match seat.last_outcome {
                Some(Outcome::PlayerBlackjack) => " blackjack",
                Some(Outcome::PlayerWin) => " win",
                Some(Outcome::Push) => " push",
                Some(Outcome::DealerWin) => " loss",
                None => "",
            };
            Line::from(vec![
                Span::styled(format!("{label:<13}"), label_style),
                Span::styled(
                    format!("{} ", seat.phase.label()),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled(
                    format!("bet {bet:<3} "),
                    Style::default().fg(theme::AMBER()),
                ),
                Span::styled(hand, Style::default().fg(theme::TEXT_BRIGHT())),
                Span::raw(format!(" ({total}){result}")),
            ])
        })
        .collect()
}

fn render_chip_stack(chips: &[i64]) -> String {
    if chips.is_empty() {
        return "empty".to_string();
    }
    chips
        .iter()
        .map(|amount| format!("({amount})"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_amount_as_chips(amount: i64) -> String {
    let mut remaining = amount;
    let mut chips = Vec::new();
    for chip in [10000, 5000, 2000, 1000, 500, 200, 100, 50, 20, 10]
        .iter()
    {
        while remaining >= *chip {
            chips.push(*chip);
            remaining -= *chip;
        }
    }
    if remaining > 0 {
        chips.push(remaining);
    }
    render_chip_stack(&chips)
}

fn selected_chip_amount(snapshot: &BlackjackSnapshot) -> i64 {
    snapshot
        .chip_denominations
        .get(snapshot.selected_chip_index)
        .copied()
        .unwrap_or(snapshot.min_bet)
}

fn render_cards_compact(cards: &[PlayingCard], reveal_all: bool) -> String {
    let theme = AsciiCardTheme::Minimal;
    cards
        .iter()
        .enumerate()
        .map(|(idx, card)| {
            if !reveal_all && idx == 1 {
                theme.render_back_compact().to_string()
            } else {
                format!("[{}]", theme.render_face_compact(*card).trim())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_card_lines(
    frame: &mut Frame,
    area: Rect,
    lines: &[String],
    color: ratatui::style::Color,
) {
    let style = Style::default().fg(color);
    let lines = lines
        .iter()
        .map(|raw| Line::from(Span::styled(raw.clone(), style)))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn card_width(theme: AsciiCardTheme) -> usize {
    match theme {
        AsciiCardTheme::Minimal => 3,
        AsciiCardTheme::Boxed => 5,
        AsciiCardTheme::Outline => 9,
    }
}

fn card_color(card: PlayingCard) -> ratatui::style::Color {
    use crate::app::games::cards::CardSuit;
    match card.suit {
        CardSuit::Hearts | CardSuit::Diamonds => theme::ERROR(),
        CardSuit::Clubs | CardSuit::Spades => theme::TEXT_BRIGHT(),
    }
}
