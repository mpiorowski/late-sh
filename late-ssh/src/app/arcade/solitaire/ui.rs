use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Card, Focus, Mode, Selection, State, Suit, TableauCard};
use crate::app::arcade::ui::{
    GameBottomBar, draw_game_frame, draw_game_overlay, game_content_area, keys_line, status_line,
    tip_line,
};
use crate::app::common::theme;
use crate::app::games::cards::{
    AsciiCardTheme, CardRank, CardSuit, OUTLINE_CARD_WIDTH, PlayingCard,
};

const SOLITAIRE_CARD_THEME: AsciiCardTheme = AsciiCardTheme::Outline;
const FACE_DOWN_PEEK_LINES: usize = 1;
const FACE_UP_PEEK_LINES: usize = 2;
const BOARD_WIDTH: u16 = 78;
const BOARD_HEIGHT: u16 = 44;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let mode_str = match state.mode {
        Mode::Daily => "daily".to_string(),
        Mode::Personal => "personal".to_string(),
    };

    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", mode_str, theme::AMBER_GLOW()),
            ("diff", state.difficulty_key().to_string(), theme::SUCCESS()),
            ("draw", state.draw_count().to_string(), theme::TEXT_BRIGHT()),
            ("score", format!("{}/52", state.score()), theme::SUCCESS()),
            ("stock", state.stock.len().to_string(), theme::TEXT_BRIGHT()),
            ("sel", state.selection_label(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(vec![
            ("h/j/k/l", "move"),
            ("Space", "select/place"),
            ("a", "auto"),
            ("u", "undo"),
            ("d/p/n", "new"),
            ("[ ]", "draw"),
            ("r", "reset"),
            ("g", "reroll"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: Some(tip_line(match state.reset_pending {
            Some(kind) => kind.confirm_tip(),
            None => "Pick a face-down card to grab the visible stack; pick a column to place it.",
        })),
    };

    let board_area = draw_game_frame(frame, area, "Solitaire", bottom, show_bottom_bar);
    let board_rect = solitaire_board_rect(board_area);
    let lines = board_lines(state);
    let lines: Vec<_> = lines
        .into_iter()
        .skip(state.scroll_offset as usize)
        .collect();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Left), board_rect);

    if state.is_game_over {
        let subtext = match state.mode {
            Mode::Daily => "Change diff via [ ]",
            Mode::Personal => "n for new",
        };
        draw_game_overlay(frame, board_area, "YOU WON!", subtext, theme::SUCCESS());
    }
}

pub fn hit_test(area: Rect, state: &State, x: u16, y: u16) -> Option<Focus> {
    let board_area = game_content_area(area, true, true);
    let board_rect = solitaire_board_rect(board_area);
    if !rect_contains(board_rect, x, y) {
        return None;
    }

    let local_x = x.saturating_sub(board_rect.x) as usize;
    let local_y = y.saturating_sub(board_rect.y) as usize + state.scroll_offset as usize;

    if SOLITAIRE_CARD_THEME.card_height() > 1 {
        hit_test_multiline(state, local_x, local_y)
    } else {
        hit_test_compact(state, local_x, local_y)
    }
}

pub fn hit_area(area: Rect) -> Rect {
    solitaire_board_rect(game_content_area(area, true, true))
}

fn solitaire_board_rect(board_area: Rect) -> Rect {
    let board_width = BOARD_WIDTH.min(board_area.width);
    let board_height = BOARD_HEIGHT.min(board_area.height);
    Rect {
        x: board_area.x + (board_area.width.saturating_sub(board_width)) / 2,
        y: board_area.y,
        width: board_width,
        height: board_height,
    }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn board_lines(state: &State) -> Vec<Line<'static>> {
    if SOLITAIRE_CARD_THEME.card_height() > 1 {
        return board_lines_multiline(state);
    }

    board_lines_compact(state)
}

fn board_lines_compact(state: &State) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        stock_span(
            "ST",
            state.stock.len(),
            matches!(state.cursor, Focus::Stock),
            false,
        ),
        Span::raw("  "),
        waste_span(state),
        Span::raw("  "),
        pile_span(
            "F1",
            state.foundation_top(0),
            matches!(state.cursor, Focus::Foundation(0)),
            matches!(state.selection, Some(Selection::Foundation(0))),
        ),
        Span::raw("  "),
        pile_span(
            "F2",
            state.foundation_top(1),
            matches!(state.cursor, Focus::Foundation(1)),
            matches!(state.selection, Some(Selection::Foundation(1))),
        ),
        Span::raw("  "),
        pile_span(
            "F3",
            state.foundation_top(2),
            matches!(state.cursor, Focus::Foundation(2)),
            matches!(state.selection, Some(Selection::Foundation(2))),
        ),
        Span::raw("  "),
        pile_span(
            "F4",
            state.foundation_top(3),
            matches!(state.cursor, Focus::Foundation(3)),
            matches!(state.selection, Some(Selection::Foundation(3))),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "T1     T2     T3     T4     T5     T6     T7",
        Style::default().fg(theme::TEXT_DIM()),
    )]));

    let height = state.max_tableau_height();
    for row in 0..height {
        let mut spans = Vec::new();
        for col in 0..7 {
            let card = state.visible_tableau_card(col, row);
            spans.push(tableau_span(state, col, row, card));
            if col < 6 {
                spans.push(Span::raw("  "));
            }
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn hit_test_compact(state: &State, x: usize, y: usize) -> Option<Focus> {
    if y == 0 {
        let slots = [
            (0usize, 5usize, Focus::Stock),
            (7, 21, Focus::Waste),
            (23, 28, Focus::Foundation(0)),
            (30, 35, Focus::Foundation(1)),
            (37, 42, Focus::Foundation(2)),
            (44, 49, Focus::Foundation(3)),
        ];
        return slots
            .into_iter()
            .find_map(|(start, end, focus)| (x >= start && x <= end).then_some(focus));
    }

    if y < 3 {
        return None;
    }

    let col = x / 7;
    let within = x % 7;
    if col < 7 && within < 5 {
        let row = y - 3;
        Some(Focus::Tableau(col, row.min(tableau_max_row(state, col))))
    } else {
        None
    }
}

fn stock_span(label: &str, remaining: usize, focused: bool, selected: bool) -> Span<'static> {
    let text = format!(
        "{label} {}",
        SOLITAIRE_CARD_THEME.render_stock_count_compact(remaining)
    );
    Span::styled(text, block_style(focused, selected, None))
}

fn top_card_text(card: Card) -> String {
    SOLITAIRE_CARD_THEME.render_face_compact(to_playing_card(card))
}

fn waste_span(state: &State) -> Span<'static> {
    let cards = state.visible_waste();
    let text = if cards.is_empty() {
        format!("WA {}", SOLITAIRE_CARD_THEME.render_empty_compact())
    } else {
        let labels = cards
            .iter()
            .map(|card| top_card_text(*card))
            .collect::<Vec<_>>()
            .join(" ");
        format!("WA {labels}")
    };

    Span::styled(
        text,
        block_style(
            matches!(state.cursor, Focus::Waste),
            matches!(state.selection, Some(Selection::Waste)),
            cards.last().map(|card| card.suit),
        ),
    )
}

