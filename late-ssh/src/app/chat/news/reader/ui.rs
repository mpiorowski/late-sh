//! The News reader's frame: a centered box over whatever screen is up,
//! shaped like the paper's, with the story laid out in one text column.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::state::{READER_MAX_TEXT_WIDTH, ReaderInk, ReaderModal, ReaderRow, ReaderViewport};
use crate::app::common::theme;

const READER_WIDTH_PERMILLE: u16 = 900;
const READER_HEIGHT_PERMILLE: u16 = 900;
const READER_MAX_WIDTH: u16 = 120;

pub(crate) fn draw(frame: &mut Frame, area: Rect, modal: &ReaderModal) {
    let width = permille(area.width, READER_WIDTH_PERMILLE).clamp(24, READER_MAX_WIDTH);
    let height = permille(area.height, READER_HEIGHT_PERMILLE).max(5);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Reading ")
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

    let close = if popup.width >= 5 && popup.height > 0 {
        let close = Rect::new(popup.right() - 4, popup.y, 3, 1);
        frame.render_widget(
            Paragraph::new("[x]").style(Style::default().fg(theme::AMBER_GLOW())),
            close,
        );
        close
    } else {
        Rect::default()
    };

    if inner.width < 24 || inner.height < 5 {
        modal.set_viewport(ReaderViewport {
            popup,
            close,
            ..ReaderViewport::default()
        });
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);
    let column = layout[1].inner(Margin {
        horizontal: 2,
        vertical: 0,
    });
    let text_width = column.width.min(READER_MAX_TEXT_WIDTH as u16);
    let body_area = Rect::new(
        column.x + (column.width - text_width) / 2,
        column.y,
        text_width,
        column.height,
    );

    let rows = modal.lay_out(usize::from(text_width));
    let content_height = u16::try_from(rows.len()).unwrap_or(u16::MAX);
    // The scrollbar sits in the right margin, clear of the text column.
    let track = if content_height > body_area.height {
        Rect::new(inner.right() - 1, body_area.y, 1, body_area.height)
    } else {
        Rect::default()
    };
    modal.set_viewport(ReaderViewport {
        popup,
        body: body_area,
        close,
        track,
        content_height,
    });
    let lines: Vec<Line<'static>> = rows.iter().map(row_line).collect();
    frame.render_widget(
        Paragraph::new(lines).scroll((modal.scroll_offset(), 0)),
        body_area,
    );
    if !track.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![Line::from("│"); track.height as usize])
                .style(Style::default().fg(theme::BORDER_DIM())),
            track,
        );
        let thumb = modal.thumb();
        frame.render_widget(
            Paragraph::new(vec![Line::from("█"); thumb.height as usize])
                .style(Style::default().fg(theme::AMBER_DIM())),
            thumb,
        );
    }

    frame.render_widget(
        Paragraph::new(footer_line(modal, usize::from(layout[2].width))).centered(),
        layout[2],
    );
}

/// `key hint` pairs in the order they drop off when the width runs short:
/// the last one goes first, `Esc` always stays.
pub(crate) fn footer_hints(modal: &ReaderModal) -> Vec<(&'static str, &'static str)> {
    let mut hints = vec![("j/k", "scroll"), ("Space", "page")];
    if modal.has_links() {
        hints.push(("1-9", "copy link"));
    }
    hints.extend([("c", "copy"), ("r", "reply")]);
    hints
}

fn footer_line(modal: &ReaderModal, width: usize) -> Line<'static> {
    let key = Style::default().fg(theme::AMBER_DIM());
    let hint = Style::default().fg(theme::TEXT_DIM());
    let esc_width = "Esc".len();
    let mut hints = footer_hints(modal);
    loop {
        let used: usize = hints
            .iter()
            .map(|(k, h)| UnicodeWidthStr::width(*k) + 1 + UnicodeWidthStr::width(*h) + 2)
            .sum();
        if used + esc_width <= width || hints.is_empty() {
            break;
        }
        hints.pop();
    }
    let mut spans = Vec::new();
    for (k, h) in hints {
        spans.push(Span::styled(k, key));
        spans.push(Span::styled(format!(" {h}  "), hint));
    }
    spans.push(Span::styled("Esc", key));
    Line::from(spans)
}

/// The palette, read here inside the draw and nowhere earlier.
fn ink_style(ink: ReaderInk) -> Style {
    match ink {
        ReaderInk::Title => Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
        ReaderInk::Meta => Style::default().fg(theme::TEXT_DIM()),
        ReaderInk::Heading => Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
        ReaderInk::Body => Style::default().fg(theme::TEXT()),
        ReaderInk::Quote => Style::default()
            .fg(theme::TEXT_DIM())
            .add_modifier(Modifier::ITALIC),
        ReaderInk::Gutter | ReaderInk::Rule => Style::default().fg(theme::BORDER_DIM()),
        ReaderInk::Code => Style::default().fg(theme::TEXT_DIM()),
        ReaderInk::LinkMark => Style::default().fg(theme::AMBER_DIM()),
        ReaderInk::LinkUrl => Style::default().fg(theme::TEXT_FAINT()),
        ReaderInk::Art => Style::default().fg(theme::AMBER_DIM()),
        ReaderInk::Status => Style::default().fg(theme::TEXT_DIM()),
    }
}

fn row_line(row: &ReaderRow) -> Line<'static> {
    Line::from(
        row.iter()
            .map(|segment| Span::styled(segment.text.clone(), ink_style(segment.ink)))
            .collect::<Vec<_>>(),
    )
}

/// `value * permille / 1000` without the `u16` overflow a wide terminal
/// would hit.
fn permille(value: u16, permille: u16) -> u16 {
    (u32::from(value) * u32::from(permille) / 1000) as u16
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
