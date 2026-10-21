//! The watch modal's frame. The player paints its body black and records
//! where it went; the paired CLI's mpv draws the video over exactly that
//! rectangle. The setup box is a small card in the reader's frame style.

use std::time::Instant;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::state::{Mark, Player, SetupBox, WatchModal, WatchSize, centered_rect, close_rect};
use crate::app::chat::news::reader::state::domain_label;
use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;

const SETUP_MAX_WIDTH: u16 = 72;

pub(crate) fn draw(frame: &mut Frame, area: Rect, modal: &WatchModal) {
    match modal {
        WatchModal::Player(player) => draw_player(frame, area, player),
        WatchModal::Setup(setup) => draw_setup(frame, area, setup),
    }
}

fn draw_player(frame: &mut Frame, area: Rect, player: &Player) {
    let layout = Player::layout(player.size(), area);
    player.set_drawn(layout);
    frame.render_widget(Clear, layout.popup);

    match player.size() {
        WatchSize::Window => {
            frame.render_widget(frame_block(" Watching "), layout.popup);
            draw_close(frame, layout.close);
            if !layout.header.is_empty() {
                draw_header(frame, layout.header, player);
            }
        }
        WatchSize::Full => {
            frame.render_widget(
                Block::default().style(Style::default().bg(theme::BG_CANVAS())),
                layout.popup,
            );
        }
    }

    if !layout.body.is_empty() {
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            layout.body,
        );
        if let Some(line) = player.fallback_line() {
            let middle = Rect::new(
                layout.body.x,
                layout.body.y + layout.body.height / 2,
                layout.body.width,
                1,
            );
            frame.render_widget(
                Paragraph::new(line)
                    .centered()
                    .style(Style::default().fg(theme::TEXT_DIM()).bg(Color::Black)),
                middle,
            );
        }
    }

    if !layout.footer.is_empty() {
        frame.render_widget(
            Paragraph::new(footer_line(
                &player.footer_hints(),
                &player.status_text(Instant::now()),
                usize::from(layout.footer.width),
            )),
            layout.footer,
        );
    }
}

fn draw_header(frame: &mut Frame, header: Rect, player: &Player) {
    let title = match player.title().trim() {
        "" => "news update",
        title => title,
    };
    let meta = format!(
        "{} · shared by @{} {}",
        domain_label(player.url()),
        player.author(),
        format_relative_time(player.shared_at())
    );
    let lines = vec![
        Line::from(vec![
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(meta, Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::styled(
            "─".repeat(usize::from(header.width)),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ];
    frame.render_widget(Paragraph::new(lines), header);
}

/// The hints on the left, the status on the right. Trailing hints drop
/// first when the width runs short, then `Esc`, the status last.
pub(crate) fn footer_line(
    hints: &[(&'static str, &'static str)],
    status: &str,
    width: usize,
) -> Line<'static> {
    let key = Style::default().fg(theme::AMBER_DIM());
    let hint = Style::default().fg(theme::TEXT_DIM());
    let status_width = UnicodeWidthStr::width(status);
    let pair_width =
        |(k, h): &(&str, &str)| UnicodeWidthStr::width(*k) + 1 + UnicodeWidthStr::width(*h) + 2;
    let mut hints = hints.to_vec();
    let mut esc = true;
    loop {
        let used: usize =
            hints.iter().map(pair_width).sum::<usize>() + if esc { "Esc".len() } else { 0 };
        if used + 1 + status_width <= width {
            break;
        }
        match (hints.pop(), esc) {
            (Some(_), _) => {}
            (None, true) => esc = false,
            (None, false) => break,
        }
    }
    let mut spans = Vec::new();
    let mut used = 0;
    for (k, h) in &hints {
        used += pair_width(&(k, h));
        spans.push(Span::styled(*k, key));
        spans.push(Span::styled(format!(" {h}  "), hint));
    }
    if esc {
        used += "Esc".len();
        spans.push(Span::styled("Esc", key));
    }
    if used + 1 + status_width <= width {
        spans.push(Span::raw(" ".repeat(width - used - status_width)));
        spans.push(Span::styled(status.to_string(), hint));
    }
    Line::from(spans)
}

fn draw_setup(frame: &mut Frame, area: Rect, setup: &SetupBox) {
    let lines = setup_lines(setup);
    let width = SETUP_MAX_WIDTH.min(area.width);
    let height = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);
    frame.render_widget(frame_block(" Watch in the terminal "), popup);
    let close = close_rect(popup);
    setup.set_close(close);
    draw_close(frame, close);
    let inner = Block::default().borders(Borders::ALL).inner(popup);
    let inner = Rect::new(
        inner.x + 1,
        inner.y,
        inner.width.saturating_sub(2),
        inner.height,
    );
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Every line inside the setup box's border, top to bottom.
pub(crate) fn setup_lines(setup: &SetupBox) -> Vec<Line<'static>> {
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let label_width = setup
        .rows()
        .iter()
        .map(|row| UnicodeWidthStr::width(row.requirement.label()))
        .max()
        .unwrap_or(0);
    let mut lines = vec![
        Line::styled("Videos play right here when your setup has:", text),
        Line::default(),
    ];
    for row in setup.rows() {
        let (mark, mark_style) = match row.mark {
            Mark::Holds => ("✓", Style::default().fg(theme::AMBER())),
            Mark::Fails => ("✗", dim),
            Mark::Unknown => ("·", Style::default().fg(theme::TEXT_FAINT())),
        };
        lines.push(Line::from(vec![
            Span::styled(mark, mark_style),
            Span::styled(
                format!(" {:<label_width$}  ", row.requirement.label()),
                text,
            ),
            Span::styled(row.requirement.detail(), dim),
        ]));
    }
    lines.push(Line::default());
    if let Some(install) = setup.install() {
        lines.push(Line::styled(install.text(), dim));
    }
    lines.push(Line::styled(
        format!("Watch on the web: {}", setup.url()),
        dim,
    ));
    lines.push(Line::default());
    lines.push(Line::from(vec![
        Span::styled("c", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" copy link  ", dim),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
    ]));
    lines
}

fn frame_block(title: &'static str) -> Block<'static> {
    Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(Style::default().bg(theme::BG_CANVAS()))
}

fn draw_close(frame: &mut Frame, close: Rect) {
    if !close.is_empty() {
        frame.render_widget(
            Paragraph::new("[x]").style(Style::default().fg(theme::AMBER_GLOW())),
            close,
        );
    }
}
