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
        ui::{draw_game_frame_with_info_sidebar, info_label_value, info_tagline, key_hint},
    },
    rooms::blackjack::state::{
        BlackjackSeat, BlackjackSnapshot, Outcome, Phase, SeatAction, SeatPhase, State, is_bust,
    },
};

const FANCY_MIN_HEIGHT: u16 = 19;
const FANCY_MIN_WIDTH: u16 = 60;
const SEAT_PANEL_WIDTH: u16 = 12;
const SEAT_PANEL_HEIGHT: u16 = 7;
const SEAT_PANEL_WIDTH_OUTLINE: u16 = 22;
const SEAT_PANEL_HEIGHT_OUTLINE: u16 = 11;
const DEALER_BLOCK_HEIGHT: u16 = 9;
const ULTRA_FANCY_MIN_WIDTH: u16 = 96;
const ULTRA_FANCY_MIN_HEIGHT: u16 = 23;

/// Height the fancy table prefers given the available area.
/// Returns 0 when the area is too small for fancy and the compact path will run.
pub fn fancy_game_height(area: Rect) -> u16 {
    if area.height < FANCY_MIN_HEIGHT || area.width < FANCY_MIN_WIDTH {
        return 0;
    }
    let panel_h = if area.height >= ULTRA_FANCY_MIN_HEIGHT && area.width >= ULTRA_FANCY_MIN_WIDTH {
        SEAT_PANEL_HEIGHT_OUTLINE
    } else {
        SEAT_PANEL_HEIGHT
    };
    DEALER_BLOCK_HEIGHT + 1 + panel_h + 2
}

pub fn draw_game(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    show_sidebar: bool,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    let snapshot = state.snapshot();
    draw_game_snapshot(
        frame,
        area,
        &snapshot,
        state.seat_index(),
        state.can_act(),
        show_sidebar,
        usernames,
    );
}

