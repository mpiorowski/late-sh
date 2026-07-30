//! Full-screen daily briscola board: your hand across the bottom, the trump
//! and the table in the middle, the opponent's hand as backs along the top.
//! Shares the daily board chrome (status line, player bars, pinned key
//! hints) with the chess and connect four renderers.
//!
//! This is the file that keeps briscola's hidden information hidden. The
//! match state holds both hands and the whole stock (see `briscola.rs`), so
//! the rule lives here and nowhere else: faces are drawn for the viewer's
//! own hand only, and a spectator gets no faces at all.
//!
//! Two size tiers, both from the shared card renderer in `games/cards.rs`:
//! the five-line `Outline` cards when there is room, one-line `Boxed` cards
//! when there is not.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    games::cards::{AsciiCardTheme, CardSuit},
    lobby::daily::{
        board_ui::{draw_center_message, name_for, result_banner},
        briscola::{self, Card, DailyBriscolaState, Table, other},
        state::{BriscolaDetail, DailyBoardState, DailyMatchDetail, DailyState, format_deadline},
    },
};

/// status + two player bars + key hints around the table.
const CHROME_ROWS: u16 = 4;
const INFO_RAIL_WIDTH: u16 = 26;
/// Breathing room required around the table before the rail appears.
const INFO_RAIL_MIN_EXTRA: u16 = 6;

/// Card size, straight from the shared renderer's themes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tier {
    /// Five-line boxed faces.
    Full,
    /// One-line faces, for a board sharing the screen with chat.
    Compact,
}

impl Tier {
    const fn theme(self) -> AsciiCardTheme {
        match self {
            Self::Full => AsciiCardTheme::Outline,
            Self::Compact => AsciiCardTheme::Boxed,
        }
    }

    fn card_h(self) -> u16 {
        self.theme().card_height() as u16
    }

    /// Every theme draws all its cards the same width, so measuring the empty
    /// slot measures them all.
    fn card_w(self) -> u16 {
        self.theme().render_empty_lines()[0].chars().count() as u16
    }

    /// One card plus its gutter.
    fn slot_w(self) -> u16 {
        self.card_w() + 1
    }

    /// Where the hand block starts inside the table, which is wider than it.
    fn hand_x(self) -> u16 {
        (self.table_width() - self.slot_w() * briscola::HAND as u16) / 2
    }

    /// opponent hand + gap + table + its label + gap + own hand + the cursor
    /// marker row.
    fn table_rows(self) -> u16 {
        self.card_h() * 3 + 4
    }

    /// The middle row is the widest: trump, a gap, then the two-card trick.
    fn table_width(self) -> u16 {
        self.slot_w() * 4
    }
}

/// What a card slot shows. `Back` and `Empty` are how the opponent's hand and
/// the untaken half of a trick are drawn; no card ever reaches them.
enum Slot {
    Face(Card),
    Back,
    /// An empty card outline: a slot a card could be in.
    Empty,
    /// Blank space: no slot at all.
    Gap,
}

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    briscola: &BriscolaDetail,
) {
    let tier = if area.height >= Tier::Full.table_rows() + CHROME_ROWS
        && area.width >= Tier::Full.table_width()
    {
        Tier::Full
    } else {
        Tier::Compact
    };
    if area.width < tier.table_width() || area.height < tier.table_rows() + CHROME_ROWS {
        draw_center_message(frame, area, "The board needs more room.");
        return;
    }

    let state = &briscola.state;
    let table = state.table();
    // A spectator holds no seat; show them the match from seat 0's side, with
    // both hands face down.
    let my_seat = state.seat_of(daily.user_id()).unwrap_or(0);

    let show_rail = area.width >= tier.table_width() + INFO_RAIL_WIDTH + INFO_RAIL_MIN_EXTRA;
    let area = if show_rail {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_RAIL_WIDTH)])
            .split(area);
        draw_info_rail(frame, cols[1], daily, board, state, &table, my_seat);
        cols[0]
    } else {
        area
    };

    let stack_h = tier.table_rows() + CHROME_ROWS;
    let top_pad = area.height.saturating_sub(stack_h) / 2;
    let rows = Layout::vertical([
        Constraint::Length(top_pad),
        Constraint::Length(1),                 // status
        Constraint::Length(1),                 // opponent bar
        Constraint::Length(tier.table_rows()), // the table
        Constraint::Length(1),                 // own bar
        Constraint::Min(0),                    // slack, pushing the hints down
        Constraint::Length(1),                 // key hints
    ])
    .split(area);
    let (status_row, top_bar, table_row, bottom_bar, hint_row) =
        (rows[1], rows[2], rows[3], rows[4], rows[6]);

    let my_turn = detail.is_active()
        && detail.row.turn_user_id == Some(daily.user_id())
        && !briscola.play_in_flight
        && !board.spectating;

    let table_x = table_row.x + table_row.width.saturating_sub(tier.table_width()) / 2;
    let over_table = |row: Rect| Rect {
        x: table_x,
        y: row.y,
        width: tier.table_width().min(row.width),
        height: row.height,
    };

    frame.render_widget(
        Paragraph::new(status_line(daily, board, detail, briscola, &table))
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
        other(my_seat),
    );

    let table_rect = Rect {
        x: table_x,
        y: table_row.y,
        width: tier.table_width(),
        height: tier.table_rows(),
    };
    frame.render_widget(
        Paragraph::new(table_lines(
            state,
            &table,
            my_seat,
            my_turn.then_some(board.cursor),
            board.spectating,
            tier,
            middle_label(board, state, &table, my_seat),
        )),
        table_rect,
    );
    // The hand block is the last `card_h + 1` rows; the marker row sits under
    // it. Fixed `HAND` slots, so the click math holds however few cards are
    // left (`board_input.rs` divides the width by `HAND`).
    board.target_geometry.set(Some(Rect {
        x: table_rect.x + tier.hand_x(),
        y: table_rect.y + tier.card_h() * 2 + 3,
        width: tier.slot_w() * briscola::HAND as u16,
        height: tier.card_h(),
    }));

    draw_player_bar(
        frame,
        over_table(bottom_bar),
        daily,
        board,
        detail,
        state,
        my_seat,
    );
    frame.render_widget(
        Paragraph::new(key_line(board, detail)).alignment(Alignment::Center),
        hint_row,
    );
}

