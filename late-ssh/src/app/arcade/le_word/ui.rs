use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use late_core::models::leaderboard::DailyPuzzle;

use super::state::{LetterScore, MAX_GUESSES, State, WORD_LEN};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, draw_rules_modal, keys_line, single_board_chips,
    status_line, tip_line,
};
use crate::app::common::theme;

const BOARD_WIDTH: u16 = 24;
const BOARD_HEIGHT: u16 = 13;
const BOARD_KEYBOARD_GAP: u16 = 2;
const KEYBOARD_WIDTH: u16 = 39;
const KEYBOARD_HEIGHT: u16 = 5;
const LETTER_KEY_WIDTH: u16 = 3;
const ACTION_KEY_WIDTH: u16 = 5;
const KEY_GAP: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyboardKey {
    Letter(char),
    Backspace,
    Enter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KeyRect {
    key: KeyboardKey,
    rect: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LeWordLayout {
    board: Rect,
    keyboard: Option<Rect>,
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("mode", "daily".to_string(), theme::AMBER_GLOW()),
            (
                "guess",
                format!("{}/{}", state.guesses.len().min(MAX_GUESSES), MAX_GUESSES),
                theme::SUCCESS(),
            ),
            ("reward", "250".to_string(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(
            vec![
                ("a-z", "type"),
                ("Backspace", "delete"),
                ("Enter", "guess"),
                ("?", "help"),
                ("!", "rules"),
                ("`", "dashboard"),
                ("Esc", "exit"),
            ]
            .into_iter()
            .chain(crate::app::arcade::ui::share_hints(super::share::is_ready(
                state,
            )))
            .collect(),
        ),
        tip: Some(tip_line(state.message.clone())),
    };

    let board_area = draw_game_frame(frame, area, "Le Word", bottom, show_bottom_bar);
    let layout = le_word_layout(board_area);
    frame.render_widget(
        Paragraph::new(board_lines(state))
            .alignment(Alignment::Center)
            .style(Style::default().fg(theme::TEXT_BRIGHT()).bg(Color::Reset)),
        layout.board,
    );
    if let Some(keyboard_rect) = layout.keyboard {
        draw_keyboard(frame, keyboard_rect, state);
    }

    if state.won {
        draw_result_panel(
            frame,
            board_area,
            layout.board,
            layout.keyboard,
            "YOU WON!",
            "Press s to share your card",
            theme::SUCCESS(),
        );
    } else if state.is_game_over {
        draw_result_panel(
            frame,
            board_area,
            layout.board,
            layout.keyboard,
            "GAME OVER",
            &state.answer.to_uppercase(),
            theme::ERROR(),
        );
    }

    if state.show_rules {
        draw_rules_modal(
            frame,
            board_area,
            58,
            16,
            "Le Word Rules",
            rules_lines(),
            "! / q / Esc closes",
        );
    }
}

fn rules_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Guess the hidden five-letter word in six tries."),
        Line::from("Each guess must be a valid word."),
        Line::from(""),
        Line::from(vec![
            score_tile('A', LetterScore::Correct),
            Span::raw("  correct letter, correct spot"),
        ]),
        Line::from(vec![
            score_tile('A', LetterScore::Present),
            Span::raw("  correct letter, wrong spot"),
        ]),
        Line::from(vec![
            score_tile('A', LetterScore::Absent),
            Span::raw("  letter not in the word"),
        ]),
        Line::from(""),
        Line::from("A new daily answer appears once per day."),
        Line::from(format!(
            "Solving the daily earns {} chips.",
            single_board_chips(DailyPuzzle::LeWord)
        )),
    ]
}

fn draw_result_panel(
    frame: &mut Frame,
    board_area: Rect,
    board_rect: Rect,
    keyboard_rect: Option<Rect>,
    heading: &str,
    subtitle: &str,
    color: Color,
) {
    let area = result_panel_area(board_area, board_rect, keyboard_rect);
    let panel = Paragraph::new(vec![
        Line::from(Span::styled(
            format!(" {heading} "),
            Style::default()
                .bg(color)
                .fg(Color::Reset)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            subtitle.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    frame.render_widget(Clear, area);
    frame.render_widget(panel, area);
}

fn result_panel_area(board_area: Rect, board_rect: Rect, keyboard_rect: Option<Rect>) -> Rect {
    let width = 28.min(board_area.width);
    let height = 4.min(board_area.height);
    let x = board_area.x + board_area.width.saturating_sub(width) / 2;
    let below_anchor = keyboard_rect.unwrap_or(board_rect);
    let below_y = below_anchor
        .y
        .saturating_add(below_anchor.height)
        .saturating_add(1);
    if below_y.saturating_add(height) <= board_area.y.saturating_add(board_area.height) {
        return Rect {
            x,
            y: below_y,
            width,
            height,
        };
    }

    if board_rect.y >= board_area.y.saturating_add(height).saturating_add(1) {
        return Rect {
            x,
            y: board_rect.y.saturating_sub(height).saturating_sub(1),
            width,
            height,
        };
    }

    centered_rect(board_area, width, height)
}

fn le_word_layout(area: Rect) -> LeWordLayout {
    let board_width = BOARD_WIDTH.min(area.width);
    let board_height = BOARD_HEIGHT.min(area.height);
    let can_show_keyboard = area.height
        >= board_height
            .saturating_add(BOARD_KEYBOARD_GAP)
            .saturating_add(KEYBOARD_HEIGHT)
        && area.width >= LETTER_KEY_WIDTH;
    let keyboard_height = if can_show_keyboard {
        KEYBOARD_HEIGHT
    } else {
        0
    };
    let content_height = board_height
        .saturating_add(if keyboard_height > 0 {
            BOARD_KEYBOARD_GAP
        } else {
            0
        })
        .saturating_add(keyboard_height)
        .min(area.height);
    let content_width = if can_show_keyboard {
        KEYBOARD_WIDTH.max(board_width).min(area.width)
    } else {
        board_width
    };
    let content = centered_rect(area, content_width, content_height);
    let board = Rect {
        x: content.x + content.width.saturating_sub(board_width) / 2,
        y: content.y,
        width: board_width,
        height: board_height,
    };
    let keyboard = (keyboard_height > 0).then_some(Rect {
        x: content.x
            + content
                .width
                .saturating_sub(KEYBOARD_WIDTH.min(content.width))
                / 2,
        y: board
            .y
            .saturating_add(board.height)
            .saturating_add(BOARD_KEYBOARD_GAP),
        width: KEYBOARD_WIDTH.min(content.width),
        height: keyboard_height,
    });

    LeWordLayout { board, keyboard }
}

fn draw_keyboard(frame: &mut Frame, area: Rect, state: &State) {
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Reset)),
        area,
    );
    for key_rect in keyboard_key_rects(area) {
        frame.render_widget(key_paragraph(state, key_rect.key), key_rect.rect);
    }
}

pub fn keyboard_hit_test(area: Rect, x: u16, y: u16) -> Option<KeyboardKey> {
    let keyboard = le_word_layout(area).keyboard?;
    keyboard_key_rects(keyboard)
        .into_iter()
        .find(|key| contains(key.rect, x, y))
        .map(|key| key.key)
}

fn keyboard_key_rects(area: Rect) -> Vec<KeyRect> {
    let rows = keyboard_rows();
    let row_step = if area.height >= KEYBOARD_HEIGHT { 2 } else { 1 };
    let mut rects = Vec::new();
    for (row_idx, row) in rows.iter().enumerate() {
        let y = area.y.saturating_add(row_idx as u16 * row_step);
        if y >= area.y.saturating_add(area.height) {
            break;
        }
        let row_width = keyboard_row_width(row).min(area.width);
        let mut x = area.x + area.width.saturating_sub(row_width) / 2;
        for (key_idx, key) in row.iter().copied().enumerate() {
            if key_idx > 0 {
                x = x.saturating_add(KEY_GAP);
            }
            let width = key_width(key).min(area.x.saturating_add(area.width).saturating_sub(x));
            if width == 0 {
                break;
            }
            rects.push(KeyRect {
                key,
                rect: Rect {
                    x,
                    y,
                    width,
                    height: 1,
                },
            });
            x = x.saturating_add(width);
        }
    }
    rects
}

fn keyboard_rows() -> [&'static [KeyboardKey]; 3] {
    static ROW_1: [KeyboardKey; 10] = [
        KeyboardKey::Letter('q'),
        KeyboardKey::Letter('w'),
        KeyboardKey::Letter('e'),
        KeyboardKey::Letter('r'),
        KeyboardKey::Letter('t'),
        KeyboardKey::Letter('y'),
        KeyboardKey::Letter('u'),
        KeyboardKey::Letter('i'),
        KeyboardKey::Letter('o'),
        KeyboardKey::Letter('p'),
    ];
    static ROW_2: [KeyboardKey; 9] = [
        KeyboardKey::Letter('a'),
        KeyboardKey::Letter('s'),
        KeyboardKey::Letter('d'),
        KeyboardKey::Letter('f'),
        KeyboardKey::Letter('g'),
        KeyboardKey::Letter('h'),
        KeyboardKey::Letter('j'),
        KeyboardKey::Letter('k'),
        KeyboardKey::Letter('l'),
    ];
    static ROW_3: [KeyboardKey; 9] = [
        KeyboardKey::Enter,
        KeyboardKey::Letter('z'),
        KeyboardKey::Letter('x'),
        KeyboardKey::Letter('c'),
        KeyboardKey::Letter('v'),
        KeyboardKey::Letter('b'),
        KeyboardKey::Letter('n'),
        KeyboardKey::Letter('m'),
        KeyboardKey::Backspace,
    ];
    [&ROW_1, &ROW_2, &ROW_3]
}

fn keyboard_row_width(row: &[KeyboardKey]) -> u16 {
    row.iter()
        .copied()
        .map(key_width)
        .sum::<u16>()
        .saturating_add(row.len().saturating_sub(1) as u16 * KEY_GAP)
}

fn key_width(key: KeyboardKey) -> u16 {
    match key {
        KeyboardKey::Letter(_) => LETTER_KEY_WIDTH,
        KeyboardKey::Backspace | KeyboardKey::Enter => ACTION_KEY_WIDTH,
    }
}

/// A scored letter key is the same tile as on the board; an unscored key and
/// the two action keys are a plain raised cap.
fn key_paragraph(state: &State, key: KeyboardKey) -> Paragraph<'static> {
    let cap = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_HIGHLIGHT())
        .add_modifier(Modifier::BOLD);
    let paragraph = match key {
        KeyboardKey::Letter(ch) => match state.score_for_keyboard_letter(ch) {
            Some(score) => Paragraph::new(score_tile(ch.to_ascii_uppercase(), score)),
            None => Paragraph::new(ch.to_ascii_uppercase().to_string()).style(cap),
        },
        KeyboardKey::Backspace => Paragraph::new("BKSP").style(cap),
        KeyboardKey::Enter => Paragraph::new("ENTER").style(cap),
    };
    paragraph.alignment(Alignment::Center)
}

