use std::collections::VecDeque;

use chrono::Utc;
use late_core::api_types::NowPlaying;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
use crate::app::activity::event::ActivityEvent;
use crate::app::audio::{
    client_state::ClientAudioState,
    svc::{QueueItemView, QueueSnapshot},
    viz::Visualizer,
};
use crate::app::bonsai::state::BonsaiState;
use crate::app::cat::state::CatState;
use crate::app::vote::{svc::Genre, ui::VoteCardView};
use late_core::models::user::AudioSource;

const TIME_HEIGHT: u16 = 1;
const RULE_HEIGHT: u16 = 1;
const VISUALIZER_HEIGHT: u16 = 6;
// Full music stage: volume + youtube block + icecast block (with vote).
const MUSIC_STAGE_HEIGHT: u16 = 17;
// Smallest useful viewport over the music stage before it is hidden entirely.
const MUSIC_STAGE_MIN_VISIBLE_HEIGHT: u16 = 4;
const MUSIC_QUEUE_HEIGHT: u16 = 2;
// Bonsai is kept fixed when shown; spare height now belongs to the music stage.
const BONSAI_MIN_HEIGHT: u16 = 16;
// Cat: 3 art rows + 1 footer row.
const CAT_HEIGHT: u16 = 4;

pub struct SidebarProps<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub visualizer: &'a Visualizer,
    pub now_playing: Option<&'a NowPlaying>,
    pub paired_client: Option<&'a ClientAudioState>,
    pub vote: VoteCardView<'a>,
    pub online_count: usize,
    pub bonsai: &'a BonsaiState,
    pub cat: &'a CatState,
    pub cat_available: bool,
    pub audio_beat: f32,
    pub connect_url: &'a str,
    pub activity: &'a VecDeque<ActivityEvent>,
    pub clock_text: &'a str,
    /// YouTube queue snapshot — drives the music stage's active panel and
    /// peek strip. Fed from the same watch channel as the booth modal.
    pub queue_snapshot: &'a QueueSnapshot,
    /// Count of users whose saved audio source is YouTube. Rendered as the
    /// YouTube block's title-bar tag; connection shape is ignored.
    pub youtube_source_count: usize,
    /// Count of users whose saved audio source is Icecast/default. Rendered
    /// as the Icecast block's title-bar tag.
    pub icecast_source_count: usize,
    /// Per-user paired-browser audio source preference (mirrors
    /// `users.settings.audio_source`, flipped by v+x). When set to
    /// `Icecast` the user has opted out of YouTube even if the global queue
    /// is playing, so the music stage stays on Icecast.
    pub paired_browser_source: AudioSource,
}

pub fn draw_sidebar(frame: &mut Frame, area: Rect, props: &SidebarProps<'_>) {
    draw_sidebar_new_shell(frame, area, props);
}

