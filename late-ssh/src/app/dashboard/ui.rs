use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::chat::ui::{DashboardChatView, draw_dashboard_chat_card},
    app::common::{
        primitives::{format_duration_mmss, genre_label},
        theme,
    },
    app::vote::svc::{Genre, VoteCount},
    app::vote::ui::{VoteCardView, draw_vote_card},
};

// `draw_dashboard` receives the content pane after the outer app border and the
// fixed 24-column sidebar are removed. A 77-column terminal yields 51 columns
// of dashboard content, which is where we want to hide voting and keep screen 1
// usable for chat.
const DASHBOARD_HIDE_VOTE_AT_WIDTH: u16 = 51;
// A 65-column terminal yields 39 columns of dashboard content, which is where
// we drop the top stream/music card entirely so chat can use the reclaimed
// vertical space.
const DASHBOARD_HIDE_STREAM_AT_WIDTH: u16 = 39;
// Below this many rows the fixed 5-row stream card plus chat card no longer
// fit cleanly, so we collapse to chat-only rather than render clipped blocks.
const DASHBOARD_MIN_FULL_HEIGHT: u16 = 16;

pub struct DashboardRenderInput<'a> {
    pub now_playing: Option<&'a str>,
    pub vote_counts: &'a VoteCount,
    pub current_genre: Genre,
    pub next_switch_in: Duration,
    pub my_vote: Option<Genre>,
    pub show_header: bool,
    /// When `Some`, the user has 2+ favorites pinned and we render a
    /// quick-switch strip directly above the chat card. Each entry is
    /// `(room_id, label, is_active, unread_count)`. `None` hides the strip.
    pub favorites_strip: Option<&'a [(uuid::Uuid, String, bool, i64)]>,
    pub chat_view: DashboardChatView<'a>,
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    if !view.show_header {
        draw_chat_section(frame, area, view.favorites_strip, view.chat_view);
        return;
    }

    let stream_props = StreamCardProps {
        now_playing: view.now_playing.unwrap_or("Waiting for stream..."),
        current_genre: view.current_genre,
        leading_genre: view.vote_counts.winner_or(view.current_genre),
        next_switch_in: view.next_switch_in,
    };
    if area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH || area.height < DASHBOARD_MIN_FULL_HEIGHT {
        draw_chat_section(frame, area, view.favorites_strip, view.chat_view);
        return;
    }

    let sections = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);

    if area.width <= DASHBOARD_HIDE_VOTE_AT_WIDTH {
        draw_stream_card(frame, sections[0], &stream_props);
    } else {
        let top = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0]);
        draw_stream_card(frame, top[0], &stream_props);
        draw_vote_card(
            frame,
            top[1],
            &VoteCardView {
                vote_counts: view.vote_counts,
                my_vote: view.my_vote,
            },
        );
    }

    draw_chat_section(frame, sections[1], view.favorites_strip, view.chat_view);
}

pub(crate) fn favorites_strip_hit_test(
    area: Rect,
    show_header: bool,
    pins: &[(uuid::Uuid, String, bool, i64)],
    x: u16,
    y: u16,
) -> Option<uuid::Uuid> {
    let strip_area = favorites_strip_area(area, show_header, pins)?;
    if y != strip_area.y || x < strip_area.x || x >= strip_area.right() {
        return None;
    }

    let mut cursor_x = strip_area.x + 1;
    for (idx, (room_id, label, _, unread)) in pins.iter().enumerate() {
        if idx > 0 {
            cursor_x = cursor_x.saturating_add(1);
        }
        let slot = if idx < 9 {
            format!("{}:", idx + 1)
        } else {
            String::new()
        };
        let unread_suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        let pill = format!(" {slot}{label}{unread_suffix} ");
        let width = UnicodeWidthStr::width(pill.as_str()) as u16;
        let end_x = cursor_x.saturating_add(width);
        if x >= cursor_x && x < end_x {
            return Some(*room_id);
        }
        cursor_x = end_x;
    }
    None
}

/// Draws the chat card, with an optional pill strip above it. When the user
/// has 2+ favorites pinned, we carve one row off the top for the strip;
/// otherwise the chat card takes the whole area.
fn draw_chat_section(
    frame: &mut Frame,
    area: Rect,
    favorites_strip: Option<&[(uuid::Uuid, String, bool, i64)]>,
    chat_view: DashboardChatView<'_>,
) {
    let Some(pins) = favorites_strip else {
        draw_dashboard_chat_card(frame, area, chat_view);
        return;
    };
    // Need enough vertical room for the strip + a useful chat card; if the
    // dashboard is squeezed below that, drop the strip rather than squash chat.
    if area.height < 6 {
        draw_dashboard_chat_card(frame, area, chat_view);
        return;
    }
    let split = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(area);
    draw_favorites_strip(frame, split[0], pins);
    draw_dashboard_chat_card(frame, split[1], chat_view);
}

fn favorites_strip_area(
    area: Rect,
    show_header: bool,
    pins: &[(uuid::Uuid, String, bool, i64)],
) -> Option<Rect> {
    if pins.len() < 2 {
        return None;
    }

    let chat_area = if show_header {
        if area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH || area.height < DASHBOARD_MIN_FULL_HEIGHT {
            area
        } else {
            Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area)[1]
        }
    } else {
        area
    };

    if chat_area.height < 6 {
        return None;
    }

    Some(Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(chat_area)[0])
}

