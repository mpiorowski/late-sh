//! The picker popup: every piece on its own row, the cursor on one, the
//! keys.

use late_core::models::user::AsciiPiece;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::{PiecePickerState, Target};
use crate::app::common::{primitives::hint_line, theme};

const POPUP_W: u16 = 36;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &PiecePickerState) {
    if !state.is_open() {
        return;
    }
    state.mouse.begin((frame.area().width, frame.area().height));
    // The frame, the list, a blank line, the keys.
    let height = (AsciiPiece::ALL.len() as u16 + 4).min(area.height);
    let popup = centered(area, POPUP_W.min(area.width), height);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Pick a piece ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let close = Rect::new(
        popup.right().saturating_sub(5).max(popup.x),
        popup.y,
        popup.width.min(3),
        popup.height.min(1),
    );
    frame.render_widget(
        Paragraph::new("[x]").style(Style::default().fg(theme::AMBER_GLOW())),
        close,
    );
    state.mouse.hit(close, Target::Close);

    let [list_area, _, foot_area] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let height = list_area.height as usize;
    let width = list_area.width as usize;
    let scroll = state
        .mouse
        .pane(list_area, (), AsciiPiece::ALL.len(), state.cursor());
    let end = (scroll + height).min(AsciiPiece::ALL.len());
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, piece) in AsciiPiece::ALL[scroll..end].iter().enumerate() {
        let at = scroll + idx;
        state.mouse.hit(
            Rect::new(list_area.x, list_area.y + idx as u16, list_area.width, 1),
            Target::Row(at),
        );
        let selected = at == state.cursor();
        let (marker, style) = match selected {
            true => (
                "›",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .bg(theme::BG_HIGHLIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            false => ("·", Style::default().fg(theme::TEXT())),
        };
        let text = format!(" {marker} {}", piece.label());
        let padded = format!("{text:<width$}");
        lines.push(Line::from(Span::styled(padded, style)));
    }
    frame.render_widget(Paragraph::new(lines), list_area);
    frame.render_widget(
        Paragraph::new(hint_line(&[("Enter", "pick"), ("j k", "move"), ("Esc", "close")])),
        foot_area,
    );
    state.mouse.finish();
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
