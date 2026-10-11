//! Card rows for the multi-hand card games (cribbage, gin rummy), drawn with
//! the shared `games/cards.rs` faces. A hand is fanned: every card but the
//! last shows only its left edge, which is where the rank and suit sit, so
//! eleven cards fit a terminal. Briscola never holds more than three and
//! lays its cards out whole; it keeps its own row code.
//!
//! The renderer records what it drew as `CardSlots` and the mouse reads the
//! same record back, so a click can never land on a card the picture did not
//! put there.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::{
    common::theme,
    games::cards::{AsciiCardTheme, CardSuit},
    lobby::daily::std_deck::Card,
};

/// Card size, straight from the shared renderer's themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Five-line boxed faces.
    Full,
    /// One-line faces, for a board sharing the screen with chat.
    Compact,
}

impl Tier {
    pub const fn theme(self) -> AsciiCardTheme {
        match self {
            Self::Full => AsciiCardTheme::Outline,
            Self::Compact => AsciiCardTheme::Boxed,
        }
    }

    pub fn card_h(self) -> u16 {
        self.theme().card_height() as u16
    }

    /// Every theme draws its cards the same width, so measuring the empty
    /// slot measures them all.
    pub fn card_w(self) -> u16 {
        self.theme().render_empty_lines()[0].chars().count() as u16
    }

    /// Columns each covered card of a fan shows: enough for `10♥`.
    pub const fn fan_step(self) -> u16 {
        match self {
            Self::Full => 5,
            Self::Compact => 4,
        }
    }

    /// Width of a fan of `count` cards.
    pub fn fan_width(self, count: usize) -> u16 {
        match count {
            0 => 0,
            count => self.fan_step() * (count as u16 - 1) + self.card_w(),
        }
    }
}

/// What one card position shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Up(Card),
    Back,
    /// An empty outline: a place a card could be.
    Empty,
}

/// The cards of one drawn row, as the mouse needs them: where the fan
/// starts, how far apart the cards are, and how many there are. Any column
/// past the last card's left edge is the last card, since it is drawn whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardSlots {
    pub rect: Rect,
    pub step: u16,
    pub count: usize,
}

impl CardSlots {
    pub fn at(&self, x: u16, y: u16) -> Option<usize> {
        let inside = x >= self.rect.x
            && y >= self.rect.y
            && x < self.rect.x + self.rect.width
            && y < self.rect.y + self.rect.height;
        if !inside || self.count == 0 {
            return None;
        }
        Some((((x - self.rect.x) / self.step.max(1)) as usize).min(self.count - 1))
    }
}

pub fn suit_color(card: Card) -> Color {
    match card.suit {
        CardSuit::Hearts | CardSuit::Diamonds => theme::ERROR(),
        CardSuit::Clubs | CardSuit::Spades => theme::TEXT_BRIGHT(),
    }
}

/// The style a face card wears: its suit colour, lifted onto the selection
/// background under the cursor and picked out in amber when marked.
pub fn face_style(card: Card, cursor: bool, marked: bool) -> Style {
    let base = match (cursor, marked) {
        (true, _) => theme::selection_style().add_modifier(Modifier::BOLD),
        (false, true) => Style::default()
            .bg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
        (false, false) => Style::default(),
    };
    base.fg(suit_color(card))
}

pub fn back_style() -> Style {
    Style::default().fg(theme::BORDER_DIM())
}

pub fn empty_style() -> Style {
    Style::default().fg(theme::TEXT_FAINT())
}

/// One fanned row. Every card but the last is cut to `tier.fan_step()`
/// columns; the last is drawn whole.
pub fn fan(cards: &[(Face, Style)], tier: Tier) -> Vec<Line<'static>> {
    let theme = tier.theme();
    let drawn: Vec<Vec<String>> = cards
        .iter()
        .map(|(face, _)| match face {
            Face::Up(card) => theme.render_face_lines(card.playing_card()),
            Face::Back => theme.render_back_lines(),
            Face::Empty => theme.render_empty_lines(),
        })
        .collect();
    (0..tier.card_h() as usize)
        .map(|row| {
            let spans = drawn
                .iter()
                .zip(cards)
                .enumerate()
                .map(|(index, (lines, (_, style)))| {
                    let line = &lines[row];
                    let text = if index + 1 == cards.len() {
                        line.clone()
                    } else {
                        line.chars().take(tier.fan_step() as usize).collect()
                    };
                    Span::styled(text, *style)
                })
                .collect::<Vec<_>>();
            Line::from(spans)
        })
        .collect()
}

/// Pad a row of lines on the left so it sits centred in `width`.
pub fn centre(lines: Vec<Line<'static>>, drawn: u16, width: u16) -> Vec<Line<'static>> {
    let pad = " ".repeat(width.saturating_sub(drawn) as usize / 2);
    lines
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::raw(pad.clone())];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// A dim single line centred in `width`.
pub fn label(text: String, width: u16, color: Color) -> Line<'static> {
    let pad = (width as usize).saturating_sub(text.chars().count()) / 2;
    Line::from(Span::styled(
        format!("{}{text}", " ".repeat(pad)),
        Style::default().fg(color),
    ))
}

/// `▲` under the card at `index` of a fan that starts `left` columns in.
pub fn cursor_marker(left: u16, index: usize, count: usize, tier: Tier) -> Line<'static> {
    let card_left = left + tier.fan_step() * index as u16;
    // A covered card shows only its edge; point at the middle of that.
    let under = if index + 1 == count {
        tier.card_w() / 2
    } else {
        tier.fan_step() / 2
    };
    Line::from(Span::styled(
        format!("{}▲", " ".repeat((card_left + under) as usize)),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

/// `10♥ J♥ Q♥`, each in its suit colour.
pub fn card_spans(cards: &[Card]) -> Vec<Span<'static>> {
    let mut spans = Vec::with_capacity(cards.len() * 2);
    for (index, card) in cards.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            card.label(),
            Style::default().fg(suit_color(*card)),
        ));
    }
    spans
}

#[cfg(test)]
#[path = "hand_ui_test.rs"]
mod hand_ui_test;
