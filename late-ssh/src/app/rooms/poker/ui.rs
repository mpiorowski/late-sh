use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    games::{
        cards::{AsciiCardTheme, CardSuit, PlayingCard},
        ui::key_hint,
    },
    rooms::poker::{
        state::State,
        svc::{PokerAction, PokerPhase, PokerPublicSnapshot, PokerSeat},
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

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    let snapshot = state.public_snapshot();
    if area.height >= FANCY_MIN_HEIGHT && area.width >= FANCY_MIN_WIDTH {
        draw_table_fancy(frame, area, state, snapshot, usernames);
    } else {
        draw_table_compact(frame, area, state, snapshot, usernames);
    }
}

fn draw_table_fancy(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    snapshot: &PokerPublicSnapshot,
    usernames: &HashMap<Uuid, String>,
) {
    let seat_count = snapshot.seats.len() as u16;
    let outline_strip_w = seat_count
        .saturating_mul(SEAT_PANEL_WIDTH_OUTLINE)
        .saturating_add(seat_count.saturating_sub(1).saturating_mul(2));
    let ultra = area.width >= ULTRA_FANCY_MIN_WIDTH
        && area.height >= ULTRA_FANCY_MIN_HEIGHT
        && area.width >= outline_strip_w;
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

    let rows = Layout::vertical([
        Constraint::Length(DEALER_BLOCK_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(panel_h),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_dealer_block(frame, rows[0], snapshot);
    draw_felt_divider(frame, rows[1], snapshot);
    draw_seats_strip(
        frame, rows[2], state, snapshot, panel_w, card_theme, usernames,
    );
    draw_status_line(frame, rows[3], snapshot);
    draw_keys_bar(frame, rows[4], state, snapshot);
}

fn draw_dealer_block(frame: &mut Frame, area: Rect, snapshot: &PokerPublicSnapshot) {
    if area.height < 4 {
        return;
    }

    let card_theme = AsciiCardTheme::Outline;
    let card_h = card_theme.card_height() as u16;
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
    let phase_area = Rect {
        x: area.x,
        y: (cards_area.y + card_h).min(area.y + area.height - 1),
        width: area.width,
        height: 1,
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "-- DEALER / BOARD --",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        label_area,
    );
    draw_community_cards(frame, cards_area, snapshot, card_theme);

    let hand = if snapshot.hand_number == 0 {
        "waiting".to_string()
    } else {
        format!("hand {} - {}", snapshot.hand_number, snapshot.phase.label())
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hand,
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        phase_area,
    );
}

fn draw_community_cards(
    frame: &mut Frame,
    area: Rect,
    snapshot: &PokerPublicSnapshot,
    card_theme: AsciiCardTheme,
) {
    let card_w = card_width(card_theme) as u16;
    let card_h = card_theme.card_height() as u16;
    let total_cards = 5usize;
    let gap: u16 = 2;
    let total_w = card_w * total_cards as u16 + gap * (total_cards as u16 - 1);
    let start_x = area.x + area.width.saturating_sub(total_w) / 2;

    for index in 0..total_cards {
        let x = start_x + (card_w + gap) * index as u16;
        if x >= area.x + area.width {
            break;
        }
        let card_area = Rect {
            x,
            y: area.y,
            width: card_w.min(area.x + area.width - x),
            height: card_h,
        };
        match snapshot.community.get(index) {
            Some(card) => render_card_lines(
                frame,
                card_area,
                &card_theme.render_face_lines(*card),
                card_color(*card),
            ),
            None => render_card_lines(
                frame,
                card_area,
                &card_theme.render_empty_lines(),
                theme::TEXT_DIM(),
            ),
        }
    }
}

fn draw_felt_divider(frame: &mut Frame, area: Rect, snapshot: &PokerPublicSnapshot) {
    if area.height == 0 || area.width < 4 {
        return;
    }
    let label = match snapshot.dealer_button {
        Some(index) => format!("button seat {}", index + 1),
        None => "waiting for dealer button".to_string(),
    };
    let label = if snapshot.current_bet > 0 {
        format!(
            "{label} · pot {} · bet {}",
            snapshot.pot, snapshot.current_bet
        )
    } else {
        format!("{label} · pot {}", snapshot.pot)
    };
    let chip_w = label.chars().count() + 6;
    let side_each = (area.width as usize).saturating_sub(chip_w) / 2;
    let half_pattern = "- ".repeat(side_each / 2);
    let line = Line::from(vec![
        Span::styled(
            half_pattern.clone(),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled("-[ ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(label, Style::default().fg(theme::AMBER())),
        Span::styled(" ]-", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(half_pattern, Style::default().fg(theme::AMBER_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn draw_seats_strip(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    snapshot: &PokerPublicSnapshot,
    panel_w: u16,
    card_theme: AsciiCardTheme,
    usernames: &HashMap<Uuid, String>,
) {
    if area.height == 0 || snapshot.seats.is_empty() {
        return;
    }

    let count = snapshot.seats.len() as u16;
    let total_w = panel_w * count + count.saturating_sub(1) * 2;
    let start_x = area.x + area.width.saturating_sub(total_w) / 2;

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
            draw_seat_panel_outline(frame, panel_area, state, snapshot, seat, usernames);
        } else {
            draw_seat_panel(frame, panel_area, state, snapshot, seat, usernames);
        }
    }
}

fn draw_seat_panel_outline(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    snapshot: &PokerPublicSnapshot,
    seat: &PokerSeat,
    usernames: &HashMap<Uuid, String>,
) {
    let is_you = state.seat_index() == Some(seat.index);
    let is_active = snapshot.active_seat == Some(seat.index);
    let is_winner = snapshot.winners.contains(&seat.index);
    let border_color = seat_border_color(seat, is_you, is_active, is_winner);

    let block = Block::default()
        .title(format!(" Seat {} ", seat.index + 1))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 8 {
        draw_seat_panel_inner(frame, inner, state, snapshot, seat, usernames);
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(identity_span(seat, is_you, usernames)))
            .alignment(Alignment::Center),
        rows[0],
    );
    draw_seat_cards(frame, rows[1], state, seat, AsciiCardTheme::Outline);
    frame.render_widget(
        Paragraph::new(seat_status_line(state, snapshot, seat)).alignment(Alignment::Center),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(seat_bet_balance_line(seat)).alignment(Alignment::Center),
        rows[3],
    );
    frame.render_widget(
        Paragraph::new(seat_badge_line(snapshot, seat, is_winner)).alignment(Alignment::Center),
        rows[4],
    );
}

fn draw_seat_panel(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    snapshot: &PokerPublicSnapshot,
    seat: &PokerSeat,
    usernames: &HashMap<Uuid, String>,
) {
    let is_you = state.seat_index() == Some(seat.index);
    let is_active = snapshot.active_seat == Some(seat.index);
    let is_winner = snapshot.winners.contains(&seat.index);
    let block = Block::default()
        .title(format!(" Seat {} ", seat.index + 1))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(seat_border_color(seat, is_you, is_active, is_winner)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    draw_seat_panel_inner(frame, inner, state, snapshot, seat, usernames);
}

fn draw_seat_panel_inner(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    snapshot: &PokerPublicSnapshot,
    seat: &PokerSeat,
    usernames: &HashMap<Uuid, String>,
) {
    if area.height == 0 {
        return;
    }
    let is_you = state.seat_index() == Some(seat.index);
    let lines = vec![
        Line::from(identity_span(seat, is_you, usernames)).alignment(Alignment::Center),
        compact_seat_cards_line(state, seat).alignment(Alignment::Center),
        seat_status_line(state, snapshot, seat).alignment(Alignment::Center),
        seat_bet_balance_line(seat).alignment(Alignment::Center),
        seat_badge_line(snapshot, seat, snapshot.winners.contains(&seat.index))
            .alignment(Alignment::Center),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn seat_border_color(
    seat: &PokerSeat,
    is_you: bool,
    is_active: bool,
    is_winner: bool,
) -> ratatui::style::Color {
    if is_winner || is_you {
        theme::SUCCESS()
    } else if is_active {
        theme::AMBER()
    } else if seat.user_id.is_some() {
        theme::TEXT()
    } else {
        theme::BORDER_DIM()
    }
}

fn identity_span(
    seat: &PokerSeat,
    is_you: bool,
    usernames: &HashMap<Uuid, String>,
) -> Span<'static> {
    let Some(user_id) = seat.user_id else {
        return Span::styled("open", Style::default().fg(theme::TEXT_DIM()));
    };
    let name = usernames
        .get(&user_id)
        .cloned()
        .unwrap_or_else(|| "player".to_string());
    let display = if is_you { format!("> {name}") } else { name };
    let style = if is_you {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    Span::styled(display, style)
}

fn draw_seat_cards(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    seat: &PokerSeat,
    theme_card: AsciiCardTheme,
) {
    let card_w = card_width(theme_card) as u16;
    let card_h = theme_card.card_height() as u16;
    let gap: u16 = 1;
    let visible_count = 2;
    let total_w = card_w * visible_count as u16 + gap * (visible_count as u16).saturating_sub(1);
    let start_x = area.x + area.width.saturating_sub(total_w) / 2;
    let card_y = area.y + area.height.saturating_sub(card_h) / 2;
    let is_you = state.seat_index() == Some(seat.index);
    let private_cards = &state.private_snapshot().hole_cards;

    for index in 0..visible_count {
        let x = start_x + (card_w + gap) * index as u16;
        if x + card_w > area.x + area.width {
            break;
        }
        let card_area = Rect {
            x,
            y: card_y,
            width: card_w,
            height: card_h,
        };
        if let Some(card) = revealed_or_private_card(seat, private_cards, is_you, index) {
            render_card_lines(
                frame,
                card_area,
                &theme_card.render_face_lines(card),
                card_color(card),
            );
        } else if seat.card_count > index {
            render_card_lines(
                frame,
                card_area,
                &theme_card.render_back_lines(),
                if seat.folded {
                    theme::TEXT_DIM()
                } else {
                    theme::TEXT_BRIGHT()
                },
            );
        } else {
            render_card_lines(
                frame,
                card_area,
                &theme_card.render_empty_lines(),
                theme::TEXT_DIM(),
            );
        }
    }
}

fn compact_seat_cards_line(state: &State, seat: &PokerSeat) -> Line<'static> {
    if seat.user_id.is_none() {
        return Line::from(Span::styled(
            "press s",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if seat.card_count == 0 {
        return Line::from(Span::styled(
            "waiting",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    let is_you = state.seat_index() == Some(seat.index);
    let mut spans = Vec::new();
    let card_theme = AsciiCardTheme::Minimal;
    for index in 0..seat.card_count.min(2) {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        if let Some(card) =
            revealed_or_private_card(seat, &state.private_snapshot().hole_cards, is_you, index)
        {
            spans.push(Span::styled(
                format!("[{}]", card_theme.render_face_compact(card).trim()),
                Style::default().fg(card_color(card)),
            ));
        } else {
            spans.push(Span::styled(
                format!("[{}]", card_theme.render_back_compact().trim()),
                Style::default().fg(if seat.folded {
                    theme::TEXT_DIM()
                } else {
                    theme::TEXT_BRIGHT()
                }),
            ));
        }
    }
    Line::from(spans)
}

fn seat_status_line(
    state: &State,
    snapshot: &PokerPublicSnapshot,
    seat: &PokerSeat,
) -> Line<'static> {
    if seat.user_id.is_none() {
        return Line::from(Span::styled(
            "to sit",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if seat.pending {
        return Line::from(Span::styled(
            "pending",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if snapshot.winners.contains(&seat.index) {
        return Line::from(Span::styled(
            "showdown",
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if seat.folded {
        return Line::from(Span::styled(
            "folded",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if seat.all_in {
        return Line::from(Span::styled(
            "all-in",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if snapshot.active_seat == Some(seat.index) {
        let label = if state.seat_index() == Some(seat.index) {
            "your turn"
        } else {
            "acting..."
        };
        return Line::from(Span::styled(
            label,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(action) = seat.last_action {
        return Line::from(Span::styled(
            action.label().to_ascii_lowercase(),
            Style::default().fg(theme::TEXT()),
        ));
    }
    if seat.in_hand {
        Line::from(Span::styled("in hand", Style::default().fg(theme::TEXT())))
    } else {
        Line::from(Span::styled(
            "seated",
            Style::default().fg(theme::TEXT_DIM()),
        ))
    }
}

fn seat_bet_balance_line(seat: &PokerSeat) -> Line<'static> {
    if seat.user_id.is_none() {
        return Line::from(Span::raw(""));
    }
    let text = if seat.committed > 0 {
        format!("bal {} · pot {}", seat.balance, seat.committed)
    } else {
        format!("bal {}", seat.balance)
    };
    Line::from(Span::styled(text, Style::default().fg(theme::SUCCESS())))
}

fn seat_badge_line(
    snapshot: &PokerPublicSnapshot,
    seat: &PokerSeat,
    is_winner: bool,
) -> Line<'static> {
    let mut spans = Vec::new();
    if is_winner {
        spans.push(Span::styled(
            "WINNER",
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if snapshot.dealer_button == Some(seat.index) {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            "button",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    if seat.in_hand && !seat.folded && !snapshot.winners.contains(&seat.index) && !seat.all_in {
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            snapshot.phase.label(),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    Line::from(spans)
}

fn draw_status_line(frame: &mut Frame, area: Rect, snapshot: &PokerPublicSnapshot) {
    if area.height == 0 {
        return;
    }
    let tone = if snapshot.phase == PokerPhase::Showdown {
        theme::SUCCESS()
    } else {
        theme::TEXT()
    };
    let line = Line::from(vec![
        Span::styled("* ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(snapshot.status_message.clone(), Style::default().fg(tone)),
    ]);
    frame.render_widget(Paragraph::new(line).alignment(Alignment::Center), area);
}

fn draw_keys_bar(frame: &mut Frame, area: Rect, state: &State, snapshot: &PokerPublicSnapshot) {
    if area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(key_line(state, snapshot)).alignment(Alignment::Center),
        area,
    );
}

fn draw_table_compact(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    snapshot: &PokerPublicSnapshot,
    usernames: &HashMap<Uuid, String>,
) {
    let block = Block::default()
        .title(" Poker ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::from(vec![
            Span::styled("Board: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                render_cards_compact(&snapshot.community),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
        ]),
        Line::from(vec![
            Span::styled("Phase: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                snapshot.phase.label(),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Pot: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                snapshot.pot.to_string(),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled("  Bet: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                snapshot.current_bet.to_string(),
                Style::default().fg(theme::AMBER()),
            ),
        ]),
        Line::raw(""),
    ];
    lines.extend(snapshot.seats.iter().map(|seat| {
        let is_you = state.seat_index() == Some(seat.index);
        compact_seat_line(state, snapshot, seat, is_you, usernames)
    }));
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        snapshot.status_message.clone(),
        Style::default().fg(theme::TEXT()),
    )));
    lines.push(key_line(state, snapshot));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn compact_seat_line(
    state: &State,
    snapshot: &PokerPublicSnapshot,
    seat: &PokerSeat,
    is_you: bool,
    usernames: &HashMap<Uuid, String>,
) -> Line<'static> {
    let label = if is_you {
        format!("Seat {} You", seat.index + 1)
    } else {
        format!("Seat {}", seat.index + 1)
    };
    let name = seat
        .user_id
        .and_then(|user_id| usernames.get(&user_id).cloned())
        .unwrap_or_else(|| "open".to_string());
    let label_style = if is_you {
        Style::default().fg(theme::SUCCESS())
    } else if snapshot.active_seat == Some(seat.index) {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let cards = compact_seat_cards_text(state, seat);
    let action = seat
        .last_action
        .map(action_label)
        .unwrap_or(if seat.folded { "folded" } else { "" });
    let stack = if seat.user_id.is_some() {
        format!(" bal {:<5}", seat.balance)
    } else {
        String::new()
    };
    Line::from(vec![
        Span::styled(format!("{label:<11}"), label_style),
        Span::styled(format!("{name:<12}"), Style::default().fg(theme::TEXT())),
        Span::styled(cards, Style::default().fg(theme::TEXT_BRIGHT())),
        Span::raw(" "),
        Span::styled(stack, Style::default().fg(theme::SUCCESS())),
        Span::styled(action.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn compact_seat_cards_text(state: &State, seat: &PokerSeat) -> String {
    if seat.card_count == 0 {
        return ".".to_string();
    }
    let is_you = state.seat_index() == Some(seat.index);
    let theme_card = AsciiCardTheme::Minimal;
    (0..seat.card_count.min(2))
        .map(|index| {
            if let Some(card) =
                revealed_or_private_card(seat, &state.private_snapshot().hole_cards, is_you, index)
            {
                format!("[{}]", theme_card.render_face_compact(card).trim())
            } else {
                format!("[{}]", theme_card.render_back_compact().trim())
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn revealed_or_private_card(
    seat: &PokerSeat,
    private_cards: &[PlayingCard],
    is_you: bool,
    index: usize,
) -> Option<PlayingCard> {
    seat.revealed_cards
        .as_ref()
        .and_then(|cards| cards.get(index))
        .copied()
        .or_else(|| is_you.then(|| private_cards.get(index).copied()).flatten())
}

fn key_line(state: &State, snapshot: &PokerPublicSnapshot) -> Line<'static> {
    if !state.is_seated() {
        return key_hint("s/Enter sit - Esc back", "");
    }
    match snapshot.phase {
        PokerPhase::PostingBlinds => key_hint("posting blinds - L leave - Esc back", ""),
        PokerPhase::Waiting | PokerPhase::Showdown => {
            key_hint("N deal next - L leave - Esc back", "")
        }
        PokerPhase::PreFlop | PokerPhase::Flop | PokerPhase::Turn | PokerPhase::River
            if state.can_act() =>
        {
            if state.to_call() > 0 {
                if state.can_raise() {
                    key_hint(
                        &format!(
                            "C call {} - R raise +{} - A all-in - F fold - [/] raise",
                            state.to_call(),
                            state.selected_raise().max(state.min_raise())
                        ),
                        "",
                    )
                } else if state.can_all_in() {
                    key_hint(
                        &format!("C call {} - A all-in - F fold", state.to_call()),
                        "",
                    )
                } else {
                    key_hint(&format!("C call {} - F fold", state.to_call()), "")
                }
            } else {
                if state.can_raise() {
                    key_hint(
                        &format!(
                            "C check - B bet {} - A all-in - F fold - [/] bet",
                            state.selected_raise().max(state.min_raise())
                        ),
                        "",
                    )
                } else {
                    key_hint("C check - F fold", "")
                }
            }
        }
        PokerPhase::PreFlop | PokerPhase::Flop | PokerPhase::Turn | PokerPhase::River => {
            key_hint("waiting action - L leave - Esc back", "")
        }
    }
}

fn action_label(action: PokerAction) -> &'static str {
    match action {
        PokerAction::SmallBlind => "small blind",
        PokerAction::BigBlind => "big blind",
        PokerAction::Check => "check",
        PokerAction::Call => "call",
        PokerAction::Bet => "bet",
        PokerAction::Raise => "raise",
        PokerAction::Fold => "fold",
        PokerAction::AllIn => "all-in",
    }
}

fn render_cards_compact(cards: &[PlayingCard]) -> String {
    if cards.is_empty() {
        return ". . . . .".to_string();
    }
    let theme_card = AsciiCardTheme::Minimal;
    cards
        .iter()
        .map(|card| format!("[{}]", theme_card.render_face_compact(*card).trim()))
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

fn card_width(theme_card: AsciiCardTheme) -> usize {
    match theme_card {
        AsciiCardTheme::Minimal => 3,
        AsciiCardTheme::Boxed => 5,
        AsciiCardTheme::Outline => 9,
    }
}

fn card_color(card: PlayingCard) -> ratatui::style::Color {
    match card.suit {
        CardSuit::Hearts | CardSuit::Diamonds => theme::ERROR(),
        CardSuit::Clubs | CardSuit::Spades => theme::TEXT_BRIGHT(),
    }
}
