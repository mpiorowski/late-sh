use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::theme;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub title: String,
    pub lines: Vec<String>,
    pub scroll_offset: u16,
}

impl Overlay {
    pub fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
            scroll_offset: 0,
        }
    }

    pub fn scroll(&mut self, delta: i16) {
        let next = self.scroll_offset as i32 + delta as i32;
        self.scroll_offset = next.clamp(0, u16::MAX as i32) as u16;
    }
}

pub fn draw_overlay(frame: &mut Frame, anchor: Rect, overlay: &Overlay) {
    if anchor.width < 12 || anchor.height < 8 {
        return;
    }

    let content_height = overlay.lines.len() as u16 + 2;
    let height = content_height.min(anchor.height).max(8);
    let width = anchor.width.saturating_sub(4).max(10);
    let area = Rect::new(
        anchor.x + 2,
        anchor.y + anchor.height - height,
        width,
        height,
    );

    let block = Block::default()
        .title(format!(" {} (j/k scroll · q/Esc close) ", overlay.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

    let lines: Vec<Line> = overlay
        .lines
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                format!(" {line}"),
                Style::default().fg(theme::TEXT()),
            ))
        })
        .collect();

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((overlay.scroll_offset, 0)),
        area,
    );
}