fn pile_span(label: &str, value: Option<Card>, focused: bool, selected: bool) -> Span<'static> {
    let suit = value.map(|card| card.suit);
    let text = format!(
        "{label} {}",
        value
            .map(top_card_text)
            .unwrap_or_else(|| SOLITAIRE_CARD_THEME.render_empty_compact().to_string())
    );
    Span::styled(text, block_style(focused, selected, suit))
}

fn tableau_span(state: &State, col: usize, row: usize, card: Option<TableauCard>) -> Span<'static> {
    let focused = matches!(state.cursor, Focus::Tableau(cursor_col, cursor_row) if cursor_col == col && cursor_row == row);
    let selected = matches!(state.selection, Some(Selection::Tableau { col: selected_col, row: selected_row }) if selected_col == col && selected_row == row);
    match card {
        Some(TableauCard {
            card,
            face_up: true,
        }) => Span::styled(
            top_card_text(card),
            block_style(focused, selected, Some(card.suit)),
        ),
        Some(_) => Span::styled(
            SOLITAIRE_CARD_THEME.render_back_compact().to_string(),
            block_style(focused, selected, None).fg(theme::TEXT_DIM()),
        ),
        None => Span::styled(
            SOLITAIRE_CARD_THEME.render_empty_compact().to_string(),
            block_style(focused, selected, None).fg(theme::TEXT_FAINT()),
        ),
    }
}