fn draw_favorites_strip(frame: &mut Frame, area: Rect, pins: &[(uuid::Uuid, String, bool, i64)]) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (idx, (_, label, active, unread)) in pins.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        // Slot hint doubles as the `g<digit>` target; only 1..9 are reachable
        // via the prefix, so pins beyond nine render without a number.
        let slot = if idx < 9 {
            format!("{}:", idx + 1)
        } else {
            String::new()
        };
        let style = if *active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        let unread_suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        spans.push(Span::styled(
            format!(" {slot}{label}{unread_suffix} "),
            style,
        ));
    }
    spans.push(Span::styled(
        "   [] cycle · , last · g_ jump",
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub struct StreamCardProps<'a> {
    pub now_playing: &'a str,
    pub current_genre: Genre,
    pub leading_genre: Genre,
    pub next_switch_in: Duration,
}

fn draw_stream_card(frame: &mut Frame, area: Rect, props: &StreamCardProps<'_>) {
    let block = Block::default()
        .title(" Stream ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Playing: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(props.now_playing, Style::default().fg(theme::TEXT_BRIGHT())),
        ]),
        Line::from(vec![
            Span::styled("Vibe: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                genre_label(props.current_genre),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Next: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                genre_label(props.leading_genre),
                Style::default().fg(theme::AMBER_DIM()),
            ),
            Span::styled("  Switch in ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format_duration_mmss(props.next_switch_in),
                Style::default().fg(theme::TEXT()),
            ),
        ]),
        Line::from(vec![Span::styled(
            "No audio? Type /music in chat for setup instructions",
            Style::default().fg(theme::TEXT_DIM()),
        )]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chat::ui::ChatRowsCache;
    use late_core::models::leaderboard::BadgeTier;
    use ratatui::{Terminal, backend::TestBackend};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn render_dashboard(width: u16) -> Vec<String> {
        render_dashboard_with_size(width, 20)
    }

    fn render_dashboard_with_size(width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let mut pinned_rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, width, height);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: None,
                        chat_view: DashboardChatView {
                            messages: &[],
                            pinned_messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            pinned_rows_cache: &mut pinned_rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                        },
                    },
                );
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut out = String::new();
                for x in 0..width {
                    out.push_str(buffer[(x, y)].symbol());
                }
                out
            })
            .collect()
    }

    #[test]
    fn dashboard_hides_vote_card_at_77_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH);
        assert!(!lines.join("\n").contains("Vote Next"));
        assert_eq!(lines[0].chars().filter(|c| *c == '┌').count(), 1);
    }

    #[test]
    fn dashboard_keeps_vote_card_above_77_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH + 1);
        assert!(lines.join("\n").contains("Vote Next"));
        assert_eq!(lines[0].chars().filter(|c| *c == '┌').count(), 2);
    }

    #[test]
    fn dashboard_still_renders_at_77_column_terminal_content_width() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH);
        assert!(!lines.join("\n").contains("Dashboard view too small."));
        assert!(lines.join("\n").contains("Stream"));
        assert!(lines.join("\n").contains("No messages yet."));
    }

    #[test]
    fn dashboard_hides_top_stream_card_at_65_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_STREAM_AT_WIDTH);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Dashboard view too small."));
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_collapses_to_chat_when_height_below_minimum() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT - 1);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_keeps_full_layout_at_minimum_height() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Stream"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_hides_stream_and_vote_when_header_setting_disabled() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let mut pinned_rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 100, 20);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: false,
                        favorites_strip: None,
                        chat_view: DashboardChatView {
                            messages: &[],
                            pinned_messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            pinned_rows_cache: &mut pinned_rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                        },
                    },
                );
            })
            .expect("draw");

        let rendered = (0..20)
            .map(|y| {
                let mut out = String::new();
                for x in 0..100 {
                    out.push_str(terminal.backend().buffer()[(x, y)].symbol());
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_favorites_strip_renders_unread_counts() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let mut pinned_rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rust_room = Uuid::now_v7();
        let go_room = Uuid::now_v7();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 100, 20);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: Some(&[
                            (rust_room, "#rust".to_string(), true, 3),
                            (go_room, "#go".to_string(), false, 0),
                        ]),
                        chat_view: DashboardChatView {
                            messages: &[],
                            pinned_messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            pinned_rows_cache: &mut pinned_rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                        },
                    },
                );
            })
            .expect("draw");

        let rendered = (0..20)
            .map(|y| {
                let mut out = String::new();
                for x in 0..100 {
                    out.push_str(terminal.backend().buffer()[(x, y)].symbol());
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("#rust (3)"));
        assert!(rendered.contains("#go"));
    }

    #[test]
    fn favorites_strip_hit_test_returns_clicked_room() {
        let rust_room = Uuid::now_v7();
        let go_room = Uuid::now_v7();
        let pins = vec![
            (rust_room, "#rust".to_string(), true, 3),
            (go_room, "#go".to_string(), false, 0),
        ];
        let area = Rect::new(1, 1, 74, 30);

        assert_eq!(
            favorites_strip_hit_test(area, true, &pins, 10, 6),
            Some(rust_room)
        );
        assert_eq!(
            favorites_strip_hit_test(area, true, &pins, 18, 6),
            Some(go_room)
        );
        assert_eq!(favorites_strip_hit_test(area, true, &pins, 40, 6), None);
    }

    #[test]
    fn favorites_strip_hit_test_returns_none_when_strip_hidden() {
        let room = Uuid::now_v7();
        let pins = vec![(room, "#rust".to_string(), true, 0)];

        assert_eq!(
            favorites_strip_hit_test(Rect::new(1, 1, 74, 30), true, &pins, 5, 7),
            None
        );
        assert_eq!(
            favorites_strip_hit_test(
                Rect::new(1, 1, 74, 5),
                false,
                &[
                    (room, "#rust".to_string(), true, 0),
                    (Uuid::now_v7(), "#go".to_string(), false, 0)
                ],
                5,
                1
            ),
            None
        );
    }
}
