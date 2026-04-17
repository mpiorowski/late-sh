use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

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
    pub chat_view: DashboardChatView<'a>,
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    let stream_props = StreamCardProps {
        now_playing: view.now_playing.unwrap_or("Waiting for stream..."),
        current_genre: view.current_genre,
        leading_genre: view.vote_counts.winner_or(view.current_genre),
        next_switch_in: view.next_switch_in,
    };
    if area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH || area.height < DASHBOARD_MIN_FULL_HEIGHT {
        draw_dashboard_chat_card(frame, area, view.chat_view);
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

    draw_dashboard_chat_card(frame, sections[1], view.chat_view);
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
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();

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
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            badges: &badges,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            composer: "",
                            composer_rows: &[],
                            composer_cursor: 0,
                            composing: false,
                            cursor_visible: false,
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
        assert!(lines.join("\n").contains("Chat"));
    }

    #[test]
    fn dashboard_hides_top_stream_card_at_65_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_STREAM_AT_WIDTH);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Dashboard view too small."));
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("Chat"));
    }

    #[test]
    fn dashboard_collapses_to_chat_when_height_below_minimum() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT - 1);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("Chat"));
    }

    #[test]
    fn dashboard_keeps_full_layout_at_minimum_height() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Stream"));
        assert!(rendered.contains("Chat"));
    }
}