fn draw_game_snapshot(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
    show_sidebar: bool,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    if area.height >= FANCY_MIN_HEIGHT && area.width >= FANCY_MIN_WIDTH {
        draw_table_fancy(
            frame,
            area,
            snapshot,
            user_seat_index,
            user_is_active,
            usernames,
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
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    let inner = area;

    let seat_count = snapshot.seats.len() as u16;
    let outline_strip_w = seat_count
        .saturating_mul(SEAT_PANEL_WIDTH_OUTLINE)
        .saturating_add(seat_count.saturating_sub(1).saturating_mul(2));
    let ultra = inner.width >= ULTRA_FANCY_MIN_WIDTH
        && inner.height >= ULTRA_FANCY_MIN_HEIGHT
        && inner.width >= outline_strip_w;
    let panel_w = if ultra {
        SEAT_PANEL_WIDTH_OUTLINE
    } else {
        SEAT_PANEL_WIDTH
    };
    let panel_h = if ultra {
        SEAT_PANEL_HEIGHT_OUTLINE
    } else {
        SEAT_PANEL_HEIGHT
    };
    let card_theme = if ultra {
        AsciiCardTheme::Outline
    } else {
        AsciiCardTheme::Minimal
    };

    let chunks = Layout::vertical([
        Constraint::Length(DEALER_BLOCK_HEIGHT),
        Constraint::Length(1), // felt divider
        Constraint::Length(panel_h),
        Constraint::Length(1), // status message
        Constraint::Length(1), // countdown + key hints
    ])
    .split(inner);

    draw_dealer_block(frame, chunks[0], snapshot);
    draw_felt_divider(frame, chunks[1], snapshot);
    draw_seats_strip(
        frame,
        chunks[2],
        snapshot,
        user_seat_index,
        panel_w,
        card_theme,
        usernames,
    );
    draw_status_line(frame, chunks[3], snapshot, user_seat_index, user_is_active);
    draw_keys_bar(frame, chunks[4], snapshot, user_seat_index, user_is_active);
}

fn draw_dealer_block(frame: &mut Frame, area: Rect, snapshot: &BlackjackSnapshot) {
    if area.height < 4 {
        return;
    }

    let theme_card = AsciiCardTheme::Outline;
    let card_h = theme_card.card_height() as u16;

    let label_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
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

    let label = Line::from(vec![Span::styled(
        "── DEALER ──",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )]);
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
        .unwrap_or_else(|| "total: ·".to_string())
}

fn draw_felt_divider(frame: &mut Frame, area: Rect, snapshot: &BlackjackSnapshot) {
    if area.height == 0 || area.width < 4 {
        return;
    }
    let dim = Style::default().fg(theme::AMBER_DIM());
    let amber = Style::default().fg(theme::AMBER());

    let seated = snapshot
        .seats
        .iter()
        .filter(|s| s.user_id.is_some())
        .count();
    let total = snapshot.seats.len();
    let label = format!("seats {seated}/{total} · min bet {}", snapshot.min_bet);

    let chip_w = label.chars().count() + 6;
    let side_each = (area.width as usize).saturating_sub(chip_w) / 2;
    let half_pattern = "─ ".repeat(side_each / 2);
    let line = Line::from(vec![
        Span::styled(half_pattern.clone(), dim),
        Span::styled("─[ ", dim),
        Span::styled(label, amber),
        Span::styled(" ]─", dim),
        Span::styled(half_pattern, dim),
    ]);

    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn draw_seats_strip(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    panel_w: u16,
    card_theme: AsciiCardTheme,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    if area.height == 0 || snapshot.seats.is_empty() {
        return;
    }

    let n = snapshot.seats.len() as u16;
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
        if card_theme == AsciiCardTheme::Outline {
            draw_seat_panel_outline(
                frame,
                panel_area,
                seat,
                user_seat_index,
                snapshot,
                usernames,
            );
        } else {
            draw_seat_panel(
                frame,
                panel_area,
                seat,
                user_seat_index,
                snapshot,
                usernames,
            );
        }
    }
}

fn draw_seat_panel_outline(
    frame: &mut Frame,
    area: Rect,
    seat: &BlackjackSeat,
    user_seat_index: Option<usize>,
    snapshot: &BlackjackSnapshot,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    let is_you = Some(seat.index) == user_seat_index;
    let is_active = seat.phase == SeatPhase::Playing;
    let is_seated = seat.user_id.is_some();
    let phase = snapshot.phase;
    let show_seat_chips =
        phase == Phase::Betting && !seat.stake_chips.is_empty() && seat.bet_amount.is_none();

    let border_color = if is_you {
        theme::SUCCESS()
    } else if is_active {
        theme::AMBER()
    } else if is_seated {
        theme::TEXT()
    } else {
        theme::BORDER_DIM()
    };

    let block = Block::default()
        .title_top(seat_title_left(seat, is_you, is_seated, usernames))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 8 {
        // not enough room for full outline cards; degrade to compact panel inside
        draw_seat_panel_inner(frame, inner, seat, user_seat_index, snapshot, usernames);
        return;
    }

    // Layout: cards (5) + extras/status (1) + bet/total (1) + balance (1) + outcome (1)
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    // Row 0: outline cards (or empty placeholders) — chips no longer overlay this slot
    draw_hand_outline(frame, rows[0], &seat.hand, true);

    // Row 1: extras (cards 3+), chip total, or status hint
    if show_seat_chips {
        let total: i64 = seat.stake_chips.iter().sum();
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("stake ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    total.to_string(),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
            .alignment(Alignment::Center),
            rows[1],
        );
    } else if seat.hand.len() > 2 {
        let extras: Vec<Span<'static>> = seat
            .hand
            .iter()
            .skip(2)
            .flat_map(|card| {
                vec![
                    Span::raw(" "),
                    Span::styled(
                        format!("{}{}", card.rank.label(), card.suit.symbol()),
                        Style::default().fg(card_color(*card)),
                    ),
                ]
            })
            .collect();
        let mut spans = vec![Span::styled("+", Style::default().fg(theme::TEXT_DIM()))];
        spans.extend(extras);
        frame.render_widget(
            Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
            rows[1],
        );
    } else if seat.hand.is_empty() && !is_seated {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "press s to sit",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            )))
            .alignment(Alignment::Center),
            rows[1],
        );
    } else {
        let status_line = seat_status_line(seat, phase, is_you);
        frame.render_widget(
            Paragraph::new(status_line).alignment(Alignment::Center),
            rows[1],
        );
    }

    // Row 2: bet + total
    frame.render_widget(
        Paragraph::new(bet_total_line(seat)).alignment(Alignment::Center),
        rows[2],
    );

    // Row 3: balance (seated players only)
    frame.render_widget(
        Paragraph::new(balance_line(seat, snapshot, is_you)).alignment(Alignment::Center),
        rows[3],
    );

    // Row 4: latest visible seat result/action (single line).
    if let Some(line) = seat_notice_line(seat) {
        frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), rows[4]);
    }
}

