use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use unicode_width::UnicodeWidthChar;

use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;

use super::api::{CircMessage, CsNotification, CsPost};
use super::state::{
    ComposeField, ComposeModal, LinkField, LinkModal, LinkStatus, Modal, OpenRoom, ReplyModal,
    RoomsModal, State, TITLE_MAX_CHARS, View,
};
use super::svc::CsThread;

const FEED_ITEM_HEIGHT: u16 = 4;

pub fn draw_pane(frame: &mut Frame, area: Rect, state: &State) {
    // A chat room is its own rail entry, and the pane is where that entry
    // renders: selecting the row is what opened the room, so the room wins
    // over the feed views below.
    if let Some(room) = &state.open_room {
        draw_room(frame, area, room);
        return;
    }
    match &state.link {
        LinkStatus::Unknown => {
            frame.render_widget(
                Paragraph::new("Checking your cyberspace link...")
                    .style(Style::default().fg(theme::TEXT_DIM())),
                area,
            );
        }
        LinkStatus::Unlinked => draw_pitch(frame, area),
        LinkStatus::Linked { username } => match state.view {
            View::Feed => draw_feed(frame, area, state, username),
            View::Thread => draw_thread(frame, area, state),
            View::Notifications => draw_notifications(frame, area, state),
        },
    }
}

/// The composer-gap hint line, one string per view (drawn by chat ui like the
/// RSS footer block).
pub fn footer_hint(state: &State) -> &'static str {
    if !state.is_linked() {
        return " Enter link your cyberspace account";
    }
    // Only the reading hint: while the composer is open it occupies this slot
    // itself, so there is no hint row to write into.
    if state.open_room.is_some() {
        return " j/k scroll · g newest · i write · b leave";
    }
    match state.view {
        View::Feed => {
            " j/k navigate · g top · Enter open · p post · n notifications · r refresh · /cs chat rooms"
        }
        View::Thread => " j/k scroll · g top · r reply · b back",
        View::Notifications => " j/k navigate · g top · Enter open the entry · b back",
    }
}