fn contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

fn board_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(MAX_GUESSES * 2 - 1);
    for row in 0..MAX_GUESSES {
        if row > 0 {
            lines.push(Line::from(""));
        }

        let mut spans = Vec::with_capacity(WORD_LEN * 2 - 1);
        let guess = state.guesses.get(row).map(String::as_str);
        let current =
            (guess.is_none() && row == state.guesses.len()).then_some(&state.current_guess);
        for col in 0..WORD_LEN {
            if col > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(cell_span(state, guess, current, col));
        }
        lines.push(Line::from(spans));
    }
    lines
}

fn cell_span(
    state: &State,
    guess: Option<&str>,
    current: Option<&String>,
    col: usize,
) -> Span<'static> {
    let letter_at = |word: &str| {
        word.as_bytes()
            .get(col)
            .copied()
            .map(char::from)
            .unwrap_or(' ')
            .to_ascii_uppercase()
    };
    match (guess, current) {
        (Some(guess), _) => score_tile(letter_at(guess), state.scores_for_guess(guess)[col]),
        (None, Some(current)) => open_tile(letter_at(current), theme::TEXT_BRIGHT()),
        (None, None) => open_tile(' ', theme::TEXT_DIM()),
    }
}

/// A tile on a row not yet guessed: the one being typed, or one still ahead.
fn open_tile(ch: char, fg: Color) -> Span<'static> {
    Span::styled(
        format!(" {ch} "),
        Style::default()
            .fg(fg)
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD),
    )
}

/// One scored letter, three cells wide, on the board or the keyboard. The
/// colors are the theme's accents with the letter punched through, so the
/// board follows the palette (terminal palette included), but each score
/// also has its own shape: several themes give `SUCCESS`, `AMBER`, and
/// `TEXT_FAINT` one value, and a score told apart by color alone vanishes
/// there. Correct is a solid tile, present a solid tile in brackets, absent
/// the bare letter with no tile at all.
fn score_tile(ch: char, score: LetterScore) -> Span<'static> {
    let (text, style) = match score {
        LetterScore::Correct => (format!(" {ch} "), theme::punch_through(theme::SUCCESS())),
        LetterScore::Present => (format!("[{ch}]"), theme::punch_through(theme::AMBER())),
        LetterScore::Absent => (format!(" {ch} "), Style::default().fg(theme::TEXT_DIM())),
    };
    Span::styled(text, style.add_modifier(Modifier::BOLD))
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