fn board_lines_multiline(state: &State) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let gap = " ";
    let waste_pad = waste_fan_offset(state);
    lines.push(Line::from(vec![
        header_span("ST", matches!(state.cursor, Focus::Stock), false, None),
        Span::raw(gap),
        Span::raw(" ".repeat(waste_pad)),
        header_span(
            "WA",
            matches!(state.cursor, Focus::Waste),
            matches!(state.selection, Some(Selection::Waste)),
            state.visible_waste().last().map(|card| card.suit),
        ),
        Span::raw(gap),
        header_span(
            "F1",
            matches!(state.cursor, Focus::Foundation(0)),
            matches!(state.selection, Some(Selection::Foundation(0))),
            state.foundation_top(0).map(|card| card.suit),
        ),
        Span::raw(gap),
        header_span(
            "F2",
            matches!(state.cursor, Focus::Foundation(1)),
            matches!(state.selection, Some(Selection::Foundation(1))),
            state.foundation_top(1).map(|card| card.suit),
        ),
        Span::raw(gap),
        header_span(
            "F3",
            matches!(state.cursor, Focus::Foundation(2)),
            matches!(state.selection, Some(Selection::Foundation(2))),
            state.foundation_top(2).map(|card| card.suit),
        ),
        Span::raw(gap),
        header_span(
            "F4",
            matches!(state.cursor, Focus::Foundation(3)),
            matches!(state.selection, Some(Selection::Foundation(3))),
            state.foundation_top(3).map(|card| card.suit),
        ),
    ]));

    let stock_lines = SOLITAIRE_CARD_THEME.render_stock_count_lines(state.stock.len());
    let foundation_lines = [
        pile_lines(state.foundation_top(0)),
        pile_lines(state.foundation_top(1)),
        pile_lines(state.foundation_top(2)),
        pile_lines(state.foundation_top(3)),
    ];

    for idx in 0..SOLITAIRE_CARD_THEME.card_height() {
        let mut row: Vec<Span<'static>> = vec![
            styled_span(
                stock_lines[idx].clone(),
                matches!(state.cursor, Focus::Stock),
                false,
                None,
            ),
            Span::raw(gap),
        ];
        row.extend(waste_line_spans(state, idx));
        row.push(Span::raw(gap));
        row.extend([
            styled_span(
                foundation_lines[0][idx].clone(),
                matches!(state.cursor, Focus::Foundation(0)),
                matches!(state.selection, Some(Selection::Foundation(0))),
                state.foundation_top(0).map(|card| card.suit),
            ),
            Span::raw(gap),
            styled_span(
                foundation_lines[1][idx].clone(),
                matches!(state.cursor, Focus::Foundation(1)),
                matches!(state.selection, Some(Selection::Foundation(1))),
                state.foundation_top(1).map(|card| card.suit),
            ),
            Span::raw(gap),
            styled_span(
                foundation_lines[2][idx].clone(),
                matches!(state.cursor, Focus::Foundation(2)),
                matches!(state.selection, Some(Selection::Foundation(2))),
                state.foundation_top(2).map(|card| card.suit),
            ),
            Span::raw(gap),
            styled_span(
                foundation_lines[3][idx].clone(),
                matches!(state.cursor, Focus::Foundation(3)),
                matches!(state.selection, Some(Selection::Foundation(3))),
                state.foundation_top(3).map(|card| card.suit),
            ),
        ]);
        lines.push(Line::from(row));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        tableau_header_line(),
        Style::default().fg(theme::TEXT_DIM()),
    )]));

    let card_height = SOLITAIRE_CARD_THEME.card_height();
    let mut col_entries: [Vec<(usize, usize)>; 7] = std::array::from_fn(|_| Vec::new());
    for (col, entries) in col_entries.iter_mut().enumerate() {
        let pile = &state.tableau[col];
        if pile.is_empty() {
            for li in 0..card_height {
                entries.push((0, li));
            }
            continue;
        }
        for (row, tc) in pile.iter().enumerate() {
            let show = if row == pile.len() - 1 {
                card_height
            } else if tc.face_up {
                FACE_UP_PEEK_LINES
            } else {
                FACE_DOWN_PEEK_LINES
            };
            for li in 0..show {
                entries.push((row, li));
            }
        }
    }
    let stacked_height = col_entries
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(card_height);
    for line in 0..stacked_height {
        let mut spans = Vec::new();
        for (col, entries) in col_entries.iter().enumerate() {
            if line < entries.len() {
                let (row, li) = entries[line];
                let card = state.visible_tableau_card(col, row);
                spans.push(tableau_span_multiline(state, col, row, card, li));
            } else {
                spans.push(Span::raw(" ".repeat(OUTLINE_CARD_WIDTH)));
            }
            if col < 6 {
                spans.push(Span::raw(gap));
            }
        }
        lines.push(Line::from(spans));
    }

    lines
}

