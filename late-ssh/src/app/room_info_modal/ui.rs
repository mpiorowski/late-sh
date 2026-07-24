use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::{ABOUT_MAX, Field, Mode, RULES_MAX, RoomInfoModalState, TITLE_MAX};

const ACCENT: Color = Color::Cyan;
const MUTED: Color = Color::DarkGray;

/// Draw the room-info form centred over `area`.
pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &RoomInfoModalState) {
    let width = 62.min(area.width.saturating_sub(4)).max(24);
    let height = 15.min(area.height.saturating_sub(2)).max(10);
    let popup = centered_rect(area, width, height);
    frame.render_widget(Clear, popup);

    let creating = matches!(state.mode(), Some(Mode::Create { .. }));
    let heading = if creating {
        " Create a room "
    } else {
        " Edit room info "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            heading,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(2), // name
        Constraint::Length(2), // about
        Constraint::Length(2), // rules
        Constraint::Min(1),    // hint / status
    ])
    .split(inner);

    draw_field(
        frame,
        rows[0],
        state,
        Field::Title,
        "Name",
        "What do you want to name your room?",
        TITLE_MAX,
    );
    draw_field(
        frame,
        rows[1],
        state,
        Field::About,
        "About",
        "Tell people what your room is about",
        ABOUT_MAX,
    );
    draw_field(
        frame,
        rows[2],
        state,
        Field::Rules,
        "Rules",
        "The general rules (optional)",
        RULES_MAX,
    );

    let mut footer: Vec<Line> = Vec::new();
    if let Some(status) = state.status() {
        footer.push(Line::from(Span::styled(
            status.to_string(),
            Style::default().fg(Color::Yellow),
        )));
    }
    let submit = if creating {
        "Enter create"
    } else {
        "Enter save"
    };
    footer.push(Line::from(Span::styled(
        format!("Tab/↑↓ move · {submit} · Esc cancel"),
        Style::default().fg(MUTED),
    )));
    frame.render_widget(Paragraph::new(footer), rows[3]);
}

fn draw_field(
    frame: &mut Frame,
    area: Rect,
    state: &RoomInfoModalState,
    field: Field,
    label: &str,
    placeholder: &str,
    max: usize,
) {
    let focused = state.focus() == field;
    let ta = state.field(field);
    let text = ta.lines().first().cloned().unwrap_or_default();
    let used = text.chars().count();

    let split = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // Label row with a focus marker and a live character count.
    let marker = if focused { "› " } else { "  " };
    let label_style = if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED)
    };
    let label_line = Line::from(vec![
        Span::styled(format!("{marker}{label}"), label_style),
        Span::styled(format!("  {used}/{max}"), Style::default().fg(MUTED)),
    ]);
    frame.render_widget(Paragraph::new(label_line), split[0]);

    // Input row: the focused field renders the live editor (with its cursor);
    // the others show their text, or the placeholder when empty.
    if focused {
        frame.render_widget(ta, split[1]);
    } else if text.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                placeholder.to_string(),
                Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
            )),
            split[1],
        );
    } else {
        frame.render_widget(Paragraph::new(text), split[1]);
    }
}

/// A centred rectangle of the given size, clamped to `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}
