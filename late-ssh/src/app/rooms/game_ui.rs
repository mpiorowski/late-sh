use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;

enum RoomSidebarContent<'a> {
    Info(Vec<Line<'a>>),
}

pub fn draw_game_frame_with_info_sidebar<'a>(
    frame: &mut Frame,
    area: Rect,
    _title: &str,
    info_lines: Vec<Line<'a>>,
    show_info_sidebar: bool,
) -> Rect {
    let (content_area, sidebar_area) = info_sidebar_layout(area, show_info_sidebar);

    if let Some(sidebar_area) = sidebar_area {
        draw_info_sidebar(frame, sidebar_area, RoomSidebarContent::Info(info_lines));
    }

    content_area
}

fn info_sidebar_layout(area: Rect, show_info_sidebar: bool) -> (Rect, Option<Rect>) {
    if show_info_sidebar {
        let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(28)]).split(area);
        (cols[0], Some(cols[1]))
    } else {
        (area, None)
    }
}

fn draw_info_sidebar(frame: &mut Frame, area: Rect, content: RoomSidebarContent<'_>) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    frame.render_widget(block, area);

    let inner_x = area.x.saturating_add(2);
    let inner = Rect {
        x: inner_x,
        y: area.y,
        width: area.width.saturating_sub(inner_x.saturating_sub(area.x)),
        height: area.height,
    };

    if inner.height < 4 || inner.width < 10 {
        return;
    }

    match content {
        RoomSidebarContent::Info(lines) => frame.render_widget(Paragraph::new(lines), inner),
    }
}

pub fn draw_game_overlay(
    frame: &mut Frame,
    area: Rect,
    heading: &str,
    subtitle: &str,
    color: Color,
) {
    let overlay_area = centered_rect(area, 28.min(area.width), 4.min(area.height));
    let overlay = Paragraph::new(vec![
        Line::from(Span::styled(
            format!(" {heading} "),
            Style::default()
                .bg(color)
                .fg(ratatui::style::Color::Reset)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            subtitle.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color)),
    );
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(overlay, overlay_area);
}

pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

pub fn info_label_value<'a>(label: &'a str, value: String, color: Color) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{:<11}", label),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            value,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

pub fn key_hint(key: &str, desc: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<12}", key),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled(desc.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

pub fn info_tagline(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::TEXT_MUTED())
            .add_modifier(Modifier::ITALIC),
    ))
}