fn draw_sidebar_new_shell(frame: &mut Frame, area: Rect, props: &SidebarProps<'_>) {
    // Single thin separator on the LEFT edge anchors the rail; sections inside
    // breathe without their own borders. Italic dim labels mark each block.
    // Paint the separator column first so content rendering overdraws nothing.
    paint_vertical_separator(frame, area.x, area.y, area.height);

    // Shrink the working area to skip the separator column + 1 col padding.
    let area = Rect {
        x: area.x + 2,
        y: area.y,
        width: area.width.saturating_sub(2),
        height: area.height,
    };

    // Responsive priority on shrink: visualizer drops first, then the music
    // stage keeps the available height and clips from the bottom. Cat and
    // bonsai are kept until music reaches its minimum useful height; spare
    // rows go to music, not the tree.
    let cost = |section: u16| RULE_HEIGHT + section;
    let h = area.height;
    let show_music = TIME_HEIGHT + cost(MUSIC_STAGE_MIN_VISIBLE_HEIGHT) <= h;
    let show_cat =
        show_music && TIME_HEIGHT + cost(MUSIC_STAGE_MIN_VISIBLE_HEIGHT) + cost(CAT_HEIGHT) <= h;
    let show_bonsai = show_cat
        && TIME_HEIGHT
            + cost(MUSIC_STAGE_MIN_VISIBLE_HEIGHT)
            + cost(CAT_HEIGHT)
            + cost(BONSAI_MIN_HEIGHT)
            <= h;
    let need_full_without_viz = TIME_HEIGHT
        + cost(MUSIC_STAGE_HEIGHT)
        + if show_cat { cost(CAT_HEIGHT) } else { 0 }
        + if show_bonsai {
            cost(BONSAI_MIN_HEIGHT)
        } else {
            0
        };
    let show_visualizer = show_music && need_full_without_viz + cost(VISUALIZER_HEIGHT) <= h;

    let fixed_without_music = TIME_HEIGHT
        + if show_visualizer {
            cost(VISUALIZER_HEIGHT)
        } else {
            0
        }
        + if show_music { RULE_HEIGHT } else { 0 }
        + if show_cat { cost(CAT_HEIGHT) } else { 0 }
        + if show_bonsai {
            cost(BONSAI_MIN_HEIGHT)
        } else {
            0
        };
    let music_height = if show_music {
        h.saturating_sub(fixed_without_music)
    } else {
        0
    };

    // Vertical real estate, top to bottom: time, [visualizer], [music],
    // [cat], [bonsai]. A hidden section takes its rule with it.
    let mut constraints = vec![Constraint::Length(TIME_HEIGHT)];
    if show_visualizer {
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
        constraints.push(Constraint::Length(VISUALIZER_HEIGHT)); // visualizer
    }
    if show_music {
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
        constraints.push(Constraint::Length(music_height)); // music stage viewport
    }
    if show_cat {
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
        constraints.push(Constraint::Length(CAT_HEIGHT)); // cat
    }
    if show_bonsai {
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
        constraints.push(Constraint::Length(BONSAI_MIN_HEIGHT)); // bonsai
    }
    if !show_music {
        constraints.push(Constraint::Fill(1));
    }

    let layout = Layout::vertical(constraints).split(area);

    // Inset content one column from the right so it doesn't kiss the frame.
    let inset = |r: Rect| -> Rect {
        Rect {
            x: r.x,
            y: r.y,
            width: r.width.saturating_sub(1),
            height: r.height,
        }
    };

    let mut i = 0usize;

    // Time: right-aligned in the top row.
    draw_time_top(frame, inset(layout[i]), props.clock_text);
    i += 1;

    if show_visualizer {
        draw_horizontal_rule(frame, inset(layout[i]));
        i += 1;
        // Visualizer: borderless inline render.
        props.visualizer.render_inline(frame, inset(layout[i]));
        i += 1;
    }

    if show_music {
        draw_horizontal_rule(frame, inset(layout[i]));
        i += 1;
        draw_music_stage(
            frame,
            inset(layout[i]),
            props.now_playing,
            props.paired_client,
            &props.vote,
            props.queue_snapshot,
            props.paired_browser_source,
            props.youtube_source_count,
            props.icecast_source_count,
        );
        i += 1;
    }

    if show_cat {
        draw_horizontal_rule(frame, inset(layout[i]));
        i += 1;
        let cat_area = inset(layout[i]);
        i += 1;
        if props.cat_available {
            crate::app::cat::ui::draw_cat_inline(frame, cat_area, props.cat);
        } else {
            draw_cat_locked(frame, cat_area);
        }
    }

    if show_bonsai {
        draw_horizontal_rule(frame, inset(layout[i]));
        i += 1;
        crate::app::bonsai::ui::draw_bonsai_inline(
            frame,
            inset(layout[i]),
            props.bonsai,
            props.audio_beat,
        );
    }
}

fn draw_cat_locked(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let row = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1) / 2,
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "cat locked / c shop",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )))
        .centered(),
        row,
    );
}

/// Top-of-rail time. Centered, `◷` clock glyph in dim amber, optional timezone
/// label dimmed, time digits bold amber. Mirrors the classic sidebar clock.
fn draw_time_top(frame: &mut Frame, area: Rect, clock_text: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut parts = clock_text.rsplitn(2, ' ');
    let time = parts.next().unwrap_or(clock_text);
    let label = parts.next();

    // Native `⊙` (U+2299 circled dot operator). Reliably mono across terminals,
    // reads as a small clock face without competing with the digits.
    let mut spans: Vec<Span<'static>> =
        vec![Span::styled("⊙ ", Style::default().fg(theme::AMBER_DIM()))];
    spans.push(Span::styled(
        time.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(label) = label {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)).centered(), area);
}

