use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph},
};

use super::state::{Field, Mode, RoomInfoModalState};
use crate::app::common::{primitives::row_with_hint, theme};

/// The gutter the focus bar lives in, left of every field's text.
const GUTTER: u16 = 2;

/// Draw the room-info form centred over `area`. Only the modal itself is
/// framed: the fields are borderless, each a label row with its character count
/// flushed right over the text, and focus is carried by an accent bar in the
/// gutter plus the label going bright. Two fields, one thin rule between them.
pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &RoomInfoModalState) {
    let popup = centered_rect(area, 66, 17);
    frame.render_widget(Clear, popup);

    let creating = matches!(state.mode(), Some(Mode::Create { .. }));
    let heading = if creating {
        format!(" Create {} ", state.room_label())
    } else {
        format!(" {} ", state.room_label())
    };
    let block = Block::default()
        .title(Span::styled(
            heading,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        // Room to breathe: a column either side and a blank row above and below,
        // so nothing is pressed against the frame.
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // owner
        Constraint::Length(1), // spacer
        Constraint::Length(2), // topic: label + one line
        Constraint::Length(1), // spacer
        Constraint::Length(1), // rule between the two fields
        Constraint::Length(1), // spacer
        Constraint::Length(4), // rules: label + three lines
        Constraint::Min(1),    // spacer
        Constraint::Length(1), // keys
    ])
    .split(inner);

    // Who holds the room sits on its own row: ownership can move on its own
    // when a creator leaves, so this is where that becomes visible.
    frame.render_widget(
        line_on_canvas(vec![
            Span::styled("Owner  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                state.owner_label().to_string(),
                Style::default().fg(theme::AMBER()),
            ),
        ]),
        indent(rows[0]),
    );

    draw_field(frame, rows[2], state, Field::Topic, "What it is about");

    frame.render_widget(
        line_on_canvas(vec![Span::styled(
            "\u{2500}".repeat(rows[4].width as usize),
            Style::default().fg(theme::BORDER_DIM()),
        )]),
        rows[4],
    );

    draw_field(frame, rows[6], state, Field::Rules, "Rules");

    let submit = if creating { "create" } else { "save" };
    frame.render_widget(
        line_on_canvas(vec![
            Span::styled("Enter", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                format!(" {submit}   "),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled("Tab", Style::default().fg(theme::AMBER())),
            Span::styled(" field   ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::ERROR())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]),
        indent(rows[8]),
    );
}

/// One field: its label with the character count flushed right, then the text
/// itself. The focused field is marked by an accent bar in the gutter and a
/// bright label, so nothing shifts when focus moves.
fn draw_field(
    frame: &mut Frame,
    area: Rect,
    state: &RoomInfoModalState,
    field: Field,
    label: &str,
) {
    state.record_field_rect(field, area);
    let focused = state.focus() == field;
    let [label_row, text_rows] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let [gutter, text] =
        Layout::horizontal([Constraint::Length(GUTTER), Constraint::Min(1)]).areas(text_rows);

    let label_style = if focused {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    frame.render_widget(
        Paragraph::new(row_with_hint(
            vec![Span::styled(label.to_string(), label_style)],
            vec![Span::styled(
                format!("{}/{}", state.used(field), field.max_len()),
                Style::default().fg(theme::TEXT_FAINT()),
            )],
            label_row.width.saturating_sub(GUTTER) as usize,
        ))
        .style(Style::default().bg(theme::BG_CANVAS())),
        indent(label_row),
    );

    let bar_style = if focused {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER_DIM())
    };
    let bars: Vec<Line> = (0..gutter.height)
        .map(|_| Line::from(Span::styled("\u{258f}", bar_style)))
        .collect();
    frame.render_widget(
        Paragraph::new(bars).style(Style::default().bg(theme::BG_CANVAS())),
        gutter,
    );
    frame.render_widget(state.field(field), text);
}

/// Align a full-width row with the field text, past the focus gutter.
fn indent(area: Rect) -> Rect {
    Rect {
        x: area.x + GUTTER,
        width: area.width.saturating_sub(GUTTER),
        ..area
    }
}

fn line_on_canvas(spans: Vec<Span<'static>>) -> Paragraph<'static> {
    Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_CANVAS()))
}

/// A centred rectangle of the given size, clamped to `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