fn hit_test_multiline(state: &State, x: usize, y: usize) -> Option<Focus> {
    let card_height = SOLITAIRE_CARD_THEME.card_height();
    let top_end = card_height;
    if y <= top_end {
        return hit_test_top_row(state, x);
    }

    let tableau_header = card_height + 2;
    if y == tableau_header {
        return tableau_col_from_x(x).map(|col| Focus::Tableau(col, tableau_max_row(state, col)));
    }

    let tableau_start = card_height + 3;
    if y < tableau_start {
        return None;
    }

    let col = tableau_col_from_x(x)?;
    let line = y - tableau_start;
    tableau_row_at_visible_line(state, col, line)
        .map(|row| Focus::Tableau(col, row.min(tableau_max_row(state, col))))
}

fn hit_test_top_row(state: &State, x: usize) -> Option<Focus> {
    let card_width = OUTLINE_CARD_WIDTH;
    let waste_width = waste_fan_offset(state) + card_width;
    let stock_start = 0;
    let stock_end = stock_start + card_width;
    let waste_start = stock_end + 1;
    let waste_end = waste_start + waste_width;

    if x < stock_end {
        return Some(Focus::Stock);
    }
    if x >= waste_start && x < waste_end {
        return Some(Focus::Waste);
    }

    let mut start = waste_end + 1;
    for idx in 0..4 {
        if x >= start && x < start + card_width {
            return Some(Focus::Foundation(idx));
        }
        start += card_width + 1;
    }

    None
}

fn tableau_col_from_x(x: usize) -> Option<usize> {
    let stride = OUTLINE_CARD_WIDTH + 1;
    let col = x / stride;
    let within = x % stride;
    (col < 7 && within < OUTLINE_CARD_WIDTH).then_some(col)
}

fn tableau_row_at_visible_line(state: &State, col: usize, line: usize) -> Option<usize> {
    let pile = state.tableau.get(col)?;
    if pile.is_empty() {
        return (line < SOLITAIRE_CARD_THEME.card_height()).then_some(0);
    }

    let mut offset = 0usize;
    for (row, tc) in pile.iter().enumerate() {
        let shown_lines = if row == pile.len() - 1 {
            SOLITAIRE_CARD_THEME.card_height()
        } else if tc.face_up {
            FACE_UP_PEEK_LINES
        } else {
            FACE_DOWN_PEEK_LINES
        };
        if line < offset + shown_lines {
            return Some(row);
        }
        offset += shown_lines;
    }

    None
}

fn tableau_max_row(state: &State, col: usize) -> usize {
    state
        .tableau
        .get(col)
        .map_or(0, |pile| pile.len().saturating_sub(1))
}

fn header_span(label: &str, focused: bool, selected: bool, suit: Option<Suit>) -> Span<'static> {
    styled_span(
        format!("{label:^width$}", width = OUTLINE_CARD_WIDTH),
        focused,
        selected,
        suit,
    )
}

fn styled_span(text: String, focused: bool, selected: bool, suit: Option<Suit>) -> Span<'static> {
    Span::styled(text, block_style(focused, selected, suit))
}

