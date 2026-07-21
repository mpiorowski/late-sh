use crate::app::chat::svc::DiscoverRoomItem;
use crate::app::common::{primitives::format_relative_time, theme};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
};

pub struct DiscoverListView<'a> {
    pub items: Vec<&'a DiscoverRoomItem>,
    pub selected_index: usize,
    pub loading: bool,
    pub filtering: bool,
    pub query: &'a str,
}

/// Each room takes two rows: the `#slug` name on top, its stats underneath.
/// Two rows read comfortably in the narrow list column left by the preview pane.
const ITEM_HEIGHT: u16 = 2;
/// Below this total width there isn't room for both a readable list and a
/// preview, so the list takes the whole area and the preview is dropped.
const PREVIEW_MIN_WIDTH: u16 = 72;
/// Share of the width given to the list when the preview pane is shown; the
/// rest goes to the preview.
const LIST_PERCENT: u16 = 45;

pub fn draw_discover_list(frame: &mut Frame, area: Rect, view: &DiscoverListView<'_>) {
    if view.loading {
        let text = Text::from("Loading rooms...");
        let loading_p = Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(loading_p, area);
        return;
    }

    if view.items.is_empty() {
        let msg = if view.query.trim().is_empty() {
            "No public rooms to discover right now.".to_string()
        } else {
            format!("No rooms match \"{}\".", view.query.trim())
        };
        let empty_p = Paragraph::new(Text::from(msg)).style(Style::default().fg(theme::TEXT_DIM()));
        frame.render_widget(empty_p, area);
        return;
    }

    // Wide enough: split into a list column and a preview pane that tracks the
    // highlighted room. Otherwise the list keeps the full width.
    let (list_area, preview_area) = if area.width >= PREVIEW_MIN_WIDTH {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(LIST_PERCENT),
                Constraint::Percentage(100 - LIST_PERCENT),
            ])
            .split(area);
        (cols[0], Some(cols[1]))
    } else {
        (area, None)
    };

    draw_room_list(frame, list_area, view);

    if let Some(preview_area) = preview_area {
        let selected_index = view.selected_index.min(view.items.len().saturating_sub(1));
        draw_preview(frame, preview_area, view.items[selected_index]);
    }
}

fn draw_room_list(frame: &mut Frame, area: Rect, view: &DiscoverListView<'_>) {
    let visible_rows = (area.height / ITEM_HEIGHT).max(1) as usize;
    let selected_index = view.selected_index.min(view.items.len().saturating_sub(1));
    let start_index = selected_index.saturating_sub(visible_rows.saturating_sub(1));
    let end_index = (start_index + visible_rows).min(view.items.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (row, row_area) in layout.iter().copied().enumerate() {
        let idx = start_index + row;
        let item = view.items[idx];
        let selected = idx == selected_index;

        let bg_color = if selected {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };

        let lines = room_lines(item, selected);
        let p = Paragraph::new(lines).style(Style::default().bg(bg_color));
        frame.render_widget(p, row_area);
    }
}

/// Render the highlighted room's recent activity so the user can size up a room
/// without leaving the list. The messages are a snapshot captured at load time.
fn draw_preview(frame: &mut Frame, area: Rect, item: &DiscoverRoomItem) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER()))
        .padding(Padding::new(2, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let member_noun = if item.member_count == 1 {
        "member"
    } else {
        "members"
    };
    let activity = item
        .last_message_at
        .map(format_relative_time)
        .unwrap_or_else(|| "no messages yet".to_string());

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            format!("#{}", item.slug),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled(
                format!("{} {}", item.member_count, member_noun),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled("  ·  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("active {activity}"),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::from(""),
    ];

    if item.recent.is_empty() {
        lines.push(Line::from(Span::styled(
            "No messages yet. Be the first to post.",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else {
        for msg in &item.recent {
            lines.push(Line::from(vec![
                Span::styled(
                    msg.author.clone(),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", format_relative_time(msg.created)),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                msg.body.clone(),
                Style::default().fg(theme::TEXT()),
            )));
            lines.push(Line::from(""));
        }
    }

    let p = Paragraph::new(lines).wrap(Wrap { trim: true });
    frame.render_widget(p, inner);
}

/// The two rows for one room: the `#slug` name on top, its stats underneath.
/// The second row is indented to align under the name past the marker.
fn room_lines(item: &DiscoverRoomItem, selected: bool) -> Vec<Line<'static>> {
    let activity = item
        .last_message_at
        .map(format_relative_time)
        .unwrap_or_else(|| "no messages yet".to_string());
    let member_noun = if item.member_count == 1 {
        "member"
    } else {
        "members"
    };
    let message_noun = if item.message_count == 1 {
        "message"
    } else {
        "messages"
    };

    let marker = if selected { "› " } else { "  " };

    let name_line = Line::from(vec![
        Span::styled(marker, Style::default().fg(theme::AMBER())),
        Span::styled(
            format!("#{}", item.slug),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let stats_line = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{} {}", item.member_count, member_noun),
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled("  ·  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!("{} {}", item.message_count, message_noun),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled("  ·  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(activity, Style::default().fg(theme::TEXT_DIM())),
    ]);

    vec![name_line, stats_line]
}
