use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    chat::state::{ChatState, MessageSearchHit},
    common::primitives::format_relative_time_short,
    common::theme,
    room_search_modal::state::{
        MessageQuery, ModalQuery, RoomSearchModalState, filtered_items, hit_room_label,
        parse_modal_query, resolve_message_scope,
    },
};
use uuid::Uuid;

const MODAL_WIDTH: u16 = 96;
const MODAL_HEIGHT: u16 = 30;
/// Outer height of the message-mode detail pane (bordered). Constant so the
/// pane never resizes while browsing results. Sized for a header row, four
/// context rows either side of the hit, and up to three hit body rows.
const DETAIL_PANE_HEIGHT: u16 = 14;
/// Context rows shown on each side of the hit when the pane is full height.
const CONTEXT_ROWS_EACH_SIDE: usize = 4;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
) {
    let popup = centered_rect(
        area,
        MODAL_WIDTH.min(area.width.saturating_sub(4)).max(20),
        MODAL_HEIGHT.min(area.height.saturating_sub(2)).max(5),
    );
    frame.render_widget(Clear, popup);

    let query = parse_modal_query(state.query());
    let title = match query {
        ModalQuery::Rooms => " Jump To Room ",
        ModalQuery::Messages(_) => " Search Messages ",
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 5 || inner.width < 20 {
        frame.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }

    match query {
        ModalQuery::Rooms => {
            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

            draw_query(frame, layout[0], state);
            draw_results(frame, layout[1], state, chat, user_id);
            draw_footer(frame, layout[2], false);
        }
        ModalQuery::Messages(message_query) => {
            let detail_height = DETAIL_PANE_HEIGHT.min(inner.height.saturating_sub(5));
            let layout = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(detail_height),
                Constraint::Length(1),
            ])
            .split(inner);

            draw_query(frame, layout[0], state);
            draw_message_results(frame, layout[1], state, chat, user_id, &message_query);
            draw_message_detail(frame, layout[2], state, chat, user_id);
            draw_footer(frame, layout[3], true);
        }
    }
}

fn draw_query(frame: &mut Frame, area: Rect, state: &RoomSearchModalState) {
    let block = Block::default()
        .title(" Search ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);

    let mut query = state.query().to_string();
    query.push('█');
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            query,
            Style::default().fg(theme::TEXT_BRIGHT()),
        ))),
        inner,
    );
}