fn draw_horizontal_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let line = Line::from(Span::styled(
        "─".repeat(area.width as usize),
        Style::default().fg(theme::BORDER_DIM()),
    ));
    frame.render_widget(Paragraph::new(line), area);
}

/// Music stage. Both surfaces (YouTube + Icecast) render together with a
/// dedicated volume row on top. The active source (what the user is
/// actually hearing) gets bold amber chrome; the other gets dim italic.
/// The `▌` accent bar carries the active signal, content widgets keep
/// their own coloring so the data stays legible on both sides.
#[allow(clippy::too_many_arguments)]
fn draw_music_stage(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
    vote: &VoteCardView<'_>,
    queue: &QueueSnapshot,
    paired_browser_source: AudioSource,
    youtube_source_count: usize,
    icecast_source_count: usize,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    // Active source follows the saved preference alone, not whether the
    // browser is currently paired. Saved pref is the source of truth — the
    // sidebar should reflect it from the first frame, before the browser
    // has finished pairing.
    let yt_active = paired_browser_source == AudioSource::Youtube;

    let lines = music_stage_lines(
        area.width,
        now_playing,
        paired_client,
        vote,
        queue,
        yt_active,
        youtube_source_count,
        icecast_source_count,
    );

    frame.render_widget(Paragraph::new(lines), area);
}

#[allow(clippy::too_many_arguments)]
fn music_stage_lines(
    width: u16,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
    vote: &VoteCardView<'_>,
    queue: &QueueSnapshot,
    yt_active: bool,
    youtube_source_count: usize,
    icecast_source_count: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(MUSIC_STAGE_HEIGHT as usize);
    lines.push(volume_row_line(paired_client));
    lines.push(keybind_row_line(width, &[("m", "mute"), ("-=", "vol")]));
    lines.extend(youtube_block_lines(
        width,
        queue,
        yt_active,
        youtube_source_count,
    ));
    lines.push(keybind_row_line(
        width,
        &[("v+v", "queue"), ("v+x", "swap")],
    ));
    lines.extend(icecast_block_lines(
        width,
        vote,
        icecast_source_count,
        now_playing,
        !yt_active,
    ));
    lines
}

fn volume_row_line(paired_client: Option<&ClientAudioState>) -> Line<'static> {
    let mut spans = vec![Span::styled(
        "vol  ",
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    )];
    match paired_client {
        None => {
            spans.push(Span::styled("—", Style::default().fg(theme::TEXT_FAINT())));
        }
        Some(state) if state.muted => {
            spans.push(Span::styled(
                "muted",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ));
        }
        Some(state) => {
            let pct = state.volume_percent.min(100) as usize;
            let filled = ((pct + 5) / 10).min(10);
            let bar_full: String = "▰".repeat(filled);
            let bar_empty: String = "▱".repeat(10 - filled);
            spans.push(Span::styled(bar_full, Style::default().fg(theme::AMBER())));
            spans.push(Span::styled(
                bar_empty,
                Style::default().fg(theme::BORDER_DIM()),
            ));
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("{pct}%"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    }
    Line::from(spans)
}

fn keybind_row_line(width: u16, groups: &[(&str, &str)]) -> Line<'static> {
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default()
        .fg(theme::TEXT_FAINT())
        .add_modifier(Modifier::ITALIC);
    let sep_style = Style::default().fg(theme::BORDER_DIM());

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for (i, (key, label)) in groups.iter().enumerate() {
        let sep = if i == 0 { "" } else { "  " };
        let group_w = sep.chars().count() + key.chars().count() + 1 + label.chars().count();
        if used + group_w > width as usize {
            break;
        }
        if !sep.is_empty() {
            spans.push(Span::styled(sep.to_string(), sep_style));
        }
        spans.push(Span::styled(key.to_string(), key_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label.to_string(), label_style));
        used += group_w;
    }

    Line::from(spans)
}