fn suit_color(card: Card) -> Color {
    match card.suit {
        CardSuit::Hearts | CardSuit::Diamonds => theme::ERROR(),
        CardSuit::Clubs | CardSuit::Spades => theme::TEXT_BRIGHT(),
    }
}

/// The whole table: the opponent's backs, the trump and the trick, then your
/// hand with the cursor marker under it. The middle label arrives pre-built
/// because naming the players needs the board, which this stays free of.
fn table_lines(
    state: &DailyBriscolaState,
    table: &Table,
    my_seat: usize,
    cursor: Option<usize>,
    spectating: bool,
    tier: Tier,
    middle: String,
) -> Vec<Line<'static>> {
    let width = tier.table_width() as usize;
    let mut lines = Vec::new();

    // Their hand: a count of backs, never a face.
    let their_hand: Vec<(Slot, Style)> = (0..briscola::HAND)
        .map(|index| {
            let slot = if index < table.hands[other(my_seat)].len() {
                Slot::Back
            } else {
                Slot::Empty
            };
            (slot, Style::default().fg(theme::BORDER_DIM()))
        })
        .collect();
    lines.extend(centered(card_row(&their_hand, tier), width));
    lines.push(Line::raw(""));

    lines.extend(centered(middle_row(state, table, tier), width));
    lines.push(centered_label(middle, width));
    lines.push(Line::raw(""));

    // Your hand, face up, unless you are only watching.
    let my_hand: Vec<(Slot, Style)> = (0..briscola::HAND)
        .map(|index| match table.hands[my_seat].get(index) {
            Some(_) if spectating => (Slot::Back, Style::default().fg(theme::BORDER_DIM())),
            Some(&card) => {
                let mut style = Style::default().fg(suit_color(card));
                if cursor == Some(index) {
                    style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
                }
                (Slot::Face(card), style)
            }
            None => (Slot::Empty, Style::default().fg(theme::TEXT_FAINT())),
        })
        .collect();
    lines.extend(centered(card_row(&my_hand, tier), width));
    lines.push(cursor_marker(cursor, tier));
    lines
}

/// Trump on the left with the stock count under it, the trick on the right.
fn middle_row(state: &DailyBriscolaState, table: &Table, tier: Tier) -> Vec<Line<'static>> {
    let trump = state.trump();
    // Once the trump has been drawn it is somebody's card; keep showing it so
    // the suit stays obvious, but faded.
    let trump_taken = table.drawn >= briscola::DRAWS;
    let trump_style = if trump_taken {
        Style::default().fg(theme::TEXT_FAINT())
    } else {
        Style::default().fg(suit_color(trump))
    };

    let (left, right) = trick_slots(table);
    // Trump against the left edge, the trick pair centred on the table, the
    // slack taken up on the right.
    let slots = vec![
        (Slot::Face(trump), trump_style),
        left,
        right,
        (Slot::Gap, Style::default()),
    ];
    card_row(&slots, tier)
}

