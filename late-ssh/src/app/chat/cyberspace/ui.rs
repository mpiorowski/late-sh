use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::common::primitives::format_relative_time;
use crate::app::common::theme;

use super::api::{CsNotification, CsPost};
use super::state::{
    ComposeField, ComposeModal, LinkField, LinkModal, LinkStatus, Modal, ReplyModal, State,
    TITLE_MAX_CHARS, View,
};

const FEED_ITEM_HEIGHT: u16 = 4;

pub fn draw_pane(frame: &mut Frame, area: Rect, state: &State) {
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
    match state.view {
        View::Feed => " j/k navigate · Enter open · p post · n notifications · r refresh",
        View::Thread => " j/k scroll · r reply · b back",
        View::Notifications => " j/k navigate · Enter open the entry · b back",
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
            Paragraph::new(feed_entry_lines(post)).wrap(Wrap { trim: true }),
            content,
        );
    }
}

/// Identity on the left, notification state on the right. The rail badge only
/// says "cyberspace (3)", which reads like new entries; this is where the
/// count gets named as notifications and points at the key that opens them.
fn draw_feed_header(frame: &mut Frame, area: Rect, state: &State, username: &str) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (right_text, right_style) = if state.unread_count() > 0 {
        (
            format!(
                "● {} unread notification{} · n to open",
                state.unread_count(),
                if state.unread_count() == 1 { "" } else { "s" }
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

    frame.render_widget(
        Paragraph::new(Line::from(vec![
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
        ])),
        left_area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(right_text, right_style))),
        right_area,
    );
}

fn feed_entry_lines(post: &CsPost) -> Vec<Line<'static>> {
    let title = post
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| first_content_line(&post.content));
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
        Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
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

    let mut lines: Vec<Line<'static>> = Vec::new();
    if let Some(title) = thread.post.title.clone().filter(|t| !t.trim().is_empty()) {
        lines.push(Line::from(Span::styled(
            title,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )));
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
    for content_line in thread.post.content.lines() {
        lines.push(Line::from(Span::styled(
            content_line.to_string(),
            Style::default().fg(theme::TEXT()),
        )));
    }
    lines.push(Line::from(""));
    let replies_header = if state.loading && thread.replies.is_empty() {
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
        for content_line in reply.content.lines() {
            lines.push(Line::from(Span::styled(
                content_line.to_string(),
                Style::default().fg(theme::TEXT()),
            )));
        }
    }

    let max_scroll = lines.len().saturating_sub(area.height as usize);
    let scroll = state.thread_scroll.min(max_scroll) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        area,
    );
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

pub(crate) fn draw_modal(frame: &mut Frame, area: Rect, modal: &Modal) {
    match modal {
        Modal::Link(link) => draw_link_modal(frame, area, link),
        Modal::Compose(compose) => draw_compose_modal(frame, area, compose),
        Modal::Reply(reply) => draw_reply_modal(frame, area, reply),
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