fn styled_span_with_style(text: String, style: Style) -> Span<'static> {
    Span::styled(text, style)
}

fn tableau_header_line() -> String {
    (1..=7)
        .map(|idx| format!("{:^width$}", format!("T{idx}"), width = OUTLINE_CARD_WIDTH))
        .collect::<Vec<_>>()
        .join(" ")
}

fn pile_lines(card: Option<Card>) -> Vec<String> {
    card.map(|card| SOLITAIRE_CARD_THEME.render_face_lines(to_playing_card(card)))
        .unwrap_or_else(|| SOLITAIRE_CARD_THEME.render_empty_lines())
}

const WASTE_FAN_STEP: usize = 3;

fn waste_fan_offset(state: &State) -> usize {
    state.visible_waste().len().saturating_sub(1) * WASTE_FAN_STEP
}

fn waste_line_spans(state: &State, line_idx: usize) -> Vec<Span<'static>> {
    let cards = state.visible_waste();
    if cards.is_empty() {
        return vec![styled_span(
            SOLITAIRE_CARD_THEME.render_empty_lines()[line_idx].clone(),
            matches!(state.cursor, Focus::Waste),
            matches!(state.selection, Some(Selection::Waste)),
            None,
        )];
    }
    let mut spans = Vec::with_capacity(cards.len());
    let last = cards.len() - 1;
    for (i, card) in cards.iter().enumerate() {
        let card_lines = SOLITAIRE_CARD_THEME.render_face_lines(to_playing_card(*card));
        let line = &card_lines[line_idx];
        if i == last {
            spans.push(styled_span(
                line.clone(),
                matches!(state.cursor, Focus::Waste),
                matches!(state.selection, Some(Selection::Waste)),
                Some(card.suit),
            ));
        } else {
            let snippet: String = line.chars().take(WASTE_FAN_STEP).collect();
            spans.push(styled_span(snippet, false, false, Some(card.suit)));
        }
    }
    spans
}

fn tableau_span_multiline(
    state: &State,
    col: usize,
    row: usize,
    card: Option<TableauCard>,
    line_idx: usize,
) -> Span<'static> {
    let focused = matches!(state.cursor, Focus::Tableau(cursor_col, cursor_row) if cursor_col == col && cursor_row == row);
    let selected = matches!(state.selection, Some(Selection::Tableau { col: selected_col, row: selected_row }) if selected_col == col && selected_row == row);
    match card {
        Some(TableauCard {
            card,
            face_up: true,
        }) => styled_span(
            SOLITAIRE_CARD_THEME.render_face_lines(to_playing_card(card))[line_idx].clone(),
            focused,
            selected,
            Some(card.suit),
        ),
        Some(_) => styled_span_with_style(
            SOLITAIRE_CARD_THEME.render_back_lines()[line_idx].clone(),
            block_style(focused, selected, None).fg(theme::TEXT_DIM()),
        ),
        None => styled_span_with_style(
            SOLITAIRE_CARD_THEME.render_empty_lines()[line_idx].clone(),
            block_style(focused, selected, None).fg(theme::TEXT_FAINT()),
        ),
    }
}

fn block_style(focused: bool, selected: bool, suit: Option<Suit>) -> Style {
    let mut style = Style::default().fg(match suit {
        Some(Suit::Hearts | Suit::Diamonds) => theme::ERROR(),
        Some(_) => theme::TEXT_BRIGHT(),
        None => theme::TEXT(),
    });

    if selected {
        style = style.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD);
    }
    if focused {
        style = style.bg(theme::BG_HIGHLIGHT()).add_modifier(Modifier::BOLD);
    }
    style
}

fn to_playing_card(card: Card) -> PlayingCard {
    PlayingCard {
        suit: match card.suit {
            Suit::Hearts => CardSuit::Hearts,
            Suit::Diamonds => CardSuit::Diamonds,
            Suit::Clubs => CardSuit::Clubs,
            Suit::Spades => CardSuit::Spades,
        },
        rank: match card.rank {
            1 => CardRank::Ace,
            11 => CardRank::Jack,
            12 => CardRank::Queen,
            13 => CardRank::King,
            n => CardRank::Number(n),
        },
    }
}