fn seat_title_left(
    seat: &BlackjackSeat,
    is_you: bool,
    is_seated: bool,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) -> Line<'static> {
    if !is_seated {
        return Line::from(Span::styled(
            format!(" Seat {} ", seat.index + 1),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    let name = seat
        .player
        .as_ref()
        .map(|player| player.username.clone())
        .or_else(|| seat.user_id.and_then(|uid| usernames.get(&uid).cloned()))
        .unwrap_or_else(|| "player".to_string());
    let max_chars = 14usize;
    let truncated: String = if name.chars().count() > max_chars {
        let head: String = name.chars().take(max_chars - 1).collect();
        format!("{head}…")
    } else {
        name
    };
    let (display, style) = if is_you {
        (
            format!(" ▶ {truncated} "),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (format!(" {truncated} "), Style::default().fg(theme::TEXT()))
    };
    Line::from(Span::styled(display, style))
}

fn bet_total_line<'a>(seat: &BlackjackSeat) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());

    match (seat.bet_amount, &seat.score) {
        (Some(amount), Some(score)) => {
            spans.push(Span::styled("tot ", dim));
            spans.push(Span::styled(
                score.total.to_string(),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::styled("bet ", dim));
            spans.push(Span::styled(amount.to_string(), amber));
        }
        (Some(amount), None) => {
            spans.push(Span::styled("bet ", dim));
            spans.push(Span::styled(amount.to_string(), amber));
        }
        (None, _) if seat.user_id.is_some() => {
            spans.push(Span::styled("no bet", dim));
        }
        _ => {}
    }

    Line::from(spans)
}

fn balance_line<'a>(
    seat: &BlackjackSeat,
    snapshot: &'a BlackjackSnapshot,
    is_you: bool,
) -> Line<'a> {
    let Some(player) = &seat.player else {
        return Line::from("");
    };
    let balance = if is_you {
        snapshot.balance
    } else {
        player.balance
    };
    Line::from(vec![
        Span::styled("$", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(balance.to_string(), Style::default().fg(theme::SUCCESS())),
    ])
}

fn bet_balance_line<'a>(
    seat: &BlackjackSeat,
    snapshot: &'a BlackjackSnapshot,
    is_you: bool,
) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());
    let success = Style::default().fg(theme::SUCCESS());

    match (seat.bet_amount, &seat.score) {
        (Some(amount), Some(score)) => {
            spans.push(Span::styled("tot ", dim));
            spans.push(Span::styled(
                score.total.to_string(),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::styled("bet ", dim));
            spans.push(Span::styled(amount.to_string(), amber));
        }
        (Some(amount), None) => {
            spans.push(Span::styled("bet ", dim));
            spans.push(Span::styled(amount.to_string(), amber));
        }
        (None, _) if seat.user_id.is_some() => {
            spans.push(Span::styled("no bet", dim));
        }
        _ => {}
    }

    if let Some(player) = &seat.player {
        if !spans.is_empty() {
            spans.push(Span::raw("  ·  "));
        }
        spans.push(Span::styled("$", dim));
        let balance = if is_you {
            snapshot.balance
        } else {
            player.balance
        };
        spans.push(Span::styled(balance.to_string(), success));
    }

    Line::from(spans)
}

