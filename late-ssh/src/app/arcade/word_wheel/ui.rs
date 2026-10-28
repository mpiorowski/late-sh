//! The Word Wheel screen: the wheel (the hub with six letters round it on
//! spokes), the word being typed, the rank meter, and the words found in
//! columns. Mouse hit-testing reads the same layout the frame drew.

use late_core::models::chips::Difficulty;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::state::{Rank, State, word_score};
use crate::app::{
    arcade::ui::{
        GameBottomBar, SHOW_GAME_BOTTOM_BAR, centered_rect, draw_game_frame, game_content_area,
        keys_line, share_hints, status_line, tip_line,
    },
    common::theme,
};

const FULL_CONTROL_HINTS_WIDTH: u16 = 100;
const WHEEL_WIDTH: u16 = 19;
const WHEEL_HEIGHT: u16 = 5;
/// The hub's cell in the wheel, as (column, row).
const HUB_CELL: (u16, u16) = (9, 2);
/// The rim's cells, clockwise from twelve o'clock, in the order of
/// `State::rim`.
const RIM_CELLS: [(u16, u16); 6] = [(9, 0), (15, 1), (15, 3), (9, 4), (3, 3), (3, 1)];
const SPOKES: [(u16, u16, &str); 6] = [
    (9, 1, "│"),
    (12, 1, "╱"),
    (12, 3, "╲"),
    (9, 3, "│"),
    (6, 3, "╱"),
    (6, 1, "╲"),
];
const METER_SLOTS: u32 = 10;
/// A column of found words: the longest answer and a gap.
const WORD_COLUMN_WIDTH: u16 = 17;

/// A letter's place on the wheel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelCell {
    Hub,
    /// An index into `State::rim`.
    Rim(usize),
}

/// Where each part goes inside the board area.
#[derive(Clone, Copy)]
pub(crate) struct WheelLayout {
    pub wheel: Rect,
    pub typed: Rect,
    pub meter: Rect,
    pub words: Rect,
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let next = match state.next_rank() {
        Some((rank, points)) => format!("{points} to {}", rank.label()),
        None => "every word".to_string(),
    };
    let control_hints = if area.width >= FULL_CONTROL_HINTS_WIDTH {
        vec![
            ("a-z", "type"),
            ("Enter", "submit"),
            ("Backspace", "delete"),
            ("Tab", "shuffle"),
            ("!", "rules"),
            ("?", "help"),
            ("`", "dashboard"),
            ("Esc", "exit"),
        ]
    } else {
        vec![
            ("a-z", "type"),
            ("Enter", "submit"),
            ("Tab", "shuffle"),
            ("!", "rules"),
            ("Esc", "exit"),
        ]
    };
    let bottom = GameBottomBar {
        status: status_line(vec![
            (
                "rank",
                state.rank().label().to_string(),
                theme::AMBER_GLOW(),
            ),
            ("score", state.score().to_string(), theme::SUCCESS()),
            (
                "words",
                state.found().len().to_string(),
                theme::TEXT_BRIGHT(),
            ),
            ("next", next, theme::TEXT_MUTED()),
        ]),
        keys: keys_line(
            control_hints
                .into_iter()
                .chain(share_hints(super::share::is_ready(state)))
                .collect(),
        ),
        tip: Some(tip_line(state.message().to_string())),
    };

    let board_area = draw_game_frame(frame, area, "Word Wheel", bottom, show_bottom_bar);
    let layout = layout(board_area);
    if state.is_loaded() {
        frame.render_widget(Paragraph::new(wheel_lines(state)), layout.wheel);
        frame.render_widget(
            Paragraph::new(typed_line(state)).alignment(Alignment::Center),
            layout.typed,
        );
        frame.render_widget(
            Paragraph::new(meter_lines(state)).alignment(Alignment::Center),
            layout.meter,
        );
        draw_found(frame, layout.words, state);
    }
    if state.show_rules() {
        draw_rules_modal(frame, board_area);
    }
}