fn draw_results(
    frame: &mut Frame,
    area: Rect,
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
) {
    let items = filtered_items(chat, user_id, state.query());
    let selected = state.selected().min(items.len().saturating_sub(1));
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  No matching rooms",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            area,
        );
        return;
    }

    let height = area.height as usize;
    if height == 0 {
        return;
    }
    let width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();
    let rows = result_rows(&items);
    let start = result_view_start(&rows, selected, height);
    for row in rows.iter().skip(start).take(height) {
        match *row {
            ResultRow::Header(favorite) => {
                let label = if favorite { "favorites" } else { "rooms" };
                lines.push(Line::from(Span::styled(
                    format!("  {label}"),
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                )));
            }
            ResultRow::Item(index) => {
                lines.push(result_item_line(&items[index], index == selected, width));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResultRow {
    Header(bool),
    Item(usize),
}

fn result_rows(items: &[crate::app::room_search_modal::state::RoomSearchItem]) -> Vec<ResultRow> {
    let mut rows = Vec::with_capacity(items.len().saturating_mul(2));
    let mut last_section: Option<bool> = None;
    for (index, item) in items.iter().enumerate() {
        if last_section != Some(item.favorite) {
            last_section = Some(item.favorite);
            rows.push(ResultRow::Header(item.favorite));
        }
        rows.push(ResultRow::Item(index));
    }
    rows
}

fn result_view_start(rows: &[ResultRow], selected: usize, height: usize) -> usize {
    if height == 0 || rows.is_empty() {
        return 0;
    }
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, ResultRow::Item(index) if *index == selected))
        .unwrap_or(0);
    selected_row
        .saturating_sub(height.saturating_sub(1))
        .min(rows.len().saturating_sub(height))
}

fn result_item_line(
    item: &crate::app::room_search_modal::state::RoomSearchItem,
    active: bool,
    width: usize,
) -> Line<'static> {
    let marker = if active { ">" } else { " " };
    let style = if active {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let meta_style = if active {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let unread = if item.unread_count > 0 {
        item.unread_count.to_string()
    } else {
        String::new()
    };
    let unread_style = if item.unread_count > 0 {
        let base = Style::default().fg(theme::AMBER_GLOW());
        if active {
            base.bg(theme::BG_SELECTION())
        } else {
            base
        }
    } else {
        meta_style
    };
    let unread_width = 6usize.min(width.saturating_sub(6));
    let meta_width = 16usize.min(width.saturating_sub(unread_width + 6));
    let label_width = width.saturating_sub(meta_width + unread_width + 5);
    Line::from(vec![
        Span::styled(format!("{marker} "), style),
        Span::styled(
            pad_right(&truncate_to_width(&item.label, label_width), label_width),
            style,
        ),
        Span::styled(" ", style),
        Span::styled(
            pad_right(&truncate_to_width(&item.meta, meta_width), meta_width),
            meta_style,
        ),
        Span::styled(" ", style),
        Span::styled(
            pad_left(&truncate_to_width(&unread, unread_width), unread_width),
            unread_style,
        ),
    ])
}

/// One-line status shown in the message results area when there are no rows
/// to draw.
fn message_status_text(
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
    query: &MessageQuery,
) -> String {
    if chat.message_search.loading {
        return "  Searching...".to_string();
    }
    if let Some(error) = &chat.message_search.error {
        return format!("  {error}");
    }
    if let Some(scope) = &query.scope
        && resolve_message_scope(chat, user_id, scope).is_none()
    {
        return match scope {
            crate::app::room_search_modal::state::MessageScope::Room(_) => {
                "  Finish the #room scope (a joined room), then type your query".to_string()
            }
            crate::app::room_search_modal::state::MessageScope::Dm(_) => {
                "  Finish the @user scope (an open DM), then type your query".to_string()
            }
        };
    }
    if query.text.chars().count() < crate::app::chat::svc::SEARCH_MIN_CHARS {
        return "  Type at least 3 characters to search".to_string();
    }
    if state.query_recently_edited() {
        return "  Searching...".to_string();
    }
    "  No matching messages".to_string()
}

fn draw_message_results(
    frame: &mut Frame,
    area: Rect,
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
    query: &MessageQuery,
) {
    let hits = &chat.message_search.hits;
    if hits.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message_status_text(state, chat, user_id, query),
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            area,
        );
        return;
    }

    let height = area.height as usize;
    if height == 0 {
        return;
    }
    let width = area.width as usize;
    let selected = state.selected().min(hits.len().saturating_sub(1));
    let start = selected
        .saturating_sub(height.saturating_sub(1))
        .min(hits.len().saturating_sub(height.min(hits.len())));

    let mut lines: Vec<Line<'static>> = Vec::new();
    for (index, hit) in hits.iter().enumerate().skip(start).take(height) {
        lines.push(message_hit_line(
            hit,
            chat,
            user_id,
            index == selected,
            width,
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn message_hit_line(
    hit: &MessageSearchHit,
    chat: &ChatState,
    user_id: Uuid,
    active: bool,
    width: usize,
) -> Line<'static> {
    let marker = if active { ">" } else { " " };
    let base_bg = if active {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };
    let room_style = base_bg.fg(if active {
        theme::AMBER_GLOW()
    } else {
        theme::TEXT()
    });
    let author_style = base_bg.fg(if active {
        theme::TEXT_BRIGHT()
    } else {
        theme::TEXT_DIM()
    });
    let time_style = base_bg.fg(theme::TEXT_FAINT());
    let snippet_style = base_bg.fg(if active {
        theme::TEXT_BRIGHT()
    } else {
        theme::TEXT_DIM()
    });
    let match_style = base_bg.fg(theme::AMBER_GLOW()).add_modifier(Modifier::BOLD);

    let room = hit_room_label(chat, user_id, hit.message.room_id);
    let author = chat
        .usernames
        .get(&hit.message.user_id)
        .cloned()
        .unwrap_or_else(|| "?".to_string());
    let time = format_relative_time_short(hit.message.created);

    let room_width = 14usize.min(width.saturating_sub(10));
    let author_width = 12usize.min(width.saturating_sub(room_width + 10));
    let time_width = 4usize;
    let snippet_width = width.saturating_sub(room_width + author_width + time_width + 6);

    let mut spans = vec![
        Span::styled(format!("{marker} "), room_style),
        Span::styled(
            pad_right(&truncate_to_width(&room, room_width), room_width),
            room_style,
        ),
        Span::styled(" ", base_bg),
        Span::styled(
            pad_right(&truncate_to_width(&author, author_width), author_width),
            author_style,
        ),
        Span::styled(" ", base_bg),
        Span::styled(
            pad_right(&truncate_to_width(&time, time_width), time_width),
            time_style,
        ),
        Span::styled(" ", base_bg),
    ];

    // Budget the snippet across its three parts so the highlighted match
    // stays visible: the match gets first claim on the row, then the prefix
    // (its tail, for lead-in context), then the suffix.
    let match_text = truncate_to_width(&hit.snippet_match, snippet_width);
    let mut remaining = snippet_width.saturating_sub(UnicodeWidthStr::width(match_text.as_str()));
    let prefix_text = truncate_tail_to_width(&hit.snippet_prefix, remaining.min(24));
    remaining = remaining.saturating_sub(UnicodeWidthStr::width(prefix_text.as_str()));
    let suffix_text = truncate_to_width(&hit.snippet_suffix, remaining);

    spans.push(Span::styled(prefix_text, snippet_style));
    spans.push(Span::styled(match_text, match_style));
    let mut suffix_padded = suffix_text;
    let used: usize = spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum::<usize>()
        + UnicodeWidthStr::width(suffix_padded.as_str());
    suffix_padded.push_str(&" ".repeat(width.saturating_sub(used)));
    spans.push(Span::styled(suffix_padded, snippet_style));

    Line::from(spans)
}

fn draw_message_detail(
    frame: &mut Frame,
    area: Rect,
    state: &RoomSearchModalState,
    chat: &ChatState,
    user_id: Uuid,
) {
    let block = Block::default()
        .title(" Message ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area).inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let hits = &chat.message_search.hits;
    let Some(hit) = hits.get(state.selected().min(hits.len().saturating_sub(1))) else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No message selected",
                Style::default().fg(theme::TEXT_FAINT()),
            ))),
            inner,
        );
        return;
    };

    let room = hit_room_label(chat, user_id, hit.message.room_id);
    let author = chat
        .usernames
        .get(&hit.message.user_id)
        .cloned()
        .unwrap_or_else(|| "?".to_string());
    let stamp = hit.message.created.format("%Y-%m-%d %H:%M UTC").to_string();

    let mut lines = vec![Line::from(vec![
        Span::styled(room, Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(author, Style::default().fg(theme::TEXT_BRIGHT())),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(stamp, Style::default().fg(theme::TEXT_FAINT())),
    ])];

    let remaining = inner.height.saturating_sub(1) as usize;
    let (side_rows, hit_rows) = context_slot_layout(remaining);
    let context = chat.message_search.context.get(&hit.message.id);

    // Fixed slots: `side_rows` above, the hit, `side_rows` below. Missing
    // context (loading, start/end of history) renders empty rows so the hit
    // never moves while the window fills in.
    let before = context.map(|c| c.before.as_slice()).unwrap_or(&[]);
    let after = context.map(|c| c.after.as_slice()).unwrap_or(&[]);
    let before_shown = &before[before.len().saturating_sub(side_rows)..];
    for slot in 0..side_rows {
        let filled = slot + before_shown.len() >= side_rows;
        if filled {
            let message = &before_shown[slot + before_shown.len() - side_rows];
            lines.push(context_line(message, chat, inner.width as usize));
        } else {
            lines.push(Line::from(String::new()));
        }
    }

    let wrapped = wrap_plain(&hit.message.body, inner.width.saturating_sub(2) as usize);
    let truncated = wrapped.len() > hit_rows;
    for (index, row) in wrapped.into_iter().take(hit_rows.max(1)).enumerate() {
        let marker = if index == 0 { "> " } else { "  " };
        let mut text = format!("{marker}{row}");
        if truncated && index + 1 == hit_rows.max(1) {
            text.push('…');
        }
        lines.push(Line::from(Span::styled(
            text,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )));
    }

    for slot in 0..side_rows {
        match after.get(slot) {
            Some(message) => lines.push(context_line(message, chat, inner.width as usize)),
            None => lines.push(Line::from(String::new())),
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Split the detail pane's post-header rows into `(context rows per side,
/// hit body rows)`. Full height gives 4 + 4 context rows and 3 hit rows;
/// shorter panes shrink the context symmetrically before the hit.
fn context_slot_layout(remaining: usize) -> (usize, usize) {
    if remaining == 0 {
        return (0, 1);
    }
    let side_rows = CONTEXT_ROWS_EACH_SIDE.min(remaining.saturating_sub(1) / 2);
    (side_rows, (remaining - 2 * side_rows).max(1))
}

/// One dim context row: `author: body` flattened to a single line.
fn context_line(
    message: &late_core::models::chat_message::ChatMessage,
    chat: &ChatState,
    width: usize,
) -> Line<'static> {
    let author = chat
        .usernames
        .get(&message.user_id)
        .cloned()
        .unwrap_or_else(|| "?".to_string());
    let body = message.body.replace(['\n', '\r'], " ");
    let text = format!("  {author}: {body}");
    Line::from(Span::styled(
        truncate_to_width(&text, width),
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

/// Minimal word wrap by display width for the detail pane. Words longer
/// than the width hard-split; newlines are respected.
fn wrap_plain(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows = Vec::new();
    for source_line in text.lines() {
        let mut row = String::new();
        let mut row_width = 0usize;
        for word in source_line.split_whitespace() {
            let word_width = UnicodeWidthStr::width(word);
            if row_width > 0 && row_width + 1 + word_width > width {
                rows.push(std::mem::take(&mut row));
                row_width = 0;
            }
            if word_width > width {
                // Hard-split an over-long word across rows.
                for ch in word.chars() {
                    let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if row_width + ch_width > width {
                        rows.push(std::mem::take(&mut row));
                        row_width = 0;
                    }
                    row.push(ch);
                    row_width += ch_width;
                }
                continue;
            }
            if row_width > 0 {
                row.push(' ');
                row_width += 1;
            }
            row.push_str(word);
            row_width += word_width;
        }
        rows.push(row);
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

/// Truncate from the left, keeping the tail (used for snippet lead-in so the
/// text immediately before the match survives).
fn truncate_tail_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut kept: Vec<char> = Vec::new();
    let mut used = 1usize; // leading ellipsis
    for ch in text.chars().rev() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        kept.push(ch);
        used += ch_width;
    }
    let mut out = String::from("…");
    out.extend(kept.into_iter().rev());
    out
}

fn draw_footer(frame: &mut Frame, area: Rect, message_mode: bool) {
    let mut spans = vec![
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" jump  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
    ];
    if message_mode {
        spans.push(Span::styled(
            "Ctrl+Y",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " copy  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    } else {
        spans.push(Span::styled("?", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " search messages  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())));
    spans.push(Span::styled(
        " close",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn pad_right(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    let mut out = String::with_capacity(text.len() + width.saturating_sub(used));
    out.push_str(text);
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out
}

fn pad_left(text: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(text);
    let mut out = String::with_capacity(text.len() + width.saturating_sub(used));
    out.push_str(&" ".repeat(width.saturating_sub(used)));
    out.push_str(text);
    out
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width >= width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