fn draw_hand_outline(frame: &mut Frame, area: Rect, cards: &[PlayingCard], show_empty: bool) {
    let card_w: u16 = 9;
    let card_h: u16 = 5;
    let gap: u16 = 1;

    let visible_count = cards.len().min(2).max(if show_empty { 2 } else { 0 });
    if visible_count == 0 {
        return;
    }
    let visible_u = visible_count as u16;
    let total_w = card_w * visible_u + gap * visible_u.saturating_sub(1);
    let start_x = area.x + area.width.saturating_sub(total_w) / 2;
    let card_y = area.y + area.height.saturating_sub(card_h) / 2;

    for i in 0..visible_count {
        let x = start_x + (card_w + gap) * i as u16;
        if x + card_w > area.x + area.width {
            break;
        }
        let card_area = Rect {
            x,
            y: card_y,
            width: card_w,
            height: card_h,
        };
        let (lines, color) = match cards.get(i) {
            Some(card) => (
                AsciiCardTheme::Outline.render_face_lines(*card),
                card_color(*card),
            ),
            None => (
                AsciiCardTheme::Outline.render_empty_lines(),
                theme::TEXT_DIM(),
            ),
        };
        render_card_lines(frame, card_area, &lines, color);
    }
}

fn identity_span(
    seat: &BlackjackSeat,
    is_you: bool,
    is_seated: bool,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) -> Span<'static> {
    if !is_seated {
        return Span::styled("open", Style::default().fg(theme::TEXT_DIM()));
    }
    let name = seat
        .player
        .as_ref()
        .map(|player| player.username.clone())
        .or_else(|| seat.user_id.and_then(|uid| usernames.get(&uid).cloned()))
        .unwrap_or_else(|| "player".to_string());
    let display = if is_you { format!("▶ {name}") } else { name };
    let style = if is_you {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    Span::styled(display, style)
}

fn seat_notice_line(seat: &BlackjackSeat) -> Option<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());

    if let Some(outcome) = seat.last_outcome {
        let (label, color) = match outcome {
            Outcome::PlayerBlackjack => ("BLACKJACK", theme::SUCCESS()),
            Outcome::PlayerWin => ("WIN", theme::SUCCESS()),
            Outcome::Push => ("PUSH", theme::TEXT_DIM()),
            Outcome::DealerWin if is_bust(&seat.hand) => ("BUST", theme::ERROR()),
            Outcome::DealerWin => ("LOSS", theme::ERROR()),
        };
        let mut spans = vec![Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )];
        if matches!(outcome, Outcome::PlayerBlackjack | Outcome::PlayerWin) {
            spans.push(Span::styled(
                format!(" +{}", seat.last_net_change),
                Style::default().fg(theme::SUCCESS()),
            ));
        }
        return Some(Line::from(spans));
    }

    let action = seat.last_action?;
    let (label, color) = match action {
        SeatAction::Sit => ("SIT", theme::SUCCESS()),
        SeatAction::Bet => ("BET", theme::AMBER()),
        SeatAction::Hit => ("HIT", theme::AMBER()),
        SeatAction::Double => ("DOUBLE", theme::AMBER()),
        SeatAction::Stand => ("STAND", theme::TEXT_BRIGHT()),
        SeatAction::MissedDeal => ("MISSED", theme::TEXT_DIM()),
        SeatAction::MissedAction => ("TIMEOUT", theme::ERROR()),
    };
    let mut spans = vec![Span::styled(
        label,
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )];
    let suffix = match action {
        SeatAction::Bet => seat.bet_amount.map(|bet| format!(" {bet}")),
        SeatAction::Hit | SeatAction::Double | SeatAction::Stand => {
            seat.score.map(|score| format!(" {}", score.total))
        }
        _ => None,
    };
    if let Some(s) = suffix {
        spans.push(Span::styled(s, dim));
    }
    Some(Line::from(spans))
}