/// The two cards on the table: the live trick if one is open, otherwise the
/// trick just taken, so a player coming back a day later can see what
/// happened. Leader on the left, answer on the right.
fn trick_slots(table: &Table) -> ((Slot, Style), (Slot, Style)) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let empty = (Slot::Empty, Style::default().fg(theme::TEXT_FAINT()));
    match (table.led, table.history.last()) {
        (Some((_, card)), _) => (
            (Slot::Face(card), Style::default().fg(suit_color(card))),
            empty,
        ),
        (None, Some(trick)) => (
            (Slot::Face(trick.lead), dim.fg(suit_color(trick.lead))),
            (Slot::Face(trick.answer), dim.fg(suit_color(trick.answer))),
        ),
        (None, None) => (
            empty,
            (Slot::Empty, Style::default().fg(theme::TEXT_FAINT())),
        ),
    }
}

/// The label under the trick. A player reads "you"/"they"; a spectator holds
/// no seat, so they read the players' names instead of being addressed as
/// seat 0. The fresh-match case names whoever actually leads, which is not
/// always the viewer.
fn middle_label(
    board: &DailyBoardState,
    state: &DailyBriscolaState,
    table: &Table,
    my_seat: usize,
) -> String {
    let subject = |seat: usize| {
        if board.spectating {
            name_for(board, state.user_of(seat))
        } else if seat == my_seat {
            "you".to_string()
        } else {
            "they".to_string()
        }
    };
    let possessive = |seat: usize| {
        if board.spectating {
            format!("{}'s", name_for(board, state.user_of(seat)))
        } else if seat == my_seat {
            "your".to_string()
        } else {
            "their".to_string()
        }
    };
    match (table.led, table.history.last()) {
        (Some((seat, _)), _) => format!("{} led", subject(seat)),
        (None, Some(trick)) => {
            format!(
                "last trick · {} took {}",
                subject(trick.winner),
                trick.points
            )
        }
        (None, None) => format!("{} lead", possessive(table.turn)),
    }
}

