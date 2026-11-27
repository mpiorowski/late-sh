//! Stream overlays. The OBS handoff modal shows the WHIP connection
//! details from `/golive obs`; the values are hand-copied into OBS
//! (Settings -> Stream -> Service: WHIP), so every value gets its own
//! full-width row, nothing is ever clipped, and only Esc closes it.

use crate::app::common::text_layout;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::theme;

pub fn draw_obs_overlay(
    frame: &mut Frame,
    area: Rect,
    whip_url: &str,
    stream_key: &str,
    watch_url: &str,
) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  OBS -> Settings -> Stream -> Service: WHIP",
            dim,
        )),
        Line::from(""),
        Line::from(Span::styled("  Server", dim)),
        Line::from(Span::styled(format!("  {whip_url}"), amber)),
        Line::from(""),
        Line::from(Span::styled("  Bearer Token", dim)),
        Line::from(Span::styled(format!("  {stream_key}"), amber)),
        Line::from(""),
        Line::from(Span::styled("  Watch link (share it)", dim)),
        Line::from(Span::styled(format!("  {watch_url}"), amber)),
        Line::from(""),
        Line::from(Span::styled(
            "  Start streaming in OBS; the room goes live when media flows.",
            dim,
        )),
        Line::from(""),
        Line::from(Span::styled("  Press Esc to close.", dim)),
        Line::from(""),
    ];

    // Wide enough for the longest value plus indent; hand-copied values must
    // never clip. When the terminal is narrower, rows wrap instead.
    let longest = text_layout::width(whip_url)
        .max(text_layout::width(stream_key))
        .max(text_layout::width(watch_url))
        .max(58)
        .min(u16::MAX as usize) as u16;
    let w = longest
        .saturating_add(6)
        .min(area.width.saturating_sub(4))
        .max(20);
    let inner_w = w.saturating_sub(2).max(1);
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    let h = paragraph
        .line_count(inner_w)
        .saturating_add(2)
        .min(area.height.saturating_sub(2) as usize) as u16;

    let [popup_area] = Layout::vertical([Constraint::Length(h)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Length(w)])
        .flex(Flex::Center)
        .areas(popup_area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Go Live via OBS ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