fn seat_notice_label(seat: &BlackjackSeat) -> Option<(&'static str, ratatui::style::Color)> {
    if let Some(outcome) = seat.last_outcome {
        return Some(match outcome {
            Outcome::PlayerBlackjack => ("BLACKJACK", theme::SUCCESS()),
            Outcome::PlayerWin => ("WIN", theme::SUCCESS()),
            Outcome::Push => ("PUSH", theme::TEXT_DIM()),
            Outcome::DealerWin if is_bust(&seat.hand) => ("BUST", theme::ERROR()),
            Outcome::DealerWin => ("LOSS", theme::ERROR()),
        });
    }

    let action = seat.last_action?;
    Some(match action {
        SeatAction::Sit => ("SIT", theme::SUCCESS()),
        SeatAction::Bet => ("BET", theme::AMBER()),
        SeatAction::Hit => ("HIT", theme::AMBER()),
        SeatAction::Double => ("DOUBLE", theme::AMBER()),
        SeatAction::Stand => ("STAND", theme::TEXT_BRIGHT()),
        SeatAction::MissedDeal => ("MISSED", theme::TEXT_DIM()),
        SeatAction::MissedAction => ("TIMEOUT", theme::ERROR()),
    })
}

fn seat_notice_subtitle(seat: &BlackjackSeat) -> Option<String> {
    if let Some(outcome) = seat.last_outcome {
        return Some(match outcome {
            Outcome::PlayerBlackjack | Outcome::PlayerWin => format!("+{}", seat.last_net_change),
            Outcome::Push => "bet returned".to_string(),
            Outcome::DealerWin => "no payout".to_string(),
        });
    }

    match seat.last_action? {
        SeatAction::Bet => seat.bet_amount.map(|bet| format!("bet {bet}")),
        SeatAction::Hit | SeatAction::Double | SeatAction::Stand => {
            seat.score.map(|score| format!("tot {}", score.total))
        }
        SeatAction::MissedDeal => Some("no bet".to_string()),
        SeatAction::MissedAction => Some("no action".to_string()),
        SeatAction::Sit => None,
    }
}

