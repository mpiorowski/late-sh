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
        blackjack::state::{Outcome, Phase, State},
        cards::AsciiCardTheme,
        ui::{draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint},
    },
};

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let info_lines = vec![
        info_tagline("Single-player blackjack. Bet, draw, settle, repeat."),
        Line::from(""),
        info_label_value("Balance", state.balance.to_string(), theme::SUCCESS),
        info_label_value(
            "Bet",
            state
                .current_bet_amount()
                .map(|bet| bet.to_string())
                .unwrap_or_else(|| {
                    if state.bet_input.is_empty() {
                        "—".to_string()
                    } else {
                        state.bet_input.clone()
                    }
                }),
            theme::AMBER_GLOW,
        ),
        info_label_value("Phase", state.phase.label().to_string(), theme::TEXT_BRIGHT),
        Line::from(""),
        key_line(state.phase),
    ];

    let inner = draw_game_frame(frame, area, "Blackjack", info_lines);
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Min(0),
    ])
    .split(inner);

    let dealer_cards = render_cards(&state.dealer_hand, state.dealer_revealed());
    let dealer_total = state
        .dealer_score()
        .map(|score| score.total.to_string())
        .unwrap_or_else(|| "—".to_string());
    let player_cards = render_cards(&state.player_hand, true);
    let player_total = state
        .player_score()
        .map(|score| score.total.to_string())
        .unwrap_or_else(|| "—".to_string());

    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![
            Span::styled("Dealer: ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(dealer_cards, Style::default().fg(theme::TEXT_BRIGHT)),
            Span::raw(format!("   ({dealer_total})")),
        ])]),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from(vec![
            Span::styled("You:    ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(player_cards, Style::default().fg(theme::TEXT_BRIGHT)),
            Span::raw(format!("   ({player_total})")),
        ])]),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(state.status_message.as_str())
            .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(theme::BORDER_DIM))),
        rows[4],
    );

    if let Some((title, subtitle)) = state.outcome_banner() {
        let color = match state.last_outcome {
            Some(Outcome::PlayerBlackjack | Outcome::PlayerWin | Outcome::Push) => theme::SUCCESS,
            Some(Outcome::DealerWin) | None => theme::ERROR,
        };
        draw_game_overlay(frame, inner, title.as_str(), subtitle.as_str(), color);
    }
}

fn key_line(phase: Phase) -> Line<'static> {
    match phase {
        Phase::Betting => key_hint("0-9 Enter Esc", "bet / deal / leave"),
        Phase::BetPending => key_hint("wait", "bet in flight"),
        Phase::PlayerTurn => key_hint("h Space / s / Esc", "hit / stand / auto-stand+leave"),
        Phase::DealerTurn => key_hint("wait", "dealer resolving"),
        Phase::Settling => key_hint("any key / Esc", "next hand / leave"),
    }
}

fn render_cards(cards: &[crate::app::games::cards::PlayingCard], reveal_all: bool) -> String {
    let theme = AsciiCardTheme::Minimal;
    cards.iter()
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