fn youtube_block_lines(
    width: u16,
    queue: &QueueSnapshot,
    active: bool,
    source_count: usize,
) -> Vec<Line<'static>> {
    let width = width as usize;
    let mut lines = Vec::with_capacity(5 + MUSIC_QUEUE_HEIGHT as usize);
    let tag_string = source_count.to_string();
    lines.push(stage_title_line(
        width as u16,
        "youtube",
        Some(&tag_string),
        active,
    ));

    let title_style = if active {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let meta_style = Style::default().fg(if active {
        theme::TEXT_DIM()
    } else {
        theme::TEXT_FAINT()
    });

    if let Some(current) = &queue.current {
        let title = current
            .title
            .clone()
            .unwrap_or_else(|| format!("yt:{}", current.video_id));
        let track_line = match current.channel.as_deref() {
            Some(channel) if !channel.trim().is_empty() => {
                format!("{} - {}", channel.trim(), title)
            }
            _ if !current.submitter.is_empty() => format!("by {} - {}", current.submitter, title),
            _ => title,
        };
        lines.push(Line::from(Span::styled(
            truncate_chars(&track_line, width),
            title_style,
        )));

        let elapsed_secs = current
            .started_at_ms
            .map(|started| {
                let now_ms = chrono::Utc::now().timestamp_millis();
                ((now_ms.saturating_sub(started)).max(0) / 1000) as u64
            })
            .unwrap_or(0);
        if let Some(duration_ms) = current.duration_ms
            && duration_ms > 0
            && !current.is_stream
        {
            lines.push(progress_line(
                width as u16,
                elapsed_secs,
                (duration_ms as u64) / 1000,
            ));
        } else {
            lines.push(elapsed_line(elapsed_secs));
        }

        if let Some(progress) = &queue.skip_progress {
            lines.push(Line::from(skip_meter_spans(progress)));
        } else {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "next ⌄",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));

        if queue.queue.is_empty() {
            lines.push(Line::from(Span::styled(
                "· fallback next",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
            pad_blank_lines(&mut lines, MUSIC_QUEUE_HEIGHT.saturating_sub(1));
        } else {
            for (idx, item) in queue
                .queue
                .iter()
                .take(MUSIC_QUEUE_HEIGHT as usize)
                .enumerate()
            {
                lines.push(queue_next_line(idx, item, width));
            }
            pad_blank_lines(
                &mut lines,
                MUSIC_QUEUE_HEIGHT
                    .saturating_sub(queue.queue.len().min(MUSIC_QUEUE_HEIGHT as usize) as u16),
            );
        }
    } else {
        lines.push(Line::from(Span::styled("fallback stream", title_style)));
        lines.push(Line::from(Span::styled("YouTube · 24/7", meta_style)));
        lines.push(Line::from(vec![
            Span::styled(
                "queue with  ",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::styled(
                "v+v",
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));
        pad_blank_lines(&mut lines, MUSIC_QUEUE_HEIGHT);
    }

    lines
}

fn icecast_block_lines(
    width: u16,
    vote: &VoteCardView<'_>,
    source_count: usize,
    now_playing: Option<&NowPlaying>,
    active: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(7);
    let tag_string = source_count.to_string();
    lines.push(stage_title_line(
        width,
        "icecast",
        Some(&tag_string),
        active,
    ));

    let title_style = if active {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let meta_style = Style::default().fg(if active {
        theme::TEXT_DIM()
    } else {
        theme::TEXT_FAINT()
    });
    let width_usize = width as usize;

    if let Some(now) = now_playing {
        let track_line = match now.track.artist.as_deref() {
            Some(artist) if !artist.trim().is_empty() => {
                format!("{} - {}", artist.trim(), now.track.title)
            }
            _ => now.track.title.clone(),
        };
        lines.push(Line::from(Span::styled(
            truncate_chars(&track_line, width_usize),
            title_style,
        )));

        let elapsed_secs = now.started_at.elapsed().as_secs();
        match now.track.duration_seconds {
            Some(duration) if duration > 0 => {
                lines.push(progress_line(width, elapsed_secs, duration));
            }
            _ => lines.push(elapsed_line(elapsed_secs)),
        }
    } else {
        lines.push(Line::from(Span::styled("no signal", meta_style)));
        lines.push(Line::from(""));
    }

    let next_genre = vote.vote_counts.winner_or(vote.current_genre);
    let ends = compact_vote_duration(vote.ends_in);

    let next_style = if active {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };

    lines.push(genre_status_line(
        width,
        vote.current_genre,
        next_genre,
        &ends,
        title_style,
        next_style,
    ));
    lines.extend(vote_inline_lines(width, vote));
    lines
}

fn genre_status_line(
    width: u16,
    current: Genre,
    next: Genre,
    ends: &str,
    current_style: Style,
    next_style: Style,
) -> Line<'static> {
    let current_label = genre_label_lower(current);
    let next_label = genre_label_lower(next);
    let current_short = genre_label_short(current);
    let next_short = genre_label_short(next);

    let candidates: Vec<(&str, &str, &str, &str)> = if current == next {
        vec![
            (&current_label, "", "", " "),
            (current_short, "", "", " "),
            ("", "", "", ""),
        ]
    } else {
        vec![
            (&current_label, " → ", &next_label, " · "),
            (current_short, " → ", next_short, " · "),
            ("", "", "", ""),
        ]
    };

    let (current_text, arrow, next_text, time_sep) = candidates
        .iter()
        .copied()
        .find(|(current_text, arrow, next_text, time_sep)| {
            current_text.chars().count()
                + arrow.chars().count()
                + next_text.chars().count()
                + time_sep.chars().count()
                + ends.chars().count()
                <= width as usize
        })
        .unwrap_or_else(|| candidates[candidates.len() - 1]);

    let ends_text = if current_text.is_empty() && arrow.is_empty() {
        truncate_chars(ends, width as usize)
    } else {
        ends.to_string()
    };

    let mut spans = vec![Span::styled(current_text.to_string(), current_style)];
    if !arrow.is_empty() {
        spans.push(Span::styled(
            arrow.to_string(),
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(next_text.to_string(), next_style));
    }
    spans.push(Span::styled(
        time_sep.to_string(),
        Style::default().fg(theme::BORDER_DIM()),
    ));
    spans.push(Span::styled(
        ends_text,
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    Line::from(spans)
}

fn genre_label_lower(genre: Genre) -> String {
    crate::app::common::primitives::genre_label(genre).to_ascii_lowercase()
}

fn genre_label_short(genre: Genre) -> &'static str {
    match genre {
        Genre::Lofi => "lofi",
        Genre::Ambient => "amb",
        Genre::Classic => "cls",
        Genre::Jazz => "jazz",
    }
}

fn vote_inline_lines(width: u16, view: &VoteCardView<'_>) -> Vec<Line<'static>> {
    let options = [
        (
            "v1",
            "lofi",
            &view.vote_counts.lofi,
            view.my_vote == Some(Genre::Lofi),
        ),
        (
            "v2",
            "ambient",
            &view.vote_counts.ambient,
            view.my_vote == Some(Genre::Ambient),
        ),
        (
            "v3",
            "classic",
            &view.vote_counts.classic,
            view.my_vote == Some(Genre::Classic),
        ),
    ];
    let total = view.vote_counts.total().max(1) as usize;
    let max_bar = (width as usize).saturating_sub(14).max(1);

    options
        .iter()
        .map(|(key, name, votes, mine)| {
            let filled = (**votes as usize * max_bar) / total;
            let empty = max_bar.saturating_sub(filled);

            let name_color = if *mine {
                theme::SUCCESS()
            } else {
                theme::TEXT()
            };
            let bar_color = if *mine {
                theme::SUCCESS()
            } else {
                theme::AMBER_DIM()
            };

            Line::from(vec![
                Span::styled(format!("{:<8}", name), Style::default().fg(name_color)),
                Span::styled("●".repeat(filled), Style::default().fg(bar_color)),
                Span::styled("○".repeat(empty), Style::default().fg(theme::BORDER_DIM())),
                Span::styled(
                    format!(" {:>2}", votes),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
                Span::raw(" "),
                Span::styled(
                    key.to_string(),
                    Style::default()
                        .fg(theme::AMBER_DIM())
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        })
        .collect()
}

fn progress_line(width: u16, elapsed_secs: u64, duration_secs: u64) -> Line<'static> {
    if width == 0 || duration_secs == 0 {
        return Line::from("");
    }
    let elapsed = elapsed_secs.min(duration_secs);
    let elapsed_str = format!("{}:{:02}", elapsed / 60, elapsed % 60);
    let total_str = format!("{}:{:02}", duration_secs / 60, duration_secs % 60);
    let time_w = elapsed_str.len() + total_str.len() + 2;
    let bar_w = (width as usize).saturating_sub(time_w);
    if bar_w == 0 {
        return Line::from(Span::styled(
            elapsed_str,
            Style::default().fg(theme::AMBER()),
        ));
    }

    let progress = (elapsed as f64 / duration_secs as f64).clamp(0.0, 1.0);
    let dot = ((bar_w as f64 * progress) as usize).min(bar_w.saturating_sub(1));
    let bar_before = "─".repeat(dot);
    let bar_after = "─".repeat(bar_w.saturating_sub(dot + 1));
    Line::from(vec![
        Span::styled(elapsed_str, Style::default().fg(theme::AMBER())),
        Span::raw(" "),
        Span::styled(bar_before, Style::default().fg(theme::BORDER_DIM())),
        Span::styled("●", Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(bar_after, Style::default().fg(theme::BORDER_DIM())),
        Span::raw(" "),
        Span::styled(total_str, Style::default().fg(theme::TEXT_FAINT())),
    ])
}

fn elapsed_line(elapsed_secs: u64) -> Line<'static> {
    let elapsed = format!("{}:{:02}", elapsed_secs / 60, elapsed_secs % 60);
    Line::from(vec![
        Span::styled(elapsed, Style::default().fg(theme::AMBER())),
        Span::styled(" live", Style::default().fg(theme::TEXT_FAINT())),
    ])
}

fn pad_blank_lines(lines: &mut Vec<Line<'static>>, count: u16) {
    for _ in 0..count {
        lines.push(Line::from(""));
    }
}

/// Stage title bar: `▌ LABEL  ───── ▶ tag`. Active: amber accent bar,
/// uppercase amber bold label, amber tag. Inactive: dim bar, lowercase
/// italic faint label, no tag. The trailing rule fills to the right edge.
fn stage_title_line(area_w: u16, label: &str, tag: Option<&str>, active: bool) -> Line<'static> {
    let (bar_style, label_style, tag_style) = if active {
        (
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
            Style::default().fg(theme::AMBER_DIM()),
        )
    } else {
        (
            Style::default().fg(theme::BORDER_DIM()),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )
    };
    // Label is always lowercase — the active state badge is communicated
    // through color/weight + the source-count tag on the right, not case.
    let label_text = label.to_lowercase();

    // Tag has no glyph prefix; color + position already reads as a state
    // badge and the prefix was eating cells on a narrow rail.
    let tag_text = tag.map(|t| t.to_string()).unwrap_or_default();
    let bar_w = 2;
    let pad_w = 2;
    let gap_w = if tag_text.is_empty() { 0 } else { 1 };
    let used = bar_w + label_text.chars().count() + pad_w + gap_w + tag_text.chars().count();
    let dash_count = (area_w as usize).saturating_sub(used).max(1);

    let mut spans = vec![
        Span::styled("▌ ", bar_style),
        Span::styled(label_text, label_style),
        Span::raw("  "),
        Span::styled(
            "─".repeat(dash_count),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ];
    if !tag_text.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(tag_text, tag_style));
    }
    Line::from(spans)
}

/// Skip-vote meter. Caps the dot row at 8 cells so a 20-pair threshold
/// doesn't overflow the rail; the literal `votes/threshold` count below
/// remains authoritative.
fn skip_meter_spans(progress: &super::super::audio::svc::SkipProgress) -> Vec<Span<'static>> {
    const MAX_DOTS: u32 = 8;
    let shown = progress.threshold.clamp(1, MAX_DOTS);
    let votes_shown = progress.votes.min(shown);
    let mut dots = String::with_capacity(shown as usize);
    for i in 0..shown {
        dots.push(if i < votes_shown { '●' } else { '○' });
    }
    vec![
        Span::styled(
            "skip ",
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(dots, Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            format!(" {}/{}", progress.votes, progress.threshold),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::raw(" "),
        Span::styled(
            "v+s",
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ]
}

/// One entry in the YouTube "next" list. Number, title, then a dim score
/// right-aligned: `+N` (positive), `-N` (negative), `·` (zero).
fn queue_next_line(idx: usize, item: &QueueItemView, width: usize) -> Line<'static> {
    let n_text = format!("{}  ", idx + 1);
    let title = item
        .title
        .clone()
        .unwrap_or_else(|| format!("yt:{}", item.video_id));

    let (score_text, score_style) = if item.vote_score > 0 {
        (
            format!("+{}", item.vote_score),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        )
    } else if item.vote_score < 0 {
        (
            item.vote_score.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )
    } else {
        ("·".to_string(), Style::default().fg(theme::TEXT_FAINT()))
    };

    let prefix_w = n_text.chars().count();
    let score_w = score_text.chars().count();
    let title_budget = width.saturating_sub(prefix_w + score_w + 2);
    let title_text = truncate_chars(&title, title_budget);
    let pad = title_budget.saturating_sub(title_text.chars().count());

    Line::from(vec![
        Span::styled(n_text, Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(title_text, Style::default().fg(theme::TEXT())),
        Span::raw(" ".repeat(pad + 2)),
        Span::styled(score_text, score_style),
    ])
}

fn compact_vote_duration(duration: std::time::Duration) -> String {
    let secs = duration.as_secs();
    if secs == 0 {
        return "now".to_string();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let minutes = secs.div_ceil(60);
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    let mins = minutes % 60;
    if mins == 0 {
        format!("{hours}h")
    } else {
        format!("{hours}h{mins:02}")
    }
}

/// Paint a thin vertical line (1 column wide) in BORDER_DIM. Used by the
/// merged shell to anchor left/right rails without wrapping them in a box.
pub fn paint_vertical_separator(frame: &mut Frame, x: u16, y: u16, height: u16) {
    let buf = frame.buffer_mut();
    for dy in 0..height {
        if let Some(cell) = buf.cell_mut((x, y + dy)) {
            cell.set_symbol("│").set_fg(theme::BORDER_DIM());
        }
    }
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
    use std::time::Duration;

    #[test]
    fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
        let clock = sidebar_clock_text(None);
        assert!(clock.starts_with("UTC "));
    }

    #[test]
    fn compact_vote_duration_rounds_remaining_minutes_up() {
        assert_eq!(compact_vote_duration(Duration::from_secs(0)), "now");
        assert_eq!(compact_vote_duration(Duration::from_secs(42)), "42s");
        assert_eq!(compact_vote_duration(Duration::from_secs(61)), "2m");
        assert_eq!(compact_vote_duration(Duration::from_secs(3600)), "1h");
        assert_eq!(compact_vote_duration(Duration::from_secs(3661)), "1h02");
    }

    #[test]
    fn genre_status_line_compacts_long_different_genres() {
        let line = genre_status_line(
            15,
            Genre::Classic,
            Genre::Ambient,
            "20m",
            Style::default(),
            Style::default(),
        );

        assert_eq!(line_text(&line), "cls → amb · 20m");
    }

    #[test]
    fn genre_status_line_compacts_repeated_genres() {
        let line = genre_status_line(
            13,
            Genre::Ambient,
            Genre::Ambient,
            "20m",
            Style::default(),
            Style::default(),
        );

        assert_eq!(line_text(&line), "ambient 20m");
    }

    #[test]
    fn genre_status_line_falls_back_to_time_when_very_narrow() {
        let line = genre_status_line(
            14,
            Genre::Classic,
            Genre::Ambient,
            "20m",
            Style::default(),
            Style::default(),
        );

        assert_eq!(line_text(&line), "20m");
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }
}
