use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use crate::app::{
    common::theme,
    rooms::tictactoe::state::{Mark, State, Winner},
};

const SIDE_WIDE: u16 = 28;
const SIDE_NARROW: u16 = 24;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    if area.height < 11 || area.width < 28 {
        draw_compact(frame, area, state);
        return;
    }

    let side_w = if area.width >= 60 {
        SIDE_WIDE
    } else {
        SIDE_NARROW
    };
    let columns = Layout::horizontal([Constraint::Min(20), Constraint::Length(side_w)]).split(area);
    draw_board(frame, columns[0], state);
    draw_side(frame, columns[1], state, usernames);
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let mut lines = Vec::new();
    for row in 0..3 {
        let mut spans = Vec::new();
        for col in 0..3 {
            let index = row * 3 + col;
            let cell = snapshot.board[index]
                .map(|mark| mark.label())
                .unwrap_or("·");
            let selected = index == state.cursor();
            spans.push(Span::styled(
                format!(" {cell} "),
                cell_style(selected, snapshot.board[index]),
            ));
        }
        lines.push(Line::from(spans).alignment(Alignment::Center));
    }
    lines.push(Line::from(status_text(state)).alignment(Alignment::Center));
    frame.render_widget(Paragraph::new(lines), area);
}

fn pick_cell_dims(area: Rect) -> (u16, u16) {
    let candidates: [(u16, u16); 3] = [(11, 5), (9, 5), (7, 3)];
    for (cw, ch) in candidates {
        if 3 * cw + 2 <= area.width && 3 * ch + 2 <= area.height {
            return (cw, ch);
        }
    }
    (5, 3)
}

fn draw_board(frame: &mut Frame, area: Rect, state: &State) {
    let snapshot = state.snapshot();
    let (cell_w, cell_h) = pick_cell_dims(area);

    let mut lines: Vec<Line> = Vec::with_capacity(3 * cell_h as usize + 2);
    for row in 0..3 {
        for cell_row in 0..cell_h {
            let mut spans: Vec<Span> = Vec::with_capacity(5);
            for col in 0..3 {
                let index = row * 3 + col;
                let mark = snapshot.board[index];
                let selected = index == state.cursor();
                let glyph_text = glyph_row(mark, cell_w, cell_h, cell_row);
                spans.push(Span::styled(glyph_text, cell_style(selected, mark)));
                if col < 2 {
                    spans.push(Span::styled("│", Style::default().fg(theme::BORDER_DIM())));
                }
            }
            lines.push(Line::from(spans));
        }
        if row < 2 {
            let dash = "─".repeat(cell_w as usize);
            let sep = format!("{dash}┼{dash}┼{dash}");
            lines.push(Line::from(Span::styled(
                sep,
                Style::default().fg(theme::BORDER_DIM()),
            )));
        }
    }

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

fn glyph_row(mark: Option<Mark>, cell_w: u16, cell_h: u16, cell_row: u16) -> String {
    let cw = cell_w as usize;
    let ch = cell_h as usize;
    let row = cell_row as usize;
    let Some(mark) = mark else {
        return " ".repeat(cw);
    };
    let (g_lines, g_w, g_h) = pick_glyph(mark, cell_w, cell_h);
    let pad_top = ch.saturating_sub(g_h) / 2;
    if row < pad_top || row >= pad_top + g_h {
        return " ".repeat(cw);
    }
    let glyph_line = g_lines[row - pad_top];
    let pad_left = cw.saturating_sub(g_w) / 2;
    let pad_right = cw.saturating_sub(g_w + pad_left);
    format!(
        "{}{}{}",
        " ".repeat(pad_left),
        glyph_line,
        " ".repeat(pad_right)
    )
}

fn pick_glyph(mark: Mark, cell_w: u16, cell_h: u16) -> (&'static [&'static str], usize, usize) {
    if cell_w >= 7 && cell_h >= 5 {
        match mark {
            Mark::X => (X_5X5, 5, 5),
            Mark::O => (O_5X5, 5, 5),
        }
    } else if cell_w >= 5 && cell_h >= 3 {
        match mark {
            Mark::X => (X_3X3, 3, 3),
            Mark::O => (O_3X3, 3, 3),
        }
    } else {
        match mark {
            Mark::X => (X_1X1, 1, 1),
            Mark::O => (O_1X1, 1, 1),
        }
    }
}

const X_5X5: &[&str] = &["█   █", " █ █ ", "  █  ", " █ █ ", "█   █"];

const O_5X5: &[&str] = &[" ███ ", "█   █", "█   █", "█   █", " ███ "];

const X_3X3: &[&str] = &["█ █", " █ ", "█ █"];

const O_3X3: &[&str] = &["███", "█ █", "███"];

const X_1X1: &[&str] = &["X"];
const O_1X1: &[&str] = &["O"];

fn draw_side(frame: &mut Frame, area: Rect, state: &State, usernames: &HashMap<Uuid, String>) {
    let snapshot = state.snapshot();
    let seated = state.seat_index().is_some();
    let mut lines = vec![
        Line::from(Span::styled(
            status_text(state),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        player_line("X", snapshot.seats[0], state, usernames),
        player_line("O", snapshot.seats[1], state, usernames),
        Line::raw(""),
    ];
    if seated {
        lines.extend([
            hint_line("1-9", "place direct"),
            hint_line("Space/Enter", "place cursor"),
            hint_line("w a s d", "move cursor"),
            hint_line("↑ ↓ ← →", "move cursor"),
            hint_line("l", "leave seat"),
            hint_line("n", "new round"),
        ]);
    } else {
        lines.extend([
            hint_line("s/Space/Enter", "sit"),
            Line::raw(""),
            hint_line("1-9", "place (after sitting)"),
            hint_line("w a s d", "move cursor"),
            hint_line("↑ ↓ ← →", "move cursor"),
        ]);
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn hint_line(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(key.to_string(), Style::default().fg(theme::AMBER_DIM())),
        Span::styled(format!("  {label}"), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn player_line(
    mark: &'static str,
    user_id: Option<Uuid>,
    state: &State,
    usernames: &HashMap<Uuid, String>,
) -> Line<'static> {
    let is_self = user_id.is_some_and(|uid| state.is_self(uid));
    let name = match user_id {
        Some(uid) => usernames
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| "player".to_string()),
        None => "open".to_string(),
    };
    let display = if is_self { format!("▶ {name}") } else { name };
    let name_style = if is_self {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if user_id.is_some() {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(vec![
        Span::styled(format!("{mark} "), mark_color(mark)),
        Span::styled(display, name_style),
    ])
}

fn mark_color(mark: &str) -> Style {
    match mark {
        "X" => Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
        _ => Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    }
}

fn status_text(state: &State) -> String {
    let snapshot = state.snapshot();
    match snapshot.winner {
        Some(Winner::Mark(mark)) => format!("{} wins", mark.label()),
        Some(Winner::Draw) => "Draw".to_string(),
        None => snapshot.status_message.clone(),
    }
}

fn cell_style(selected: bool, mark: Option<Mark>) -> Style {
    let base = match mark {
        Some(Mark::X) => Style::default().fg(theme::AMBER()),
        Some(Mark::O) => Style::default().fg(theme::TEXT_BRIGHT()),
        None => Style::default().fg(theme::TEXT_DIM()),
    };
    if selected {
        base.bg(theme::BG_SELECTION()).add_modifier(Modifier::BOLD)
    } else if mark.is_some() {
        base.add_modifier(Modifier::BOLD)
    } else {
        base
    }
}
