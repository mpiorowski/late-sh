use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    audio::svc::{QueueItemView, QueueSnapshot, SkipProgress},
    common::theme,
};

use super::state::{BoothFocus, BoothModalState};

const MODAL_WIDTH: u16 = 90;
const MODAL_HEIGHT: u16 = 30;

pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &BoothModalState,
    snapshot: &QueueSnapshot,
    submit_enabled: bool,
) {
    let popup = centered_rect(
        area,
        MODAL_WIDTH.min(area.width),
        MODAL_HEIGHT.min(area.height),
    );
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Music Booth ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 14 || inner.width < 40 {
        frame.render_widget(Paragraph::new("Terminal too small"), inner);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing
        Constraint::Length(1), // Submit heading
        Constraint::Length(1), // breathing
        Constraint::Length(1), // submit row
        Constraint::Length(1), // breathing
        Constraint::Length(1), // Now Playing heading
        Constraint::Length(1), // breathing
        Constraint::Length(1), // now playing line
        Constraint::Length(1), // breathing
        Constraint::Length(1), // Queue heading
        Constraint::Length(1), // breathing
        Constraint::Min(4),    // queue list
        Constraint::Length(1), // footer
    ])
    .split(inner);

    let width = inner.width as usize;

    frame.render_widget(Paragraph::new(section_heading("Submit")), layout[1]);
    draw_submit(frame, layout[3], state, submit_enabled, width);

    frame.render_widget(Paragraph::new(section_heading("Now Playing")), layout[5]);
    draw_current(
        frame,
        layout[7],
        snapshot.current.as_ref(),
        snapshot.skip_progress(),
    );

    frame.render_widget(Paragraph::new(section_heading("Queue")), layout[9]);
    draw_queue(frame, layout[11], state, &snapshot.queue);

    draw_footer(frame, layout[12], submit_enabled);
}

fn draw_submit(
    frame: &mut Frame,
    area: Rect,
    state: &BoothModalState,
    enabled: bool,
    width: usize,
) {
    let focused = state.focus() == BoothFocus::Submit && enabled;
    let marker = if focused { "›" } else { " " };

    let prefix_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if focused {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let trailing_style = if focused {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let label = if enabled { "YouTube URL" } else { "Disabled" };
    let prefix = format!(" {marker} ");
    let label_text = format!("{label:<16}");

    let (value_text, value_color) = if !enabled {
        (
            "server YouTube key is unset - staff /audio still works".to_string(),
            theme::TEXT_DIM(),
        )
    } else {
        let typed = state.submit_input().to_string();
        if focused {
            let mut text = typed;
            text.push('█');
            (text, theme::AMBER())
        } else if typed.is_empty() {
            ("paste link…".to_string(), theme::TEXT_FAINT())
        } else {
            (typed, theme::TEXT_BRIGHT())
        }
    };
    let value_style = if focused {
        Style::default().fg(value_color).bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(value_color)
    };

    let used = prefix.chars().count() + label_text.chars().count() + value_text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(prefix, prefix_style),
            Span::styled(label_text, label_style),
            Span::styled(value_text, value_style),
            Span::styled(" ".repeat(padding), trailing_style),
        ])),
        area,
    );
}

fn draw_current(
    frame: &mut Frame,
    area: Rect,
    current: Option<&QueueItemView>,
    skip: Option<SkipProgress>,
) {
    let Some(item) = current else {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    "queue empty - falling back to Icecast",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            area,
        );
        return;
    };

    let label = item
        .title
        .clone()
        .unwrap_or_else(|| format!("yt:{}", item.video_id));
    let mut spans = vec![
        Span::raw("   "),
        Span::styled("▶ ", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            label,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if !item.submitter.is_empty() {
        spans.push(Span::styled(
            format!("  by {}", item.submitter),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if let Some(progress) = skip {
        spans.push(Span::styled(
            format!("   skip {}/{}", progress.votes, progress.threshold),
            Style::default().fg(theme::AMBER_DIM()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_queue(
    frame: &mut Frame,
    area: Rect,
    state: &BoothModalState,
    queue: &[QueueItemView],
) {
    if queue.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("   "),
                Span::styled("queue empty", Style::default().fg(theme::TEXT_DIM())),
            ])),
            area,
        );
        return;
    }

    let selected = state.selected().min(queue.len().saturating_sub(1));
    let focused = state.focus() == BoothFocus::Queue;
    let height = area.height as usize;
    if height == 0 {
        return;
    }
    let width = area.width as usize;
    let start = selected
        .saturating_sub(height.saturating_sub(1))
        .min(queue.len().saturating_sub(height.min(queue.len())));

    let lines: Vec<Line<'static>> = queue
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(index, item)| {
            let active = focused && index == selected;
            queue_line(item, active, width)
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), area);
}

fn queue_line(item: &QueueItemView, active: bool, width: usize) -> Line<'static> {
    let marker = if active { "›" } else { " " };
    let prefix_style = if active {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if active {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let meta_style = if active {
        Style::default()
            .fg(theme::TEXT_DIM())
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let trailing_style = if active {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };
    let score = format!("{:+}", item.vote_score);
    let score_style = if item.vote_score > 0 {
        let base = Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD);
        if active { base.bg(theme::BG_SELECTION()) } else { base }
    } else if item.vote_score < 0 {
        let base = Style::default().fg(theme::TEXT_DIM());
        if active { base.bg(theme::BG_SELECTION()) } else { base }
    } else {
        meta_style
    };

    let label = item
        .title
        .clone()
        .unwrap_or_else(|| format!("yt:{}", item.video_id));
    let prefix = format!(" {marker} ");
    let prefix_w = prefix.chars().count();
    let score_width = 5usize.min(width.saturating_sub(prefix_w + 4));
    let submitter_width = 20usize.min(width.saturating_sub(prefix_w + score_width + 4));
    let label_width = width.saturating_sub(prefix_w + submitter_width + score_width + 2);

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(
            pad_right(&truncate_to_width(&label, label_width), label_width),
            label_style,
        ),
        Span::styled(" ", trailing_style),
        Span::styled(
            pad_right(
                &truncate_to_width(&item.submitter, submitter_width),
                submitter_width,
            ),
            meta_style,
        ),
        Span::styled(" ", trailing_style),
        Span::styled(
            pad_left(&truncate_to_width(&score, score_width), score_width),
            score_style,
        ),
    ])
}

fn draw_footer(frame: &mut Frame, area: Rect, submit_enabled: bool) {
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" focus  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ Ctrl+J/K", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("+/-", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" vote  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("0", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" clear  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("s", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" skip  ", Style::default().fg(theme::TEXT_DIM())),
    ];
    if submit_enabled {
        spans.push(Span::styled(
            "↵",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " submit  ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::styled(
        "Esc/q",
        Style::default().fg(theme::AMBER_DIM()),
    ));
    spans.push(Span::styled(
        " close",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
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
