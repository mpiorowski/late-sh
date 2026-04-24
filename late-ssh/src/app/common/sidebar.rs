use std::collections::VecDeque;

use chrono::Utc;
use late_core::api_types::NowPlaying;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::theme;
use crate::app::bonsai::state::BonsaiState;
use crate::app::visualizer::Visualizer;
use crate::session::ClientAudioState;
use crate::state::ActivityEvent;

pub struct SidebarProps<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub visualizer: &'a Visualizer,
    pub now_playing: Option<&'a NowPlaying>,
    pub paired_client: Option<&'a ClientAudioState>,
    pub online_count: usize,
    pub bonsai: &'a BonsaiState,
    pub audio_beat: f32,
    pub connect_url: &'a str,
    pub activity: &'a VecDeque<ActivityEvent>,
    pub clock_text: &'a str,
}

pub fn draw_sidebar(frame: &mut Frame, area: Rect, props: &SidebarProps<'_>) {
    let visualizer = props.visualizer;
    let now_playing = props.now_playing;
    let paired_client = props.paired_client;
    let online_count = props.online_count;
    let layout = Layout::vertical([
        Constraint::Length(3),  // clock
        Constraint::Length(10), // visualizer
        Constraint::Length(7),  // now playing
        Constraint::Fill(1),    // activity (shrinks on small screens)
        Constraint::Length(16), // bonsai tree (12 max art + 2 status + 2 border)
    ])
    .split(area);

    draw_clock_card(frame, layout[0], props.clock_text);
    visualizer.render(frame, layout[1]);
    draw_now_playing(frame, layout[2], now_playing, paired_client);
    draw_status(frame, layout[3], online_count, props.activity);
    crate::app::bonsai::ui::draw_bonsai(frame, layout[4], props.bonsai, props.audio_beat);
}

fn draw_clock_card(frame: &mut Frame, area: Rect, clock_text: &str) {
    let block = Block::default()
        .title(" Time ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(clock_line(clock_text)).centered(), inner);
}

fn clock_line(clock_text: &str) -> Line<'static> {
    let mut parts = clock_text.rsplitn(2, ' ');
    let time = parts.next().unwrap_or(clock_text);
    let label = parts.next();

    let mut spans = vec![Span::styled("◷ ", Style::default().fg(theme::AMBER_DIM()))];
    if let Some(label) = label {
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        time.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    Line::from(spans)
}

fn draw_now_playing(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
) {
    let block = Block::default()
        .title(" Now Playing ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = match now_playing {
        Some(np) => {
            let artist = np.track.artist.as_deref().unwrap_or("Unknown");
            let title = &np.track.title;
            let elapsed_secs = np.started_at.elapsed().as_secs();
            let duration = np.track.duration_seconds;

            let mut lines = vec![
                Line::from(Span::styled(artist, Style::default().fg(theme::TEXT_DIM()))),
                Line::from(Span::styled(
                    title.as_str(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                )),
            ];

            if let Some(dur) = duration {
                let elapsed = elapsed_secs.min(dur);
                let elapsed_str = format!("{}:{:02}", elapsed / 60, elapsed % 60);
                let total_str = format!("{}:{:02}", dur / 60, dur % 60);

                let time_width = elapsed_str.len() + total_str.len() + 2;
                let bar_width = (inner.width as usize).saturating_sub(time_width);

                let progress = if dur > 0 {
                    (elapsed as f64 / dur as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let dot_pos =
                    ((bar_width as f64 * progress) as usize).min(bar_width.saturating_sub(1));

                let bar_before = "─".repeat(dot_pos);
                let bar_after = "─".repeat(bar_width.saturating_sub(dot_pos + 1));

                lines.push(Line::from(vec![
                    Span::styled(elapsed_str, Style::default().fg(theme::AMBER())),
                    Span::raw(" "),
                    Span::styled(bar_before, Style::default().fg(theme::BORDER_DIM())),
                    Span::styled("●", Style::default().fg(theme::AMBER_GLOW())),
                    Span::styled(bar_after, Style::default().fg(theme::BORDER_DIM())),
                    Span::raw(" "),
                    Span::styled(total_str, Style::default().fg(theme::TEXT_FAINT())),
                ]));
            } else {
                let elapsed_str = format!("{}:{:02}", elapsed_secs / 60, elapsed_secs % 60);
                lines.push(Line::from(vec![
                    Span::styled(elapsed_str, Style::default().fg(theme::AMBER())),
                    Span::styled(" ▸", Style::default().fg(theme::AMBER_GLOW())),
                ]));
            }

            lines.push(Line::from(vec![
                Span::styled("- / =", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" vol  ", Style::default().fg(theme::TEXT_FAINT())),
                Span::styled("m", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(" mute", Style::default().fg(theme::TEXT_FAINT())),
            ]));
            lines.push(paired_client_line(paired_client));

            lines
        }
        None => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "Waiting...",
                    Style::default().fg(theme::TEXT_FAINT()),
                )),
                Line::raw(""),
            ];
            lines.push(paired_client_line(paired_client));
            lines
        }
    };

    frame.render_widget(Paragraph::new(content), inner);
}

fn paired_client_line(paired_client: Option<&ClientAudioState>) -> Line<'static> {
    match paired_client {
        Some(state) => Line::from(vec![
            Span::styled(
                state.client_kind.label(),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                if state.muted { "Muted" } else { "Live" },
                Style::default().fg(if state.muted {
                    theme::AMBER()
                } else {
                    theme::TEXT_BRIGHT()
                }),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("{}%", state.volume_percent),
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]),
        None => Line::from(Span::styled(
            "No pair",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    }
}

fn draw_status(
    frame: &mut Frame,
    area: Rect,
    online_count: usize,
    activity: &VecDeque<ActivityEvent>,
) {
    if area.height < 3 {
        return;
    }

    let block = Block::default()
        .title(" Activity ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let header_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: 1,
    };
    let online_line = Line::from(vec![
        Span::styled("● ", Style::default().fg(theme::SUCCESS())),
        Span::styled(
            format!("{}", online_count),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" online", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(online_line), header_area);

    let events_area = Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    };
    if events_area.height == 0 {
        return;
    }

    let activity_rows = events_area.height.min(20) as usize;
    let visible_events = (activity_rows / 2).max(1);
    let meta_width = events_area.width as usize;
    let action_width = events_area.width as usize;

    let mut lines = Vec::new();
    for event in activity.iter().rev().take(visible_events) {
        let elapsed = event.at.elapsed().as_secs();
        let ago = if elapsed < 60 {
            format!("{}s", elapsed)
        } else {
            format!("{}m", elapsed / 60)
        };

        let meta = truncate_chars(&format!("@{}  {}", event.username, ago), meta_width);
        let action = truncate_chars(&event.action, action_width);

        lines.push(Line::from(vec![Span::styled(
            meta,
            Style::default().fg(theme::TEXT_MUTED()),
        )]));
        lines.push(Line::from(vec![Span::styled(
            action,
            Style::default().fg(theme::TEXT_DIM()),
        )]));
    }

    frame.render_widget(Paragraph::new(lines), events_area);
}

pub fn sidebar_clock_text(timezone: Option<&str>) -> String {
    crate::app::common::time::timezone_current_time(Utc::now(), timezone)
        .unwrap_or_else(|| Utc::now().format("UTC %H:%M").to_string())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
        let clock = sidebar_clock_text(None);
        assert!(clock.starts_with("UTC "));
    }
}