/// The wheel, the typed word, the meter and the words found, top to
/// bottom; the wheel centred.
pub(crate) fn layout(board_area: Rect) -> WheelLayout {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(WHEEL_HEIGHT),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(board_area);
    let wheel_row = rows[1];
    let width = WHEEL_WIDTH.min(wheel_row.width);
    WheelLayout {
        wheel: Rect::new(
            wheel_row.x + (wheel_row.width - width) / 2,
            wheel_row.y,
            width,
            wheel_row.height,
        ),
        typed: rows[3],
        meter: rows[5],
        words: rows[7],
    }
}

/// The wheel letter under a zero-based terminal cell, for the Arcade page
/// `area`. A letter takes a click on the cell either side of it too.
pub fn hit_test(area: Rect, x: u16, y: u16) -> Option<WheelCell> {
    let board_area = game_content_area(area, true, SHOW_GAME_BOTTOM_BAR);
    let wheel = layout(board_area).wheel;
    let hits = |(column, row): (u16, u16)| {
        let center = wheel.x + column;
        y == wheel.y + row && x + 1 >= center && x <= center + 1 && x < wheel.right()
    };
    if hits(HUB_CELL) {
        return Some(WheelCell::Hub);
    }
    RIM_CELLS
        .iter()
        .position(|cell| hits(*cell))
        .map(WheelCell::Rim)
}

fn wheel_lines(state: &State) -> Vec<Line<'static>> {
    let mut grid: Vec<Vec<Span<'static>>> = (0..WHEEL_HEIGHT)
        .map(|_| (0..WHEEL_WIDTH).map(|_| Span::raw(" ")).collect())
        .collect();
    let spoke = Style::default().fg(theme::TEXT_DIM());
    for (column, row, glyph) in SPOKES {
        grid[row as usize][column as usize] = Span::styled(glyph, spoke);
    }
    let rim = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    for ((column, row), letter) in RIM_CELLS.into_iter().zip(state.rim()) {
        grid[row as usize][column as usize] =
            Span::styled(letter.to_ascii_uppercase().to_string(), rim);
    }
    if let Some(hub) = state.hub() {
        let hub_style = Style::default()
            .fg(Color::Black)
            .bg(theme::AMBER())
            .add_modifier(Modifier::BOLD);
        let (column, row) = HUB_CELL;
        let row = &mut grid[row as usize];
        row[column as usize - 1] = Span::styled(" ", hub_style);
        row[column as usize] = Span::styled(hub.to_ascii_uppercase().to_string(), hub_style);
        row[column as usize + 1] = Span::styled(" ", hub_style);
    }
    grid.into_iter().map(Line::from).collect()
}

