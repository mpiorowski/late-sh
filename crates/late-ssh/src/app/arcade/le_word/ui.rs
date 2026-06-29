use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::{LetterScore, MAX_GUESSES, State, WORD_LEN};
use crate::app::arcade::ui::{
    GameBottomBar, centered_rect, draw_game_frame, keys_line, status_line, tip_line,
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
const WORDLE_TEXT: Color = Color::Rgb(255, 255, 255);
const WORDLE_TEXT_DIM: Color = Color::Rgb(211, 214, 218);
const WORDLE_BG: Color = Color::Rgb(18, 18, 18);
const WORDLE_TILE_EMPTY_BG: Color = Color::Rgb(67, 67, 69);
const WORDLE_KEY_BG: Color = Color::Rgb(130, 131, 133);
const WORDLE_CORRECT_BG: Color = Color::Rgb(82, 141, 77);
const WORDLE_PRESENT_BG: Color = Color::Rgb(181, 159, 58);
const WORDLE_ABSENT_BG: Color = Color::Rgb(58, 58, 60);

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
            ("reward", "100".to_string(), theme::TEXT_BRIGHT()),
        ]),
        keys: keys_line(vec![
            ("a-z", "type"),
            ("Backspace", "delete"),
            ("Enter", "guess"),
            ("?", "help"),
            ("!", "rules"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]),
        tip: Some(tip_line(state.message.clone())),
    };

    let board_area = draw_game_frame(frame, area, "Le Word", bottom, show_bottom_bar);
    let layout = le_word_layout(board_area);
    frame.render_widget(
        Paragraph::new(board_lines(state))
            .alignment(Alignment::Center)
            .style(Style::default().fg(WORDLE_TEXT).bg(WORDLE_BG)),
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
            "Come back tomorrow",
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
        draw_rules_modal(frame, board_area);
    }
}

fn draw_rules_modal(frame: &mut Frame, area: Rect) {
    let modal = centered_rect(area, 58.min(area.width), 16.min(area.height));
    let rules = Paragraph::new(vec![
        Line::from(Span::styled(
            "Le Word Rules",
            Style::default()
                .fg(WORDLE_TEXT)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Guess the hidden five-letter word in six tries."),
        Line::from("Each guess must be a valid word."),
        Line::from(""),
        Line::from(vec![
            Span::styled("GREEN", score_style(LetterScore::Correct)),
            Span::raw("  correct letter, correct spot"),
        ]),
        Line::from(vec![
            Span::styled("YELLOW", score_style(LetterScore::Present)),
            Span::raw(" correct letter, wrong spot"),
        ]),
        Line::from(vec![
            Span::styled("GRAY", score_style(LetterScore::Absent)),
            Span::raw("   letter not in the word"),
        ]),
        Line::from(""),
        Line::from("A new daily answer appears once per day."),
        Line::from("Solving the daily earns 100 chips."),
        Line::from(""),
        Line::from(Span::styled(
            "! / q / Esc closes",
            Style::default().fg(WORDLE_TEXT_DIM).bg(WORDLE_BG),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .style(Style::default().fg(WORDLE_TEXT_DIM).bg(WORDLE_BG))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::AMBER_GLOW())),
    );
    frame.render_widget(Clear, modal);
    frame.render_widget(rules, modal);
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
    frame.render_widget(Block::default().style(Style::default().bg(WORDLE_BG)), area);
    for key_rect in keyboard_key_rects(area) {
        let label = key_label(key_rect.key);
        let key = Paragraph::new(label)
            .alignment(Alignment::Center)
            .style(key_style(state, key_rect.key));
        frame.render_widget(key, key_rect.rect);
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

fn key_label(key: KeyboardKey) -> String {
    match key {
        KeyboardKey::Letter(ch) => ch.to_ascii_uppercase().to_string(),
        KeyboardKey::Backspace => "BKSP".to_string(),
        KeyboardKey::Enter => "ENTER".to_string(),
    }
}

fn key_style(state: &State, key: KeyboardKey) -> Style {
    let Some(score) = keyboard_key_score(state, key) else {
        return Style::default()
            .fg(WORDLE_TEXT)
            .bg(WORDLE_KEY_BG)
            .add_modifier(Modifier::BOLD);
    };
    score_style(score).add_modifier(Modifier::BOLD)
}

fn keyboard_key_score(state: &State, key: KeyboardKey) -> Option<LetterScore> {
    match key {
        KeyboardKey::Letter(ch) => state.score_for_keyboard_letter(ch),
        KeyboardKey::Backspace | KeyboardKey::Enter => None,
    }
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
    let (ch, style) = if let Some(guess) = guess {
        let ch = guess
            .as_bytes()
            .get(col)
            .copied()
            .map(char::from)
            .unwrap_or(' ')
            .to_ascii_uppercase();
        let scores = state.scores_for_guess(guess);
        (ch, score_style(scores[col]))
    } else if let Some(current) = current {
        let ch = current
            .as_bytes()
            .get(col)
            .copied()
            .map(char::from)
            .unwrap_or(' ')
            .to_ascii_uppercase();
        (
            ch,
            Style::default().fg(WORDLE_TEXT).bg(WORDLE_TILE_EMPTY_BG),
        )
    } else {
        (
            ' ',
            Style::default()
                .fg(WORDLE_TEXT_DIM)
                .bg(WORDLE_TILE_EMPTY_BG),
        )
    };

    Span::styled(format!(" {ch} "), style.add_modifier(Modifier::BOLD))
}

fn score_style(score: LetterScore) -> Style {
    match score {
        LetterScore::Correct => Style::default().fg(WORDLE_TEXT).bg(WORDLE_CORRECT_BG),
        LetterScore::Present => Style::default().fg(WORDLE_TEXT).bg(WORDLE_PRESENT_BG),
        LetterScore::Absent => Style::default().fg(WORDLE_TEXT).bg(WORDLE_ABSENT_BG),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_panel_prefers_space_below_board() {
        let board_area = Rect::new(0, 0, 80, 40);
        let board_rect = Rect::new(28, 10, 24, 13);
        let keyboard_rect = Rect::new(20, 25, 39, 5);

        let area = result_panel_area(board_area, board_rect, Some(keyboard_rect));

        assert!(area.y > keyboard_rect.y + keyboard_rect.height);
        assert_eq!(area.width, 28);
        assert_eq!(area.height, 4);
    }

    #[test]
    fn layout_places_keyboard_two_rows_below_board() {
        let layout = le_word_layout(Rect::new(0, 0, 80, 40));
        let keyboard = layout.keyboard.expect("keyboard fits");

        assert_eq!(
            keyboard.y,
            layout.board.y + layout.board.height + BOARD_KEYBOARD_GAP
        );
        assert_eq!(keyboard.width, KEYBOARD_WIDTH);
        assert_eq!(keyboard.height, KEYBOARD_HEIGHT);
    }

    #[test]
    fn keyboard_hit_test_maps_clicks_to_keys() {
        let area = Rect::new(0, 0, 80, 40);

        assert_eq!(
            keyboard_hit_test(area, 20, 25),
            Some(KeyboardKey::Letter('q'))
        );
        assert_eq!(
            keyboard_hit_test(area, 22, 27),
            Some(KeyboardKey::Letter('a'))
        );
        assert_eq!(keyboard_hit_test(area, 20, 29), Some(KeyboardKey::Enter));
        assert_eq!(
            keyboard_hit_test(area, 54, 29),
            Some(KeyboardKey::Backspace)
        );
        assert_eq!(keyboard_hit_test(area, 0, 0), None);
    }
}