fn draw_seat_panel_inner(
    frame: &mut Frame,
    inner: Rect,
    seat: &BlackjackSeat,
    user_seat_index: Option<usize>,
    snapshot: &BlackjackSnapshot,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    let is_you = Some(seat.index) == user_seat_index;
    let is_seated = seat.user_id.is_some();
    let phase = snapshot.phase;
    let mut lines: Vec<Line> = Vec::new();
    lines.push(
        Line::from(identity_span(seat, is_you, is_seated, usernames)).alignment(Alignment::Center),
    );
    let card_line = if seat.hand.is_empty() {
        if !is_seated {
            Line::from(Span::styled(
                "press s",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ))
        } else if phase == Phase::Betting && !seat.stake_chips.is_empty() {
            Line::from(Span::styled(
                render_chip_stack(&seat.stake_chips),
                Style::default().fg(theme::AMBER()),
            ))
        } else {
            Line::from(Span::styled("·  ·", Style::default().fg(theme::TEXT_DIM())))
        }
    } else {
        Line::from(compact_card_spans(&seat.hand))
    };
    lines.push(card_line.alignment(Alignment::Center));
    lines.push(seat_status_line(seat, phase, is_you).alignment(Alignment::Center));
    lines.push(bet_balance_line(seat, snapshot, is_you).alignment(Alignment::Center));
    if let Some((label, color)) = seat_notice_label(seat) {
        lines.push(
            Line::from(Span::styled(
                label,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
        );
        if let Some(subtitle) = seat_notice_subtitle(seat) {
            lines.push(
                Line::from(Span::styled(
                    subtitle,
                    Style::default().fg(theme::TEXT_DIM()),
                ))
                .alignment(Alignment::Center),
            );
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_seat_panel(
    frame: &mut Frame,
    area: Rect,
    seat: &BlackjackSeat,
    user_seat_index: Option<usize>,
    snapshot: &BlackjackSnapshot,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
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

    draw_seat_panel_inner(frame, inner, seat, user_seat_index, snapshot, usernames);
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

fn seat_status_line(seat: &BlackjackSeat, phase: Phase, is_you: bool) -> Line<'static> {
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
        (Some(_), _, SeatPhase::Ready) => {
            Line::from(Span::styled("ready", Style::default().fg(theme::SUCCESS())))
        }
        (Some(_), _, SeatPhase::Playing) => {
            let label = if is_you { "your turn" } else { "thinking…" };
            Line::from(Span::styled(
                label,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ))
        }
        (Some(_), _, SeatPhase::ActionPending) => Line::from(Span::styled(
            "doubling…",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        (Some(_), Some(score), SeatPhase::Stood) => Line::from(Span::styled(
            format!("stood {}", score.total),
            Style::default().fg(theme::TEXT()),
        )),
        (Some(_), Some(score), _) => Line::from(Span::styled(
            format!("tot {}", score.total),
            Style::default().fg(theme::TEXT_BRIGHT()),
        )),
        (Some(_), None, _) => Line::from(Span::styled("·", Style::default().fg(theme::TEXT_DIM()))),
    }
}

enum HeadlineTone {
    Normal,
    Error,
    Win,
    Loss,
    Push,
}

fn draw_status_line(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
) {
    if area.height == 0 {
        return;
    }
    let (headline, tone) = phase_headline(snapshot, user_seat_index, user_is_active);
    let headline = match snapshot
        .betting_countdown_secs
        .or(snapshot.action_countdown_secs)
    {
        Some(0) => format!("{headline} timer expired."),
        Some(secs) => format!("{headline} {secs}s left."),
        None => headline,
    };
    let mut body_style = Style::default().fg(match tone {
        HeadlineTone::Normal => theme::TEXT(),
        HeadlineTone::Error | HeadlineTone::Loss => theme::ERROR(),
        HeadlineTone::Win => theme::SUCCESS(),
        HeadlineTone::Push => theme::TEXT_DIM(),
    });
    if matches!(
        tone,
        HeadlineTone::Win | HeadlineTone::Loss | HeadlineTone::Push
    ) {
        body_style = body_style.add_modifier(Modifier::BOLD);
    }
    let line = Line::from(vec![
        Span::styled("· ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(headline, body_style),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn phase_headline(
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
) -> (String, HeadlineTone) {
    if let Some(notice) = &snapshot.private_notice {
        return (notice.clone(), HeadlineTone::Error);
    }
    let is_seated = user_seat_index.is_some();
    match snapshot.phase {
        Phase::Betting => {
            let text = if is_seated {
                "Place your bet"
            } else {
                "Players placing bets"
            };
            (text.to_string(), HeadlineTone::Normal)
        }
        Phase::BetPending => ("Locking bets".to_string(), HeadlineTone::Normal),
        Phase::PlayerTurn => {
            let text = if user_is_active {
                "Your turn"
            } else {
                "Players hit or stand"
            };
            (text.to_string(), HeadlineTone::Normal)
        }
        Phase::DealerTurn => ("Dealer's turn".to_string(), HeadlineTone::Normal),
        Phase::Settling => settled_headline(snapshot),
    }
}

fn settled_headline(snapshot: &BlackjackSnapshot) -> (String, HeadlineTone) {
    match snapshot.last_outcome {
        Some(Outcome::PlayerBlackjack) => (
            format!("Blackjack! +{}", snapshot.last_net_change),
            HeadlineTone::Win,
        ),
        Some(Outcome::PlayerWin) => (
            format!("You won +{}", snapshot.last_net_change),
            HeadlineTone::Win,
        ),
        Some(Outcome::Push) => ("Push, bet returned".to_string(), HeadlineTone::Push),
        Some(Outcome::DealerWin) => ("You lost".to_string(), HeadlineTone::Loss),
        None => ("Round settled".to_string(), HeadlineTone::Normal),
    }
}

fn draw_keys_bar(
    frame: &mut Frame,
    area: Rect,
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
    user_is_active: bool,
) {
    if area.height == 0 {
        return;
    }
    let line = key_line(snapshot, user_seat_index.is_some(), user_is_active);
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
            if snapshot.phase == Phase::PlayerTurn {
                "Act"
            } else {
                "Deal"
            },
            snapshot
                .betting_countdown_secs
                .or(snapshot.action_countdown_secs)
                .map(|secs| format!("{secs}s"))
                .unwrap_or_else(|| "auto".to_string()),
            theme::AMBER(),
        ),
        Line::from(""),
        key_line(snapshot, is_seated, user_is_active),
    ];

    let inner =
        draw_game_frame_with_info_sidebar(frame, area, "Blackjack", info_lines, show_sidebar);
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
        .unwrap_or_else(|| "·".to_string());

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
}

// ──────────────── Shared helpers ────────────────

fn key_line(snapshot: &BlackjackSnapshot, is_seated: bool, is_active: bool) -> Line<'static> {
    if !is_seated {
        return key_hint("s/Enter sit · Esc back", "");
    }
    match snapshot.phase {
        Phase::Betting => key_hint(
            &format!("[ ] chip {}", selected_chip_amount(snapshot)),
            " · Space throw · Backspace pull · Enter/S lock · L leave · Esc back",
        ),
        Phase::BetPending => key_hint("Esc back", ""),
        Phase::PlayerTurn if is_active => key_hint(
            "H/Space hit · D double · S stand · L leave · Esc auto-stand",
            "",
        ),
        Phase::PlayerTurn => key_hint("watching others · L leave seat · Esc back", ""),
        Phase::DealerTurn => key_hint("dealer resolving…", ""),
        Phase::Settling => key_hint("Space/Enter next hand · L leave · Esc back", ""),
    }
}

fn render_seats_compact(
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
) -> Line<'static> {
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
                if !seat.stake_chips.is_empty() && seat.bet_amount.is_none() {
                    render_chip_stack(&seat.stake_chips)
                } else {
                    "·".to_string()
                }
            } else {
                render_cards_compact(&seat.hand, true)
            };
            let total = seat
                .score
                .map(|score| score.total.to_string())
                .unwrap_or_else(|| "·".to_string());
            let bet = seat
                .bet_amount
                .map(|bet| bet.to_string())
                .unwrap_or_else(|| "·".to_string());
            let result = match seat.last_outcome {
                Some(Outcome::PlayerBlackjack) => " blackjack",
                Some(Outcome::PlayerWin) => " win",
                Some(Outcome::Push) => " push",
                Some(Outcome::DealerWin) => " loss",
                None => match seat.last_action {
                    Some(SeatAction::Sit) => " sit",
                    Some(SeatAction::Bet) => " bet",
                    Some(SeatAction::Hit) => " hit",
                    Some(SeatAction::Double) => " double",
                    Some(SeatAction::Stand) => " stand",
                    Some(SeatAction::MissedDeal) => " missed",
                    Some(SeatAction::MissedAction) => " timeout",
                    None => "",
                },
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
    for chip in [10000, 5000, 2000, 1000, 500, 200, 100, 50, 20, 10].iter() {
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
