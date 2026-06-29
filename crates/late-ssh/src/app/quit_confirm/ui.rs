use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;

const MODAL_WIDTH: u16 = 60;
const MODAL_HEIGHT: u16 = 11;

/// Compute the rect the sayonara sixel scene should occupy, centred in
/// the modal's spacer area. Returns `None` when the modal is too small
/// to fit the scene cleanly — the non-image render path covers that
/// case automatically.
pub(crate) fn sayonara_scene_area(modal_area: Rect) -> Option<Rect> {
    use super::sayonara_sixel::{SAYONARA_DISPLAY_COLS, SAYONARA_DISPLAY_ROWS};
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, modal_area);
    let block = Block::default().borders(Borders::ALL);
    let inner = block.inner(popup);
    if inner.width < SAYONARA_DISPLAY_COLS || inner.height < SAYONARA_DISPLAY_ROWS + 2 {
        return None;
    }
    // Spacer sits between the prompt row (y = inner.y + 1) and the
    // footer row (y = inner.bottom() - 1). Centre the scene there with
    // a one-row gutter so it doesn't bleed into either.
    let spacer_top = inner.y + 2;
    let spacer_height = inner.height.saturating_sub(3);
    if spacer_height < SAYONARA_DISPLAY_ROWS {
        return None;
    }
    let x = inner.x + inner.width.saturating_sub(SAYONARA_DISPLAY_COLS) / 2;
    let y = spacer_top + spacer_height.saturating_sub(SAYONARA_DISPLAY_ROWS) / 2;
    Some(Rect::new(
        x,
        y,
        SAYONARA_DISPLAY_COLS,
        SAYONARA_DISPLAY_ROWS,
    ))
}

pub fn draw(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Quit? ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Length(1), // prompt
        Constraint::Min(1),    // spacer
        Constraint::Length(1), // footer
    ])
    .split(inner);

    let prompt = Line::from(Span::styled(
        "Clicked by mistake, right?",
        Style::default().fg(theme::TEXT_BRIGHT()),
    ));
    frame.render_widget(Paragraph::new(prompt).centered(), layout[1]);

    let footer_cols = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .split(layout[3]);

    let left = Line::from(vec![
        Span::styled("q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" bye, I'll be back", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(left), footer_cols[1]);

    let right = Line::from(vec![
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            " yeah, my bad, stay",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(right).right_aligned(), footer_cols[2]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
