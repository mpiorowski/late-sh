use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::{Card, Focus, Mode, Selection, State, Suit, TableauCard};
use crate::app::common::theme;
use crate::app::games::cards::{
    AsciiCardTheme, CardRank, CardSuit, OUTLINE_CARD_WIDTH, PlayingCard,
};
use crate::app::games::ui::{
    draw_game_frame, draw_game_overlay, info_label_value, info_tagline, key_hint,
};

const SOLITAIRE_CARD_THEME: AsciiCardTheme = AsciiCardTheme::Outline;
const FACE_DOWN_PEEK_LINES: usize = 1;
const FACE_UP_PEEK_LINES: usize = 2;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let info_lines = vec![
        info_tagline("Klondike solitaire."),
        Line::from(""),
        info_label_value(
            "Mode",
            match state.mode {
                Mode::Daily => "daily".to_string(),
                Mode::Personal => "personal".to_string(),
            },
            theme::AMBER_GLOW(),
        ),
        info_label_value(
            "Difficulty",
            state.difficulty_key().to_string(),
            theme::SUCCESS(),
        ),
        info_label_value(
            "Draw",
            format!(
                "{} card{}",
                state.draw_count(),
                if state.draw_count() == 1 { "" } else { "s" }
            ),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            "Progress",
            format!("{}/52", state.score()),
            theme::SUCCESS(),
        ),
        info_label_value(
            "Cards",
            SOLITAIRE_CARD_THEME.name().to_string(),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value("Stock", state.stock.len().to_string(), theme::TEXT_BRIGHT()),
        info_label_value("Cursor", state.cursor_label(), theme::TEXT_BRIGHT()),
        info_label_value("Selected", state.selection_label(), theme::TEXT_BRIGHT()),
        Line::from(""),
        key_hint("h/j/k/l", "move"),
        key_hint("Space", "select/place"),
        key_hint("a", "auto-place"),
        key_hint("f", "auto-foundation all"),
        key_hint("u", "undo"),
        key_hint("c", "deselect"),
        key_hint("d/p/n", "daily/pers/new"),
        key_hint("[ ]", "draw mode"),
        key_hint("{ }", "scroll"),
        key_hint("r", "reset"),
        key_hint("Esc", "exit"),
        Line::from(""),
        Line::from(Span::styled(
            "Selection tips",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        info_tagline("Click a face-down card to"),
        info_tagline("select the visible stack."),
        info_tagline("Select + click any column"),
        info_tagline("to place on that column."),
    ];

    let board_area = draw_game_frame(frame, area, "Solitaire", info_lines);
    let board_width = 78.min(board_area.width);
    let board_height = 44.min(board_area.height);
    let board_rect = Rect {
        x: board_area.x + (board_area.width.saturating_sub(board_width)) / 2,
        y: board_area.y,
        width: board_width,
        height: board_height,
    };
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
    lines.push(Line::from(vec![
        header_span("ST", matches!(state.cursor, Focus::Stock), false, None),
        Span::raw(gap),
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
    let waste_lines = waste_lines(state);
    let foundation_lines = [
        pile_lines(state.foundation_top(0)),
        pile_lines(state.foundation_top(1)),
        pile_lines(state.foundation_top(2)),
        pile_lines(state.foundation_top(3)),
    ];

    for idx in 0..SOLITAIRE_CARD_THEME.card_height() {
        lines.push(Line::from(vec![
            styled_span(
                stock_lines[idx].clone(),
                matches!(state.cursor, Focus::Stock),
                false,
                None,
            ),
            Span::raw(gap),
            styled_span(
                waste_lines[idx].clone(),
                matches!(state.cursor, Focus::Waste),
                matches!(state.selection, Some(Selection::Waste)),
                state.visible_waste().last().map(|card| card.suit),
            ),
            Span::raw(gap),
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
        ]));
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

fn waste_lines(state: &State) -> Vec<String> {
    let Some(card) = state.visible_waste().last().copied() else {
        return SOLITAIRE_CARD_THEME.render_empty_lines();
    };
    SOLITAIRE_CARD_THEME.render_face_lines(to_playing_card(card))
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
