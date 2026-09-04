use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::theme;

/// What a span of an overlay is, never what colour it is. Overlays are
/// built during the session tick (`/members` arrives through
/// `ChatState::drain_events`), and `theme`'s palette lives in a thread
/// local that `App::render` sets afterwards, on whatever worker thread the
/// session woke on: a span styled at build time takes whichever session
/// last rendered there. `draw_overlay` resolves ink inside the draw, where
/// the reader's theme is the live one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayInk {
    /// Here now: bold, in the success colour.
    Strong,
    /// Ordinary body text, and what an unstyled line is made of.
    Body,
    /// Absent, or secondary to what sits beside it.
    Dim,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlaySpan {
    pub text: String,
    pub ink: OverlayInk,
}

impl OverlaySpan {
    pub fn new(text: impl Into<String>, ink: OverlayInk) -> Self {
        Self {
            text: text.into(),
            ink,
        }
    }
}

/// One overlay line: the spans left to right.
pub type OverlayLine = Vec<OverlaySpan>;

fn ink_style(ink: OverlayInk) -> Style {
    match ink {
        OverlayInk::Strong => Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD),
        OverlayInk::Body => Style::default().fg(theme::TEXT()),
        OverlayInk::Dim => Style::default().fg(theme::TEXT_DIM()),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overlay {
    pub title: String,
    pub lines: Vec<String>,
    pub styled_lines: Option<Vec<OverlayLine>>,
    pub scroll_offset: u16,
    pub close_on_any_key: bool,
}

impl Overlay {
    pub fn new(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            title: title.into(),
            lines,
            styled_lines: None,
            scroll_offset: 0,
            close_on_any_key: false,
        }
    }

    pub fn styled(title: impl Into<String>, lines: Vec<OverlayLine>) -> Self {
        Self {
            title: title.into(),
            lines: Vec::new(),
            styled_lines: Some(lines),
            scroll_offset: 0,
            close_on_any_key: false,
        }
    }

    pub fn dismissible(title: impl Into<String>, lines: Vec<String>) -> Self {
        Self {
            close_on_any_key: true,
            ..Self::new(title, lines)
        }
    }

    pub fn scroll(&mut self, delta: i16) {
        let next = self.scroll_offset as i32 + delta as i32;
        self.scroll_offset = next.clamp(0, u16::MAX as i32) as u16;
    }
}

fn wrapped_row_count(line: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    let cell_count = 1 + line.chars().count();
    cell_count.div_ceil(width) as u16
}

pub fn draw_overlay(frame: &mut Frame, anchor: Rect, overlay: &Overlay) {
    if anchor.width < 12 || anchor.height < 8 {
        return;
    }

    let width = anchor.width.saturating_sub(4).max(10);
    let inner_width = width.saturating_sub(2).max(1);
    let content_height = if let Some(lines) = &overlay.styled_lines {
        lines.len() as u16
    } else {
        overlay
            .lines
            .iter()
            .map(|line| wrapped_row_count(line, inner_width))
            .sum::<u16>()
    }
    .saturating_add(2);
    let height = content_height.min(anchor.height).max(8);
    let area = Rect::new(
        anchor.x + 2,
        anchor.y + anchor.height - height,
        width,
        height,
    );

    let hint = if overlay.close_on_any_key {
        "↑/↓ j/k scroll · other key close"
    } else {
        "↑/↓ j/k scroll · Esc/q close"
    };
    let block = Block::default()
        .title(format!(" {} ({hint}) ", overlay.title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

    let lines: Vec<Line> = match &overlay.styled_lines {
        Some(styled) => styled
            .iter()
            .map(|line| {
                Line::from(
                    line.iter()
                        .map(|span| Span::styled(span.text.clone(), ink_style(span.ink)))
                        .collect::<Vec<_>>(),
                )
            })
            .collect(),
        None => overlay
            .lines
            .iter()
            .map(|line| {
                Line::from(Span::styled(
                    format!(" {line}"),
                    ink_style(OverlayInk::Body),
                ))
            })
            .collect(),
    };

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((overlay.scroll_offset, 0)),
        area,
    );
}