fn draw_pitch(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "cyberspace.online",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "A small, human social network for computer people, a lot like this one.",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(Span::styled(
            "Link your cyberspace account and late.sh becomes your client: read the",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(Span::styled(
            "feed, reply, and publish entries without leaving your terminal.",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Everything happens as you, under your own account. late.sh stores a",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "login token, never your password. /cs unlink forgets it.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "No account yet? Sign up at ",
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                "https://cyberspace.online",
                Style::default().fg(theme::AMBER()),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(theme::TEXT())),
            Span::styled(
                "Enter",
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" to link your account.", Style::default().fg(theme::TEXT())),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_feed(frame: &mut Frame, area: Rect, state: &State, username: &str) {
    let [header_area, area] =
        Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);
    draw_feed_header(frame, header_area, state, username);

    if state.posts.is_empty() {
        let text = if state.loading {
            "Loading the cyberspace feed...".to_string()
        } else {
            "No entries yet. Press r to refresh, p to post.".to_string()
        };
        frame.render_widget(
            Paragraph::new(Text::from(text)).style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    }

    let visible_items = ((area.height / FEED_ITEM_HEIGHT).max(1)) as usize;
    let selected = state.selected.min(state.posts.len().saturating_sub(1));
    let start = selected.saturating_sub(visible_items.saturating_sub(1));
    let end = (start + visible_items).min(state.posts.len());
    let constraints =
        std::iter::repeat_n(Constraint::Length(FEED_ITEM_HEIGHT), end - start).collect::<Vec<_>>();
    let rows = Layout::vertical(constraints).split(area);

    for (row, row_area) in rows.iter().copied().enumerate() {
        let index = start + row;
        let post = &state.posts[index];
        let is_selected = index == selected;
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(theme::row_style(is_selected));
        let content = block.inner(row_area);
        frame.render_widget(block, row_area);
        frame.render_widget(
            Paragraph::new(feed_entry_lines(post, state.is_unread_entry(post)))
                .wrap(Wrap { trim: true }),
            content,
        );
    }
}

/// Identity and new entries on the left, notifications on the right. The rail
/// badge is the sum of the two, so this row is where the number gets split
/// back into the things it counts and each half points at the key for it.
fn draw_feed_header(frame: &mut Frame, area: Rect, state: &State, username: &str) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (right_text, right_style) = if state.unread_notifications() > 0 {
        (
            format!(
                "● {} unread notification{} · n to open",
                state.unread_notifications(),
                if state.unread_notifications() == 1 {
                    ""
                } else {
                    "s"
                }
            ),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            "n notifications".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )
    };
    let right_width = right_text.chars().count() as u16;
    let [left_area, right_area] =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)]).areas(inner);

    let mut left = vec![
        Span::styled(
            format!("@{username}"),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " on cyberspace.online",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    if state.unread_entries() > 0 {
        left.push(Span::styled(
            format!(" · {} new", state.unread_entries()),
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(left)), left_area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(right_text, right_style))),
        right_area,
    );
}

fn feed_entry_lines(post: &CsPost, unread: bool) -> Vec<Line<'static>> {
    let title = post
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| first_content_line(&post.content));
    let mut title_spans = Vec::new();
    if unread {
        title_spans.push(Span::styled(
            "● ",
            Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        ));
    }
    title_spans.push(Span::styled(
        title,
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ));
    let mut meta = vec![Span::styled(
        post.author_username.clone(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(stamp) = relative_stamp(post.created_at) {
        meta.push(Span::styled(
            format!(" - {stamp}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if post.replies_count > 0 {
        meta.push(Span::styled(
            format!(" - {} replies", post.replies_count),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if !post.topics.is_empty() {
        meta.push(Span::styled(
            format!(" - {}", post.topics.join(", ")),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    if post.is_nsfw {
        meta.push(Span::styled(" - NSFW", Style::default().fg(theme::ERROR())));
    }
    vec![
        Line::from(title_spans),
        Line::from(meta),
        Line::from(Span::styled(
            preview_text(&post.content, 200),
            Style::default().fg(theme::TEXT()),
        )),
    ]
}

fn draw_thread(frame: &mut Frame, area: Rect, state: &State) {
    let Some(thread) = &state.thread else {
        frame.render_widget(
            Paragraph::new("Loading entry...").style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    };

    let lines = thread_lines(thread, area.width as usize, state.loading);
    // Only this pass knows how the entry wrapped into this viewport, so it
    // hands the ceiling back for `j` to clamp against.
    let max_scroll = lines.len().saturating_sub(area.height as usize);
    state.thread_max_scroll.set(max_scroll);
    let scroll = state.thread_scroll.min(max_scroll) as u16;
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), area);
}

/// Every row the entry and its replies occupy at `width`, already wrapped. One
/// `Line` per rendered row is the whole point: the scroll ceiling is the
/// length of this vec, so a count that ignores wrapping leaves the tail of a
/// long entry unreachable.
fn thread_lines(thread: &CsThread, width: usize, loading: bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(title) = thread.post.title.clone().filter(|t| !t.trim().is_empty()) {
        lines.extend(wrapped_lines(
            &title,
            width,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let mut meta = vec![Span::styled(
        thread.post.author_username.clone(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(stamp) = relative_stamp(thread.post.created_at) {
        meta.push(Span::styled(
            format!(" - {stamp}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if !thread.post.topics.is_empty() {
        meta.push(Span::styled(
            format!(" - {}", thread.post.topics.join(", ")),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    lines.push(Line::from(meta));
    lines.push(Line::from(""));
    lines.extend(wrapped_lines(
        &thread.post.content,
        width,
        Style::default().fg(theme::TEXT()),
    ));
    lines.push(Line::from(""));
    let replies_header = if loading && thread.replies.is_empty() {
        "replies (loading...)".to_string()
    } else {
        format!("replies ({})", thread.replies.len())
    };
    lines.push(Line::from(Span::styled(
        replies_header,
        Style::default()
            .fg(theme::TEXT_DIM())
            .add_modifier(Modifier::BOLD),
    )));
    for reply in &thread.replies {
        lines.push(Line::from(""));
        let mut reply_meta = vec![Span::styled(
            reply.author_username.clone(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )];
        if let Some(stamp) = relative_stamp(reply.created_at) {
            reply_meta.push(Span::styled(
                format!(" - {stamp}"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        lines.push(Line::from(reply_meta));
        lines.extend(wrapped_lines(
            &reply.content,
            width,
            Style::default().fg(theme::TEXT()),
        ));
    }
    lines
}

/// Pre-wrap plain text into one `Line` per rendered row. The thread view wraps
/// here rather than handing `Wrap` to the paragraph so that `lines.len()` is
/// the real height: counting unwrapped lines puts the scroll ceiling short of
/// the end of the entry, and an entry written as a few long paragraphs (the
/// normal shape of a markdown post) does not scroll at all.
fn wrapped_lines(text: &str, width: usize, style: Style) -> Vec<Line<'static>> {
    // Their editor sends CRLF; a stray \r would render as a control glyph.
    let text = text.replace('\r', "");
    let width = width.max(1);
    text.split('\n')
        .flat_map(|paragraph| {
            let rows = wrap_paragraph(paragraph, width);
            if rows.is_empty() {
                vec![String::new()]
            } else {
                rows
            }
        })
        .map(|row| Line::from(Span::styled(row, style)))
        .collect()
}

/// Greedy wrap budgeted by display column, not char count: CJK and emoji
/// occupy two columns each, and a row over-full in columns gets truncated by
/// the widget, dropping text off the right edge. Breaks at the last whitespace
/// that fits, hard-breaking words wider than the pane.
fn wrap_paragraph(paragraph: &str, width: usize) -> Vec<String> {
    wrap_paragraph_hanging(paragraph, width, width)
}

/// Wrap with a narrower first row, for text that follows a prefix on its own
/// line (a chat message's stamp and author) and hangs under it afterwards.
fn wrap_paragraph_hanging(paragraph: &str, first_width: usize, rest_width: usize) -> Vec<String> {
    let chars: Vec<char> = paragraph.chars().collect();
    let mut rows = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let width = match rows.is_empty() {
            true => first_width.max(1),
            false => rest_width.max(1),
        };
        let mut cols = 0;
        let mut end = start;
        while end < chars.len() {
            let char_cols = chars[end].width().unwrap_or(0);
            // A single glyph wider than the pane still has to land somewhere,
            // so a row always takes at least one char.
            if cols + char_cols > width && end > start {
                break;
            }
            cols += char_cols;
            end += 1;
        }

        if end == chars.len() {
            rows.push(chars[start..end].iter().collect());
            break;
        }

        let break_at = chars[start..end]
            .iter()
            .rposition(|ch| ch.is_whitespace())
            .map(|idx| start + idx);
        match break_at {
            Some(split) if split > start => {
                rows.push(chars[start..split].iter().collect());
                start = split + 1;
            }
            _ => {
                rows.push(chars[start..end].iter().collect());
                start = end;
            }
        }
    }

    rows
}

/// The room picker: their whole chat roster, with a check against the rooms
/// already on the rail. There is no join or leave over there, so the check is
/// about our own rail and says so.
fn draw_rooms_modal(frame: &mut Frame, area: Rect, rooms: &RoomsModal, state: &State) {
    let popup = centered_rect(area, 56, 20);
    frame.render_widget(Clear, popup);
    let block = modal_block(" Add cyberspace chat rooms ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let areas = Layout::vertical([
        Constraint::Length(1), // hint
        Constraint::Length(1), // blank
        Constraint::Min(3),    // roster
        Constraint::Length(1), // status
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                " add or remove  ".to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled("j/k", Style::default().fg(theme::AMBER())),
            Span::styled(
                " move  ".to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled("Esc", Style::default().fg(theme::ERROR())),
            Span::styled(" close".to_string(), Style::default().fg(theme::TEXT_DIM())),
        ]))
        .style(Style::default().bg(theme::BG_CANVAS())),
        areas[0],
    );

    if rooms.roster.is_empty() {
        let text = match rooms.loading {
            true => "Loading their chat rooms...",
            false => "No chat rooms available to your account.",
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM())),
            areas[2],
        );
    } else {
        let height = areas[2].height.max(1) as usize;
        let selected = rooms.selected.min(rooms.roster.len().saturating_sub(1));
        let start = selected.saturating_sub(height.saturating_sub(1));
        let lines: Vec<Line<'static>> = rooms
            .roster
            .iter()
            .enumerate()
            .skip(start)
            .take(height)
            .map(|(index, room)| {
                let is_selected = index == selected;
                let on_rail = state.is_pinned(room.key());
                let mut spans = vec![
                    Span::styled(
                        match is_selected {
                            true => "> ",
                            false => "  ",
                        },
                        Style::default().fg(theme::AMBER()),
                    ),
                    Span::styled(
                        match on_rail {
                            true => "[x] ",
                            false => "[ ] ",
                        },
                        Style::default().fg(match on_rail {
                            true => theme::SUCCESS(),
                            false => theme::TEXT_FAINT(),
                        }),
                    ),
                    Span::styled(
                        format!("#{}", room.key()),
                        match is_selected {
                            true => Style::default()
                                .fg(theme::TEXT_BRIGHT())
                                .add_modifier(Modifier::BOLD),
                            false => Style::default().fg(theme::TEXT()),
                        },
                    ),
                ];
                if room.online_count > 0 {
                    spans.push(Span::styled(
                        format!("  {} here", room.online_count),
                        Style::default().fg(theme::TEXT_DIM()),
                    ));
                }
                Line::from(spans)
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), areas[2]);
    }

    let status = match &rooms.error {
        Some(error) => Line::from(Span::styled(
            error.clone(),
            Style::default().fg(theme::ERROR()),
        )),
        None => Line::from(Span::styled(
            "Rooms you add become entries under cyberspace in your rail.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().bg(theme::BG_CANVAS())),
        areas[3],
    );
}

/// One of their chat rooms, live. Everything on screen arrived through this
/// session's own stream and renders for this user alone.
fn draw_room(frame: &mut Frame, area: Rect, room: &OpenRoom) {
    // No composer here: writing happens in the chat composer slot below the
    // pane, the same box every other room types into (`chat::ui`).
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Fill(1)]).areas(area);

    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let header_inner = block.inner(header_area);
    frame.render_widget(block, header_area);
    let mut header = vec![
        Span::styled(
            format!("#{}", room.slug),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            " on cyberspace.online",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    if room.stream_down {
        header.push(Span::styled(
            "  · not live, press b and come back",
            Style::default().fg(theme::ERROR()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(header)), header_inner);

    if room.messages.is_empty() {
        let text = match room.loading {
            true => "Joining the room...",
            false => "Nothing said here yet. Press i to write.",
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM())),
            body_area,
        );
    } else {
        let lines = room_lines(&room.messages, body_area.width as usize);
        // Scroll counts rendered rows back from the newest, so a room opens
        // live at the bottom the way a chat room should. Only the renderer
        // knows how many rows the conversation wrapped to, so it writes the
        // ceiling back for `room_scroll` to clamp against (same contract as
        // the thread view's `thread_max_scroll`).
        let height = body_area.height as usize;
        let max_scroll = lines.len().saturating_sub(height);
        room.max_scroll.set(max_scroll);
        let end = lines.len().saturating_sub(room.scroll.min(max_scroll));
        let start = end.saturating_sub(height);
        let visible: Vec<Line<'static>> = lines[start..end].to_vec();
        frame.render_widget(Paragraph::new(visible), body_area);
    }
}

/// The conversation as rendered rows, pre-wrapped so one `Line` is one row.
/// Handing `Wrap` to the paragraph instead would make the scroll window lie:
/// it counts rows, and a wrapped message occupies more than the one it is
/// counted as, which pushes the newest messages off the bottom of the pane.
///
/// Continuations hang under the author rather than restarting at the margin,
/// so a long message still reads as one message.
fn room_lines(messages: &[CircMessage], width: usize) -> Vec<Line<'static>> {
    const STAMP: &str = "%H:%M";
    // "HH:MM " plus the indent continuations hang at.
    let indent_cols = 6usize;
    let mut rows: Vec<Line<'static>> = Vec::new();

    for message in messages {
        let stamp = message
            .at()
            .map(|at| at.format(STAMP).to_string())
            .unwrap_or_else(|| "     ".to_string());
        let stamp_span = Span::styled(
            format!("{stamp} "),
            Style::default().fg(theme::TEXT_FAINT()),
        );
        let text = message.display_text();

        // Each shape is a styled prefix on the first row plus one body of
        // text that wraps under it.
        let (prefix, body, body_style) = if message.deleted {
            (
                Span::styled(
                    format!("{} ", message.username),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
                text,
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            )
        } else if message.is_action {
            // `/me` and the emotes read as third person on their side too.
            (
                Span::raw(""),
                format!("* {} {text}", message.username),
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::ITALIC),
            )
        } else {
            let mut body = text;
            if let Some(label) = message.attachment_label() {
                match body.is_empty() {
                    true => body = label.to_string(),
                    false => body = format!("{body} {label}"),
                }
            }
            (
                Span::styled(
                    format!("{}: ", message.username),
                    Style::default()
                        .fg(theme::AMBER_DIM())
                        .add_modifier(Modifier::BOLD),
                ),
                body,
                Style::default().fg(theme::TEXT()),
            )
        };

        let prefix_cols: usize = prefix
            .content
            .chars()
            .map(|ch| ch.width().unwrap_or(0))
            .sum();
        let first_width = width.saturating_sub(indent_cols + prefix_cols);
        let rest_width = width.saturating_sub(indent_cols);
        let wrapped = wrap_paragraph_hanging(&body, first_width, rest_width);

        match wrapped.split_first() {
            None => rows.push(Line::from(vec![stamp_span, prefix])),
            Some((first, rest)) => {
                rows.push(Line::from(vec![
                    stamp_span,
                    prefix,
                    Span::styled(first.clone(), body_style),
                ]));
                for row in rest {
                    rows.push(Line::from(vec![
                        Span::raw(" ".repeat(indent_cols)),
                        Span::styled(row.clone(), body_style),
                    ]));
                }
            }
        }
    }
    rows
}

fn draw_notifications(frame: &mut Frame, area: Rect, state: &State) {
    if state.notifications.is_empty() {
        let text = if state.loading {
            "Loading notifications..."
        } else {
            "No notifications."
        };
        frame.render_widget(
            Paragraph::new(text).style(Style::default().fg(theme::TEXT_DIM())),
            area,
        );
        return;
    }
    let visible = area.height.max(1) as usize;
    let selected = state
        .notif_selected
        .min(state.notifications.len().saturating_sub(1));
    let start = selected.saturating_sub(visible.saturating_sub(1));
    let lines: Vec<Line<'static>> = state
        .notifications
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(index, notification)| notification_line(notification, index == selected))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn notification_line(notification: &CsNotification, selected: bool) -> Line<'static> {
    let mut spans = Vec::new();
    if !notification.read {
        spans.push(Span::styled(
            "● ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        notification
            .actor_username
            .clone()
            .unwrap_or_else(|| "someone".to_string()),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        format!(" {}", describe_notification(&notification.kind)),
        Style::default().fg(theme::TEXT()),
    ));
    if let Some(stamp) = relative_stamp(notification.created_at) {
        spans.push(Span::styled(
            format!(" - {stamp}"),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans).style(theme::row_style(selected))
}

/// Human phrasing for the notification types the API documents; unknown
/// types fall back to the raw name so new server types stay readable.
fn describe_notification(kind: &str) -> String {
    match kind {
        "reply" => "replied to your entry".to_string(),
        "thread_reply" => "replied in a thread you watch".to_string(),
        "new_follower" => "followed you".to_string(),
        "unfollowed" => "unfollowed you".to_string(),
        "bookmark" => "bookmarked your entry".to_string(),
        "new_post_following" => "posted a new entry".to_string(),
        "new_post_friend" => "posted a new entry".to_string(),
        "post_mention" => "mentioned you in an entry".to_string(),
        "reply_mention" => "mentioned you in a reply".to_string(),
        "poke" => "poked you".to_string(),
        "chat_mention" => "mentioned you in chat".to_string(),
        "dm_message" => "sent you a c-mail".to_string(),
        "guild_new_thread" => "started a guild thread".to_string(),
        other => other.replace('_', " "),
    }
}

pub(crate) fn draw_modal(frame: &mut Frame, area: Rect, state: &State) {
    let Some(modal) = &state.modal else {
        return;
    };
    match modal {
        Modal::Link(link) => draw_link_modal(frame, area, link),
        Modal::Compose(compose) => draw_compose_modal(frame, area, compose),
        Modal::Reply(reply) => draw_reply_modal(frame, area, reply),
        // The picker needs the pinned list to check its rows, which lives on
        // the pane rather than in the modal: the rail is the truth about what
        // was added, and the modal is a view onto it.
        Modal::Rooms(rooms) => draw_rooms_modal(frame, area, rooms, state),
    }
}

/// The link funnel. Unlinked users never reach the pane, so this modal is
/// where they meet cyberspace.online: it carries the pitch that used to live
/// on the pane's empty state.
fn draw_link_modal(frame: &mut Frame, area: Rect, link: &LinkModal) {
    let popup = centered_rect(area, 64, 18);
    frame.render_widget(Clear, popup);
    let block = modal_block(" Link cyberspace account ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    // A blank row between every group and a column of padding down each side.
    // The lengths fill the inner height exactly, so nothing pools into dead
    // space at the bottom.
    let areas = Layout::vertical([
        Constraint::Length(1), // pad
        Constraint::Length(3), // pitch
        Constraint::Length(1), // blank
        Constraint::Length(1), // keys
        Constraint::Length(1), // blank
        Constraint::Length(3), // email
        Constraint::Length(1), // blank
        Constraint::Length(3), // password
        Constraint::Length(1), // blank
        Constraint::Length(1), // token note
    ])
    .horizontal_margin(2)
    .split(inner);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "A small, human social network for computer people, a lot",
                Style::default().fg(theme::TEXT()),
            )),
            Line::from(Span::styled(
                "like this one. Link yours and late.sh becomes its client.",
                Style::default().fg(theme::TEXT()),
            )),
            Line::from(vec![
                Span::styled("No account yet? ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    "https://cyberspace.online",
                    Style::default().fg(theme::AMBER()),
                ),
            ]),
        ])
        .style(Style::default().bg(theme::BG_CANVAS())),
        areas[1],
    );
    frame.render_widget(hint_line(link.busy, "link"), areas[3]);
    draw_input_field(
        frame,
        areas[5],
        "Email",
        &link.email,
        link.focus == LinkField::Email,
    );
    draw_input_field(
        frame,
        areas[7],
        "Password",
        &link.password,
        link.focus == LinkField::Password,
    );
    // Busy/error takes the bottom row when there is something to say;
    // otherwise the credentials note keeps it, so the row is never dead space.
    if link.busy || link.error.is_some() {
        draw_modal_status(frame, areas[9], link.busy, &link.error, "Linking...");
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "late.sh keeps a login token, never your password.",
                Style::default().fg(theme::TEXT_FAINT()),
            )))
            .style(Style::default().bg(theme::BG_CANVAS())),
            areas[9],
        );
    }
}

fn draw_compose_modal(frame: &mut Frame, area: Rect, compose: &ComposeModal) {
    let popup = centered_rect(area, 76, 20);
    frame.render_widget(Clear, popup);
    let block = modal_block(" New cyberspace entry ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(hint_line(compose.busy, "publish"), areas[0]);
    draw_input_field(
        frame,
        areas[1],
        &format!(
            "Title {}/{TITLE_MAX_CHARS}",
            compose.title.lines().join(" ").chars().count()
        ),
        &compose.title,
        compose.focus == ComposeField::Title,
    );
    draw_input_field(
        frame,
        areas[2],
        "Topics",
        &compose.topics,
        compose.focus == ComposeField::Topics,
    );
    draw_input_field(
        frame,
        areas[3],
        "Body (markdown, Alt+Enter for newline)",
        &compose.body,
        compose.focus == ComposeField::Body,
    );
    draw_modal_status(
        frame,
        areas[4],
        compose.busy,
        &compose.error,
        "Publishing...",
    );
}

fn draw_reply_modal(frame: &mut Frame, area: Rect, reply: &ReplyModal) {
    let popup = centered_rect(area, 70, 14);
    frame.render_widget(Clear, popup);
    let title = reply
        .post
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| reply.post.author_username.clone());
    let block = modal_block(&format!(" Reply · {title} "));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let areas = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(hint_line(reply.busy, "send"), areas[0]);
    draw_input_field(
        frame,
        areas[1],
        "Reply (markdown, Alt+Enter for newline)",
        &reply.body,
        true,
    );
    draw_modal_status(frame, areas[2], reply.busy, &reply.error, "Sending...");
}

fn hint_line(busy: bool, verb: &str) -> Paragraph<'static> {
    let spans = if busy {
        vec![
            Span::styled("Esc".to_string(), Style::default().fg(theme::ERROR())),
            Span::styled(
                " cancel".to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]
    } else {
        vec![
            Span::styled("Enter".to_string(), Style::default().fg(theme::SUCCESS())),
            Span::styled(format!(" {verb}  "), Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Tab".to_string(), Style::default().fg(theme::AMBER())),
            Span::styled(
                " next  ".to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled("Esc".to_string(), Style::default().fg(theme::ERROR())),
            Span::styled(
                " discard".to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]
    };
    Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::BG_CANVAS()))
}

fn draw_modal_status(
    frame: &mut Frame,
    area: Rect,
    busy: bool,
    error: &Option<String>,
    busy_label: &str,
) {
    let line = if busy {
        Line::from(Span::styled(
            format!(" {busy_label}"),
            Style::default().fg(theme::AMBER()),
        ))
    } else if let Some(error) = error {
        Line::from(Span::styled(
            format!(" {error}"),
            Style::default().fg(theme::ERROR()),
        ))
    } else {
        Line::from("")
    };
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(theme::BG_CANVAS())),
        area,
    );
}

fn draw_input_field(
    frame: &mut Frame,
    area: Rect,
    label: &str,
    input: &ratatui_textarea::TextArea<'static>,
    focused: bool,
) {
    let border = if focused {
        theme::BORDER_ACTIVE()
    } else {
        theme::BORDER()
    };
    let block = Block::default()
        .title(Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(if focused {
                    theme::TEXT_BRIGHT()
                } else {
                    theme::TEXT_DIM()
                })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(input, inner);
}

fn modal_block(title: &str) -> Block<'static> {
    Block::default()
        .title(title.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        .style(Style::default().bg(theme::BG_CANVAS()))
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn first_content_line(content: &str) -> String {
    let line = content.lines().next().unwrap_or("").trim();
    preview_text(line, 80)
}

fn preview_text(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let mut out: String = flat.chars().take(max_chars).collect();
    out.push('…');
    out
}

fn relative_stamp(created_at: Option<DateTime<Utc>>) -> Option<String> {
    created_at.map(format_relative_time)
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
