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
    pub connect_url: &'a str,
    pub now_playing: Option<&'a str>,
    pub vote_counts: &'a VoteCount,
    pub current_genre: Genre,
    pub next_switch_in: Duration,
    pub my_vote: Option<Genre>,
    pub chat_view: DashboardChatView<'a>,
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    const DASHBOARD_MIN_WIDTH: u16 = 24;
    const DASHBOARD_SPLIT_TOP_MIN_WIDTH: u16 = 49;

    if area.width < DASHBOARD_MIN_WIDTH || area.height < 16 {
        let compact = Paragraph::new("Dashboard view too small.")
            .style(Style::default().fg(theme::TEXT_DIM))
            .centered();
        frame.render_widget(compact, area);
        return;
    }

    let sections = Layout::vertical([Constraint::Length(6), Constraint::Fill(1)]).split(area);

    let top = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0]);
    let stream_props = StreamCardProps {
        connect_url: view.connect_url,
        now_playing: view.now_playing.unwrap_or("Waiting for stream..."),
        current_genre: view.current_genre,
        leading_genre: view.vote_counts.winner_or(view.current_genre),
        next_switch_in: view.next_switch_in,
    };
    if area.width >= DASHBOARD_SPLIT_TOP_MIN_WIDTH {
        draw_stream_card(frame, top[0], &stream_props);
        draw_vote_card(
            frame,
            top[1],
            &VoteCardView {
                vote_counts: view.vote_counts,
                my_vote: view.my_vote,
            },
        );
    } else {
        draw_vote_card(
            frame,
            sections[0],
            &VoteCardView {
                vote_counts: view.vote_counts,
                my_vote: view.my_vote,
            },
        );
    }

    draw_dashboard_chat_card(frame, sections[1], view.chat_view);
}

pub struct StreamCardProps<'a> {
    pub connect_url: &'a str,
    pub now_playing: &'a str,
    pub current_genre: Genre,
    pub leading_genre: Genre,
    pub next_switch_in: Duration,
}

fn draw_stream_card(frame: &mut Frame, area: Rect, props: &StreamCardProps<'_>) {
    let block = Block::default()
        .title(" Stream ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines = if inner.width <= 31 {
        compact_stream_lines(inner.width as usize, inner.height as usize)
    } else {
        stream_detail_lines(props)
    };

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn stream_detail_lines<'a>(props: &'a StreamCardProps<'a>) -> Vec<Line<'a>> {
    vec![
        Line::from(vec![
            Span::styled("CLI:     ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(
                "curl -fsSL https://cli.late.sh/install.sh | bash",
                Style::default().fg(theme::AMBER),
            ),
            Span::styled("  (Enter to copy)", Style::default().fg(theme::TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("Browser: ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(
                props.connect_url,
                Style::default()
                    .fg(theme::AMBER)
                    .add_modifier(Modifier::UNDERLINED),
            ),
            Span::styled("  (p to copy)", Style::default().fg(theme::TEXT_DIM)),
        ]),
        Line::from(vec![
            Span::styled("Playing: ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(props.now_playing, Style::default().fg(theme::TEXT_BRIGHT)),
        ]),
        Line::from(vec![
            Span::styled("Vibe: ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(
                genre_label(props.current_genre),
                Style::default()
                    .fg(theme::AMBER)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Next: ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(
                genre_label(props.leading_genre),
                Style::default().fg(theme::AMBER_DIM),
            ),
            Span::styled("  Switch in ", Style::default().fg(theme::TEXT_DIM)),
            Span::styled(
                format_duration_mmss(props.next_switch_in),
                Style::default().fg(theme::TEXT),
            ),
        ]),
    ]
}

fn compact_stream_lines(width: usize, max_rows: usize) -> Vec<Line<'static>> {
    let install_label = if width >= 20 { "Enter" } else { "⏎" };
    let install_gap = 1;
    let install_desc = if width >= 24 {
        "Copy CLI Install Command"
    } else {
        "Copy Install Command"
    };
    let browser_gap = if width >= 20 { 5 } else { 1 };
    let mut lines = wrapped_action_lines(install_label, install_gap, install_desc, width);
    let browser_lines = wrapped_action_lines("P", browser_gap, "Copy Browser Music URL", width);

    if lines.len() + browser_lines.len() < max_rows {
        lines.push(Line::raw(""));
    }
    lines.extend(browser_lines);

    while lines.len() < max_rows {
        lines.push(Line::raw(""));
    }
    lines.truncate(max_rows);
    lines
}

fn wrapped_action_lines(
    key: &str,
    gap: usize,
    description: &str,
    width: usize,
) -> Vec<Line<'static>> {
    let prefix = format!("{key}{}", " ".repeat(gap));
    let indent = " ".repeat(prefix.chars().count());
    let wrapped = wrap_with_indent(description, width, prefix.chars().count());
    let mut lines = Vec::with_capacity(wrapped.len());

    for (index, chunk) in wrapped.into_iter().enumerate() {
        if index == 0 {
            lines.push(Line::from(vec![
                Span::styled(prefix.clone(), Style::default().fg(theme::AMBER)),
                Span::styled(chunk, Style::default().fg(theme::TEXT_DIM)),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("{indent}{chunk}"),
                Style::default().fg(theme::TEXT_DIM),
            )));
        }
    }

    lines
}

fn wrap_with_indent(text: &str, width: usize, indent_width: usize) -> Vec<String> {
    let first_width = width.saturating_sub(indent_width).max(1);
    let next_width = first_width;
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut line_width = first_width;

    for word in text.split_whitespace() {
        let needed = if current.is_empty() {
            word.len()
        } else {
            current.len() + 1 + word.len()
        };
        if needed <= line_width {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
            continue;
        }

        if current.is_empty() {
            lines.push(word.to_string());
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
        }
        line_width = next_width;
    }

    if !current.is_empty() {
        lines.push(current);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}
