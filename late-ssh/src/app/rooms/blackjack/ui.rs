use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{
    common::theme,
    games::{
        cards::AsciiCardTheme,
        ui::{draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint},
    },
    rooms::blackjack::state::{BlackjackSnapshot, Outcome, Phase, State},
};

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
            "Bet",
            snapshot
                .current_bet_amount
                .map(|bet| bet.to_string())
                .unwrap_or_else(|| {
                    if snapshot.bet_input.is_empty() {
                        "—".to_string()
                    } else {
                        snapshot.bet_input.clone()
                    }
                }),
            theme::AMBER_GLOW(),
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
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(render_seats(snapshot, user_seat_index)).block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme::BORDER_DIM())),
        ),
        rows[0],
    );

    let dealer_cards = render_cards(&snapshot.dealer_hand, snapshot.dealer_revealed);
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
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(render_seat_hands(snapshot, user_seat_index)),
        rows[4],
    );
    frame.render_widget(
        Paragraph::new(snapshot.status_message.as_str()).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(theme::BORDER_DIM())),
        ),
        rows[6],
    );

    if let Some((title, subtitle)) = &snapshot.outcome_banner {
        let color = match snapshot.last_outcome {
            Some(Outcome::PlayerBlackjack | Outcome::PlayerWin | Outcome::Push) => theme::SUCCESS(),
            Some(Outcome::DealerWin) | None => theme::ERROR(),
        };
        draw_game_overlay(frame, inner, title.as_str(), subtitle.as_str(), color);
    }
}

fn key_line(phase: Phase, is_seated: bool, is_active: bool) -> Line<'static> {
    if !is_seated {
        return key_hint("s Enter / Esc", "sit / leave");
    }
    match phase {
        Phase::Betting => key_hint("0-9 Enter / l / Esc", "bet / leave seat / leave"),
        Phase::BetPending => key_hint("wait", "bet in flight"),
        Phase::PlayerTurn if is_active => key_hint(
            "h Space / s / l / Esc",
            "hit / stand / leave seat / auto-stand+leave",
        ),
        Phase::PlayerTurn => key_hint("wait / l / Esc", "watch hand / leave seat / leave"),
        Phase::DealerTurn => key_hint("wait", "dealer resolving"),
        Phase::Settling => key_hint("n Enter / l / Esc", "next hand / leave seat / leave"),
    }
}

fn render_seats(snapshot: &BlackjackSnapshot, user_seat_index: Option<usize>) -> Line<'static> {
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
            Some(_) if seat.phase == crate::app::rooms::blackjack::state::SeatPhase::Playing => {
                format!("[{} Play]", seat.index + 1)
            }
            Some(_) => format!("[{} Taken]", seat.index + 1),
            None => format!("[{} Open]", seat.index + 1),
        };
        let style = match seat.user_id {
            Some(_) if Some(seat.index) == user_seat_index => Style::default().fg(theme::SUCCESS()),
            Some(_) if seat.phase == crate::app::rooms::blackjack::state::SeatPhase::Playing => {
                Style::default().fg(theme::AMBER())
            }
            Some(_) => Style::default().fg(theme::TEXT()),
            None => Style::default().fg(theme::TEXT_DIM()),
        };
        spans.push(Span::styled(label, style));
    }
    Line::from(spans)
}

fn render_seat_hands(
    snapshot: &BlackjackSnapshot,
    user_seat_index: Option<usize>,
) -> Vec<Line<'static>> {
    snapshot
        .seats
        .iter()
        .map(|seat| {
            let label = if Some(seat.index) == user_seat_index {
                format!("Seat {} You", seat.index + 1)
            } else if seat.phase == crate::app::rooms::blackjack::state::SeatPhase::Playing {
                format!("Seat {} Play", seat.index + 1)
            } else {
                format!("Seat {}", seat.index + 1)
            };
            let label_style = if Some(seat.index) == user_seat_index {
                Style::default().fg(theme::SUCCESS())
            } else if seat.phase == crate::app::rooms::blackjack::state::SeatPhase::Playing {
                Style::default().fg(theme::AMBER())
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            let hand = if seat.hand.is_empty() {
                "—".to_string()
            } else {
                render_cards(&seat.hand, true)
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

fn render_cards(cards: &[crate::app::games::cards::PlayingCard], reveal_all: bool) -> String {
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