/// `▲` under the selected card.
fn cursor_marker(cursor: Option<usize>, tier: Tier) -> Line<'static> {
    let Some(cursor) = cursor else {
        return Line::raw("");
    };
    let pad = (tier.hand_x() + tier.slot_w() * cursor as u16 + tier.card_w() / 2) as usize;
    Line::from(Span::styled(
        format!("{}▲", " ".repeat(pad)),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

/// One row of cards: every slot contributes its line of the face, back, or
/// blank, plus a gutter.
fn card_row(slots: &[(Slot, Style)], tier: Tier) -> Vec<Line<'static>> {
    let theme = tier.theme();
    let drawn: Vec<Vec<String>> = slots
        .iter()
        .map(|(slot, _)| match slot {
            Slot::Face(card) => theme.render_face_lines(card.playing_card()),
            Slot::Back => theme.render_back_lines(),
            Slot::Empty => theme.render_empty_lines(),
            Slot::Gap => vec![" ".repeat(tier.card_w() as usize); tier.card_h() as usize],
        })
        .collect();
    (0..tier.card_h() as usize)
        .map(|row| {
            let mut spans = Vec::with_capacity(slots.len() * 2);
            for (lines, (_, style)) in drawn.iter().zip(slots.iter()) {
                spans.push(Span::styled(lines[row].clone(), *style));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect()
}

fn centered(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|line| centered_line(line, width))
        .collect()
}

fn centered_line(line: Line<'static>, width: usize) -> Line<'static> {
    let drawn: usize = line
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum();
    if drawn == 0 || drawn >= width {
        return line;
    }
    let mut spans = vec![Span::raw(" ".repeat((width - drawn) / 2))];
    spans.extend(line.spans);
    Line::from(spans)
}

fn centered_label(text: String, width: usize) -> Line<'static> {
    let pad = width.saturating_sub(text.chars().count()) / 2;
    Line::from(Span::styled(
        format!("{}{text}", " ".repeat(pad)),
        Style::default().fg(theme::TEXT_FAINT()),
    ))
}

fn status_line(
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    briscola: &BriscolaDetail,
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
    let mut spans = Vec::new();
    if detail.is_active() {
        if briscola.play_in_flight {
            spans.push(Span::styled(
                "Card away…",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else if detail.row.turn_user_id == Some(daily.user_id()) {
            spans.push(Span::styled(
                if table.led.is_some() {
                    "Your answer"
                } else {
                    "Your lead"
                },
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                format!(
                    "Waiting for {}",
                    name_for(board, detail.row.turn_user_id.unwrap_or(Uuid::nil()))
                ),
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::BOLD),
            ));
        }
        if let Some(deadline) = detail.row.turn_deadline_at {
            spans.push(Span::styled(
                format!("   {} on the clock", format_deadline(deadline, Utc::now())),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    } else {
        let (heading, subtitle, color) = result_banner(daily, board, detail);
        spans.push(Span::styled(
            format!("{heading} · {subtitle}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some((seat, card)) = table.last {
        let who = if briscola.state.user_of(seat) == daily.user_id() {
            "you".to_string()
        } else {
            name_for(board, briscola.state.user_of(seat))
        };
        spans.push(Span::styled(
            format!("   last {who} {}", card.label()),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

/// `● mira    38 pts · 4 tricks`, with the running deadline on the mover's
/// bar. Both scores are public here by design (see `briscola.rs`).
fn draw_player_bar(
    frame: &mut Frame,
    rect: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    detail: &DailyMatchDetail,
    state: &DailyBriscolaState,
    seat: usize,
) {
    if rect.height == 0 {
        return;
    }
    let table = state.table();
    let user_id = state.user_of(seat);
    let on_turn = detail.is_active() && detail.row.turn_user_id == Some(user_id);
    let dot_color = if on_turn {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT_FAINT()
    };
    let name = if user_id == daily.user_id() {
        "you".to_string()
    } else {
        name_for(board, user_id)
    };
    let points = table.points[seat];
    let points_color = if points >= briscola::WINNING_POINTS {
        theme::SUCCESS()
    } else {
        theme::TEXT()
    };
    let left = vec![
        Span::raw("  "),
        Span::styled("\u{25CF} ", Style::default().fg(dot_color)),
        Span::styled(
            format!("{name}  "),
            Style::default()
                .fg(theme::TEXT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{points} pts"), Style::default().fg(points_color)),
        Span::styled(
            format!(" · {} tricks", table.tricks[seat]),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
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

fn key_line(board: &DailyBoardState, detail: &DailyMatchDetail) -> Line<'static> {
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
        hint(&mut spans, "Space/Enter", "play");
        hint(&mut spans, "r", "resign");
    }
    if !board.spectating && detail.row.chat_room_id.is_some() {
        hint(&mut spans, "i", "chat");
    }
    hint(&mut spans, "Esc", "back to lobby");
    if let Some(last) = spans.last_mut() {
        let trimmed = last.content.trim_end().to_string();
        *last = Span::styled(trimmed, Style::default().fg(theme::TEXT_DIM()));
    }
    Line::from(spans)
}

/// Trick history rail: every completed trick in play order, newest at the
/// bottom, same slot the chess move list occupies.
fn draw_info_rail(
    frame: &mut Frame,
    area: Rect,
    daily: &DailyState,
    board: &DailyBoardState,
    state: &DailyBriscolaState,
    table: &Table,
    my_seat: usize,
) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Briscola".to_string(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(Span::styled(
            format!("trump {}", state.trump().label()),
            Style::default().fg(suit_color(state.trump())),
        )),
        Line::from(Span::styled(
            format!(
                "{} to draw · {} of {}",
                state.stock_remaining(),
                table.points[my_seat] + table.points[other(my_seat)],
                briscola::TOTAL_POINTS
            ),
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            "Tricks".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )),
    ];

    let budget = (area.height as usize).saturating_sub(lines.len());
    let mut tricks: Vec<Line<'static>> = table
        .history
        .iter()
        .map(|trick| {
            let winner = if state.user_of(trick.winner) == daily.user_id() {
                "you".to_string()
            } else {
                name_for(board, state.user_of(trick.winner))
            };
            Line::from(vec![
                Span::styled(
                    format!("{:<4}", trick.lead.label()),
                    Style::default().fg(suit_color(trick.lead)),
                ),
                Span::styled(
                    format!("{:<5}", trick.answer.label()),
                    Style::default().fg(suit_color(trick.answer)),
                ),
                Span::styled(
                    format!("{winner:<8}"),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled(
                    format!("{:>3}", trick.points),
                    Style::default().fg(theme::TEXT()),
                ),
            ])
        })
        .collect();

    if tricks.is_empty() {
        lines.push(Line::from(Span::styled(
            "no tricks yet",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        if tricks.len() > budget && budget > 0 {
            lines.push(Line::from(Span::styled(
                "  \u{22EE}",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
            let skip = tricks.len() - (budget - 1);
            tricks.drain(..skip);
        }
        lines.append(&mut tricks);
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
#[path = "briscola_ui_test.rs"]
mod briscola_ui_test;