/// The word being typed, the hub letter in amber, then the cursor.
fn typed_line(state: &State) -> Line<'static> {
    if state.typed().is_empty() {
        return Line::from(Span::styled(
            "type a word",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    let hub = state.hub();
    let mut spans: Vec<Span<'static>> = state
        .typed()
        .chars()
        .map(|letter| {
            let color = if Some(letter) == hub {
                theme::AMBER()
            } else {
                theme::TEXT_BRIGHT()
            };
            Span::styled(
                letter.to_ascii_uppercase().to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    spans.push(Span::styled("▏", Style::default().fg(theme::AMBER_GLOW())));
    Line::from(spans)
}

/// The rank with the score as ten slots, then what the next rank needs.
fn meter_lines(state: &State) -> Vec<Line<'static>> {
    let max = state.max_score().max(1);
    let filled = state.score() * METER_SLOTS / max;
    let slots: String = (0..METER_SLOTS)
        .map(|slot| if slot < filled { '▰' } else { '▱' })
        .collect();
    let next = match state.next_rank() {
        Some((rank, points)) => format!("{points} more for {}", rank.label()),
        None => "Every word found.".to_string(),
    };
    vec![
        Line::from(vec![
            Span::styled(
                format!("{}  ", state.rank().label()),
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(slots, Style::default().fg(theme::AMBER())),
            Span::styled(
                format!("  {} points", state.score()),
                Style::default().fg(theme::TEXT()),
            ),
        ]),
        Line::from(Span::styled(next, Style::default().fg(theme::TEXT_DIM()))),
    ]
}

/// The words found in columns, alphabetical down each column, full wheels
/// in amber. When they outgrow the area the last line says how many more.
fn draw_found(frame: &mut Frame, area: Rect, state: &State) {
    if area.height < 2 || area.width == 0 {
        return;
    }
    let found = state.found();
    let header = match state.full_wheels_found() {
        0 => format!("{} found", plural(found.len(), "word")),
        full => format!(
            "{} found · {}",
            plural(found.len(), "word"),
            plural(full, "full wheel")
        ),
    };
    let columns = usize::from((area.width / WORD_COLUMN_WIDTH).max(1));
    let rows = usize::from(area.height - 1);
    let capacity = columns * rows;
    let (shown, more) = if found.len() > capacity {
        (capacity - 1, found.len() - (capacity - 1))
    } else {
        (found.len(), 0)
    };
    let per_column = shown.div_ceil(columns).max(1);
    let mut lines = vec![Line::from(Span::styled(
        header,
        Style::default().fg(theme::TEXT_MUTED()),
    ))];
    for row in 0..per_column.min(rows) {
        let mut spans = Vec::new();
        for column in 0..columns {
            let index = column * per_column + row;
            if index >= shown {
                break;
            }
            let word = &found[index];
            let style = if state.is_full_wheel(word) {
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            spans.push(Span::styled(
                format!("{:<width$}", word, width = usize::from(WORD_COLUMN_WIDTH)),
                style,
            ));
        }
        lines.push(Line::from(spans));
    }
    if more > 0 {
        lines.push(Line::from(Span::styled(
            format!("+{more} more"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    let width = (columns as u16 * WORD_COLUMN_WIDTH).min(area.width);
    let list = Rect::new(
        area.x + (area.width - width) / 2,
        area.y,
        width,
        area.height,
    );
    frame.render_widget(Paragraph::new(lines), list);
}

fn plural(count: usize, noun: &str) -> String {
    match count {
        1 => format!("1 {noun}"),
        count => format!("{count} {noun}s"),
    }
}

fn draw_rules_modal(frame: &mut Frame, area: Rect) {
    let modal = centered_rect(area, 60.min(area.width), 19.min(area.height));
    let text = |line: String| Line::from(line);
    let rules = Paragraph::new(vec![
        Line::from(Span::styled(
            "Word Wheel Rules",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        text("Spell words from the seven letters on the wheel.".to_string()),
        text(
            "Every word uses the hub in the middle, has four letters or more, and may use a letter more than once."
                .to_string(),
        ),
        Line::from(""),
        text(format!(
            "A four-letter word scores {}, and each letter past three adds one. A full wheel uses all seven letters and scores double.",
            word_score("abcdefg", "abcd")
        )),
        Line::from(""),
        text(format!(
            "Bronze, Silver and Gold are {}%, {}% and {}% of the wheel's top score, and pay {}, {} and {} chips.",
            Rank::Bronze.percent(),
            Rank::Silver.percent(),
            Rank::Gold.percent(),
            Difficulty::Easy.chips(),
            Difficulty::Medium.chips(),
            Difficulty::Hard.chips()
        )),
        text("One wheel for everyone, new each day.".to_string()),
        Line::from(""),
        Line::from(Span::styled(
            "Tab shuffles the rim · ! / Esc closes",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .style(Style::default().fg(theme::TEXT_DIM()).bg(Color::Reset))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::AMBER_GLOW())),
    );
    frame.render_widget(Clear, modal);
    frame.render_widget(rules, modal);
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
