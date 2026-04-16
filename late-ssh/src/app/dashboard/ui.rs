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

pub struct DashboardRenderInput<'a> {
    pub now_playing: Option<&'a str>,
    pub vote_counts: &'a VoteCount,
    pub current_genre: Genre,
    pub next_switch_in: Duration,
    pub my_vote: Option<Genre>,
    pub chat_view: DashboardChatView<'a>,
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    if area.width < 52 || area.height < 16 {
        let compact = Paragraph::new("Dashboard view too small.")
            .style(Style::default().fg(theme::TEXT_DIM()))
            .centered();
        frame.render_widget(compact, area);
        return;
    }

    let sections = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);

    let top = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0]);
    let stream_props = StreamCardProps {
        now_playing: view.now_playing.unwrap_or("Waiting for stream..."),
        current_genre: view.current_genre,
        leading_genre: view.vote_counts.winner_or(view.current_genre),
        next_switch_in: view.next_switch_in,
    };
    draw_stream_card(frame, top[0], &stream_props);
    draw_vote_card(
        frame,
        top[1],
        &VoteCardView {
            vote_counts: view.vote_counts,
            my_vote: view.my_vote,
        },
    );

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
