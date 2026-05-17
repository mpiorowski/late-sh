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
    client_state::{ClientAudioState, ClientKind},
    svc::{AudioMode, QueueItemView, QueueSnapshot},
    viz::Visualizer,
};
use late_core::models::user::AudioSource;
use crate::app::bonsai::state::BonsaiState;
use crate::app::dashboard::ui::DashboardRoomCard;
use crate::app::vote::ui::VoteCardView;

pub struct SidebarProps<'a> {
    pub game_selection: usize,
    pub is_playing_game: bool,
    pub visualizer: &'a Visualizer,
    pub now_playing: Option<&'a NowPlaying>,
    pub paired_client: Option<&'a ClientAudioState>,
    pub vote: VoteCardView<'a>,
    pub online_count: usize,
    pub bonsai: &'a BonsaiState,
    pub audio_beat: f32,
    pub connect_url: &'a str,
    pub activity: &'a VecDeque<ActivityEvent>,
    pub clock_text: &'a str,
    /// Top multiplayer rooms — rendered as a compact "active tables" block
    /// in the right rail.
    pub top_rooms: &'a [DashboardRoomCard],
    /// YouTube queue snapshot — drives the music stage's active panel and
    /// peek strip. Fed from the same watch channel as the booth modal.
    pub queue_snapshot: &'a QueueSnapshot,
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

    const TIME_HEIGHT: u16 = 1;
    const RULE_HEIGHT: u16 = 1;
    const VISUALIZER_HEIGHT: u16 = 6;
    // Music stage: active panel (~11 rows) + peek strip (1 row) + spacer (1).
    const MUSIC_STAGE_HEIGHT: u16 = 13;
    const ACTIVE_TABLES_HEIGHT: u16 = 6;
    const BONSAI_MIN_HEIGHT: u16 = 3;

    let fixed_without_active = TIME_HEIGHT
        + RULE_HEIGHT
        + VISUALIZER_HEIGHT
        + RULE_HEIGHT
        + MUSIC_STAGE_HEIGHT
        + RULE_HEIGHT;
    let active_tables_budget = ACTIVE_TABLES_HEIGHT + RULE_HEIGHT;
    let show_active_tables =
        fixed_without_active + active_tables_budget + BONSAI_MIN_HEIGHT <= area.height;

    // Vertical real estate, top to bottom. Active tables are lower priority
    // than bonsai: hide them before squeezing the tree below its visible size.
    let mut constraints = vec![
        Constraint::Length(TIME_HEIGHT),         // time
        Constraint::Length(RULE_HEIGHT),         // ── rule
        Constraint::Length(VISUALIZER_HEIGHT),   // visualizer
        Constraint::Length(RULE_HEIGHT),         // ── rule
        Constraint::Length(MUSIC_STAGE_HEIGHT),  // active stage + peek strip
        Constraint::Length(RULE_HEIGHT),         // ── rule
    ];
    if show_active_tables {
        constraints.push(Constraint::Length(ACTIVE_TABLES_HEIGHT)); // active tables
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
    }
    constraints.push(Constraint::Fill(1)); // bonsai

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

    // Time: right-aligned in the top row.
    draw_time_top(frame, inset(layout[0]), props.clock_text);
    draw_horizontal_rule(frame, inset(layout[1]));

    // Visualizer: borderless inline render.
    props.visualizer.render_inline(frame, inset(layout[2]));

    draw_horizontal_rule(frame, inset(layout[3]));

    draw_music_stage(
        frame,
        inset(layout[4]),
        props.now_playing,
        props.paired_client,
        &props.vote,
        props.queue_snapshot,
        props.paired_browser_source,
    );

    draw_horizontal_rule(frame, inset(layout[5]));

    let mut bonsai_idx = 6;
    if show_active_tables {
        draw_active_tables(frame, inset(layout[6]), props.top_rooms);
        draw_horizontal_rule(frame, inset(layout[7]));
        bonsai_idx = 8;
    }
    crate::app::bonsai::ui::draw_bonsai_inline(
        frame,
        inset(layout[bonsai_idx]),
        props.bonsai,
        props.audio_beat,
    );
}

/// Compact active tables panel for the right rail. Shows up to 3 busy rooms,
/// 2 rows each: name, then seat dots + timer.
fn draw_active_tables(frame: &mut Frame, area: Rect, rooms: &[DashboardRoomCard]) {
    if area.width == 0 || area.height < 2 {
        return;
    }

    if rooms.is_empty() {
        let chunks = Layout::vertical([
            Constraint::Length(1), // empty-state label
            Constraint::Fill(1),   // hints
        ])
        .split(area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "multiplayer",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            chunks[0],
        );
        draw_empty_active_tables(frame, chunks[1]);
        return;
    }

    let body = area;
    let rows_per_room: u16 = 2;
    let max_rooms = ((body.height / rows_per_room) as usize).min(3);
    let visible_rooms = rooms.iter().take(max_rooms.max(1));

    let mut lines: Vec<Line<'_>> = Vec::new();
    for (idx, card) in visible_rooms.enumerate() {
        let inner_w = body.width as usize;
        let room_hint = active_tables_room_hint(idx);
        let hint_w = room_hint
            .iter()
            .map(|span| span.content.chars().count())
            .sum::<usize>();
        let name_budget = inner_w.saturating_sub(hint_w + 1).max(1);
        let name = truncate_chars(&card.room.display_name, name_budget);
        let mut row = vec![Span::styled(
            name,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )];
        let pad = inner_w.saturating_sub(
            row.iter()
                .map(|span| span.content.chars().count())
                .sum::<usize>()
                + hint_w,
        );
        row.push(Span::raw(" ".repeat(pad)));
        row.extend(room_hint);
        lines.push(Line::from(row));

        lines.push(active_table_status_line(card, body.width as usize));
    }

    frame.render_widget(Paragraph::new(lines), body);
}

fn active_tables_room_hint(idx: usize) -> Vec<Span<'static>> {
    vec![Span::styled(
        format!("b{}", idx + 1),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )]
}

fn draw_empty_active_tables(frame: &mut Frame, area: Rect) {
    let faint = |text: &str| -> Span<'static> {
        Span::styled(
            text.to_string(),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )
    };

    let lines = vec![Line::from(faint("no active tables"))];
    frame.render_widget(Paragraph::new(lines), area);
}

fn active_table_status_line(card: &DashboardRoomCard, width: usize) -> Line<'static> {
    let occupied = card.occupied_seats.unwrap_or(0);
    let total = card.total_seats;
    let dots = seat_dot_spans(occupied, total);
    let dot_width = total.min(6);
    let timer = compact_timer_label(&card.pace);
    let timer_budget = width.saturating_sub(dot_width + 1);
    let timer = truncate_chars(&timer, timer_budget);

    let mut spans = dots;
    if !timer.is_empty() {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(timer, Style::default().fg(theme::TEXT_DIM())));
    }
    Line::from(spans)
}

fn seat_dot_spans(occupied: usize, total: usize) -> Vec<Span<'static>> {
    let visible_total = total.clamp(1, 6);
    let visible_occupied = occupied.min(visible_total);
    let mut spans = Vec::with_capacity(visible_total);
    for idx in 0..visible_total {
        let symbol = if idx < visible_occupied { "●" } else { "○" };
        spans.push(Span::styled(symbol, Style::default().fg(theme::AMBER())));
    }
    spans
}

fn compact_timer_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        return "waiting".to_string();
    }
    label
        .replace(" action timer", " timer")
        .replace('-', " ")
        .to_string()
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

/// Music stage. Renders the *currently producing* audio surface as a full
/// panel and the other surface as a single-line peek strip.
///
/// Stage selector is intentionally simple — it follows the global
/// `audio_mode` flip set by `AudioService`. An empty queue does NOT mean
/// we leave YouTube: `audio_mode = Youtube` with `current = None` is the
/// fallback-stream case and stays on the YouTube stage. The only override
/// is CLI listeners, who literally cannot decode YouTube and will only
/// hear Icecast no matter what the server is producing globally.
///
/// The per-user `paired_browser_source` preference (flipped by v+x) does
/// not move the stage — it informs the stage *tag* (`▶ pinned` when the
/// user opted out of YouTube) and the peek strip's hint text.
fn draw_music_stage(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
    vote: &VoteCardView<'_>,
    queue: &QueueSnapshot,
    paired_browser_source: AudioSource,
) {
    if area.width == 0 || area.height < 4 {
        return;
    }

    let cli_paired =
        matches!(paired_client, Some(c) if c.client_kind == ClientKind::Cli);
    let on_youtube = !cli_paired && queue.audio_mode == AudioMode::Youtube;

    let split = Layout::vertical([
        Constraint::Min(3),    // active stage
        Constraint::Length(1), // peek strip
    ])
    .split(area);

    if on_youtube {
        draw_youtube_stage(frame, split[0], queue, paired_client, paired_browser_source);
        draw_icecast_peek(frame, split[1], now_playing, vote);
    } else {
        draw_icecast_stage(frame, split[0], now_playing, paired_client, vote);
        draw_youtube_peek(
            frame,
            split[1],
            queue,
            paired_client,
            paired_browser_source,
        );
    }
}

/// Stage title bar: `▌ LABEL  ─────── ▶ tag`. The accent bar + amber title
/// reads as "this is the one you're hearing"; the trailing rule fills to
/// the right edge so the band always anchors visually.
fn stage_title_line(area_w: u16, label: &str, mode_tag: &str) -> Line<'static> {
    let mode_text = format!("▶ {mode_tag}");
    let bar_w = 2;
    let pad_w = 2;
    let gap_w = 1;
    let used = bar_w + label.chars().count() + pad_w + gap_w + mode_text.chars().count();
    let dash_count = (area_w as usize).saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled(
            "▌ ",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "─".repeat(dash_count),
            Style::default().fg(theme::BORDER_DIM()),
        ),
        Span::raw(" "),
        Span::styled(mode_text, Style::default().fg(theme::AMBER_DIM())),
    ])
}

/// Active YouTube panel. Either real queue playback (with skip meter +
/// next list) or a fallback-stream placeholder when the queue is empty
/// but a YouTube fallback is configured. Tag reflects what the user is
/// actually hearing: `off` (no pairing), `pinned` (browser paired but
/// user opted out via v+x → hearing Icecast instead), `fallback`
/// (fallback stream playing), or `live` (queue track playing).
fn draw_youtube_stage(
    frame: &mut Frame,
    area: Rect,
    queue: &QueueSnapshot,
    paired_client: Option<&ClientAudioState>,
    paired_browser_source: AudioSource,
) {
    let width = area.width as usize;
    let has_track = queue.current.is_some();
    let is_browser =
        matches!(paired_client, Some(c) if c.client_kind == ClientKind::Browser);
    let pinned_icecast = is_browser && paired_browser_source == AudioSource::Icecast;
    let mode_tag = if paired_client.is_none() {
        "off"
    } else if pinned_icecast {
        "pinned"
    } else if has_track {
        "live"
    } else {
        "fallback"
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // blank
        Constraint::Length(1), // track title
        Constraint::Length(1), // channel / subtitle
        Constraint::Length(1), // progress
        Constraint::Length(1), // blank
        Constraint::Length(1), // skip meter / hint
        Constraint::Length(1), // blank
        Constraint::Length(1), // "next ⌄"
        Constraint::Min(0),    // next items
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(stage_title_line(area.width, "YOUTUBE", mode_tag)),
        rows[0],
    );

    if let Some(current) = &queue.current {
        let title = current
            .title
            .clone()
            .unwrap_or_else(|| format!("yt:{}", current.video_id));
        frame.render_widget(track_title_para(&title, width, "  "), rows[2]);

        let subtitle = current
            .channel
            .clone()
            .or_else(|| (!current.submitter.is_empty()).then(|| format!("by {}", current.submitter)));
        if let Some(subtitle) = subtitle {
            frame.render_widget(track_meta_para(&subtitle, width, "  "), rows[3]);
        }

        let elapsed_secs = current
            .started_at_ms
            .map(|started| {
                let now_ms = chrono::Utc::now().timestamp_millis();
                ((now_ms.saturating_sub(started)).max(0) / 1000) as u64
            })
            .unwrap_or(0);
        let inner = inset_left(rows[4], 2);
        if let Some(duration_ms) = current.duration_ms
            && duration_ms > 0
            && !current.is_stream
        {
            draw_progress_line(frame, inner, elapsed_secs, (duration_ms as u64) / 1000);
        } else {
            draw_elapsed_line(frame, inner, elapsed_secs);
        }

        if let Some(progress) = &queue.skip_progress {
            frame.render_widget(
                Paragraph::new(Line::from(skip_meter_spans(progress))),
                rows[6],
            );
        }

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "next ⌄",
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                ),
            ])),
            rows[8],
        );

        let max_rows = (rows[9].height as usize).min(3);
        if queue.queue.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        "· queue ends after this one",
                        Style::default().fg(theme::TEXT_FAINT()),
                    ),
                ])),
                rows[9],
            );
        } else {
            let lines: Vec<Line<'static>> = queue
                .queue
                .iter()
                .take(max_rows)
                .enumerate()
                .map(|(idx, item)| queue_next_line(idx, item, width))
                .collect();
            frame.render_widget(Paragraph::new(lines), rows[9]);
        }
    } else {
        frame.render_widget(track_title_para("fallback stream", width, "  "), rows[2]);
        frame.render_widget(track_meta_para("YouTube · 24/7", width, "  "), rows[3]);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled("queue empty", Style::default().fg(theme::TEXT_DIM())),
            ])),
            rows[6],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled("submit with  ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    "v+v",
                    Style::default()
                        .fg(theme::AMBER_DIM())
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            rows[8],
        );
    }
}

/// Active Icecast panel: title + artist + progress + genre vibe + vote dots.
/// Mirrors the previous now-playing block's data but framed in the new
/// stage chrome so it sits alongside the YouTube panel cleanly.
fn draw_icecast_stage(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    paired_client: Option<&ClientAudioState>,
    vote: &VoteCardView<'_>,
) {
    let width = area.width as usize;
    let mode_tag = match paired_client {
        Some(state) if state.muted => "muted",
        Some(_) => "live",
        None => "off",
    };

    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Length(1), // blank
        Constraint::Length(1), // track title
        Constraint::Length(1), // artist
        Constraint::Length(1), // progress
        Constraint::Length(1), // blank
        Constraint::Length(1), // vibe heading
        Constraint::Min(0),    // vote rows
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(stage_title_line(area.width, "ICECAST", mode_tag)),
        rows[0],
    );

    let (title, artist) = match now_playing {
        Some(np) => (
            np.track.title.clone(),
            np.track
                .artist
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
        ),
        None => ("waiting for stream".to_string(), String::new()),
    };
    frame.render_widget(track_title_para(&title, width, "  "), rows[2]);
    if !artist.is_empty() {
        frame.render_widget(track_meta_para(&artist, width, "  "), rows[3]);
    }

    if let Some(np) = now_playing {
        let elapsed = np.started_at.elapsed().as_secs();
        let inner = inset_left(rows[4], 2);
        if let Some(dur) = np.track.duration_seconds {
            draw_progress_line(frame, inner, elapsed, dur);
        } else {
            draw_elapsed_line(frame, inner, elapsed);
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "vibe ",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw(" "),
            vote_vibe_inline_span(vote),
        ])),
        rows[6],
    );
    crate::app::vote::ui::draw_vote_inline(frame, rows[7], vote);
}

/// Peek strip shown beneath the active panel when YouTube is the stage —
/// one dim line summarising the radio (genre vote + countdown) so listeners
/// can see what they'd fall back to.
fn draw_icecast_peek(
    frame: &mut Frame,
    area: Rect,
    now_playing: Option<&NowPlaying>,
    vote: &VoteCardView<'_>,
) {
    if area.width == 0 {
        return;
    }
    let next = vote.vote_counts.winner_or(vote.current_genre);
    let current_label =
        crate::app::common::primitives::genre_label(vote.current_genre).to_ascii_lowercase();
    let next_label = crate::app::common::primitives::genre_label(next).to_ascii_lowercase();
    let ends = compact_vote_duration(vote.ends_in);
    let now_label = now_playing
        .map(|np| {
            let artist = np.track.artist.as_deref().unwrap_or("unknown");
            format!("{} · {}", artist, np.track.title)
        })
        .unwrap_or_else(|| "waiting".to_string());

    let spans = vec![
        Span::styled("─ ", Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            "icecast ",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled("· ", Style::default().fg(theme::BORDER_DIM())),
        Span::styled(current_label, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(" → ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(next_label, Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" · ", Style::default().fg(theme::BORDER_DIM())),
        Span::styled(ends, Style::default().fg(theme::TEXT_FAINT())),
    ];
    let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let remaining = (area.width as usize).saturating_sub(used);
    let mut spans = spans;
    if remaining > 6 {
        spans.push(Span::styled("  ", Style::default()));
        let now_budget = remaining.saturating_sub(2);
        spans.push(Span::styled(
            truncate_chars(&now_label, now_budget),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Peek strip shown beneath the active Icecast panel — a one-liner about
/// the YouTube side. Tag is different depending on *why* the user isn't on
/// YouTube: CLI listeners can't hear it at all, browser users pinned to
/// Icecast via v+x are opting out and the hint reminds them how to swap.
fn draw_youtube_peek(
    frame: &mut Frame,
    area: Rect,
    queue: &QueueSnapshot,
    paired_client: Option<&ClientAudioState>,
    paired_browser_source: AudioSource,
) {
    if area.width == 0 {
        return;
    }
    let is_browser =
        matches!(paired_client, Some(state) if state.client_kind == ClientKind::Browser);
    let pinned_icecast = is_browser && paired_browser_source == AudioSource::Icecast;

    let mut spans = vec![
        Span::styled("─ ", Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            "youtube ",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled("· ", Style::default().fg(theme::BORDER_DIM())),
    ];

    let suffix_text = if pinned_icecast {
        Some("  v+x to switch")
    } else if !is_browser && queue.current.is_some() {
        Some("  pair browser to hear")
    } else {
        None
    };

    let yt_mode_active = queue.audio_mode == AudioMode::Youtube;
    match (&queue.current, queue.queue.len()) {
        (Some(current), extra) => {
            let title = current
                .title
                .clone()
                .unwrap_or_else(|| format!("yt:{}", current.video_id));
            spans.push(Span::styled(
                "▶ ",
                Style::default().fg(theme::AMBER_DIM()),
            ));
            let queue_suffix = if extra > 0 {
                format!("  (+{extra} queued)")
            } else {
                String::new()
            };
            let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            let suffix_len = suffix_text.map(|s| s.chars().count()).unwrap_or(0)
                + queue_suffix.chars().count();
            let title_budget = (area.width as usize).saturating_sub(used + suffix_len);
            spans.push(Span::styled(
                truncate_chars(&title, title_budget),
                Style::default().fg(theme::TEXT_DIM()),
            ));
            if !queue_suffix.is_empty() {
                spans.push(Span::styled(
                    queue_suffix,
                    Style::default().fg(theme::TEXT_FAINT()),
                ));
            }
        }
        (None, _) if yt_mode_active => {
            // Queue empty but the YouTube fallback stream is still on —
            // distinct from "queue truly empty, mode flipped to icecast".
            spans.push(Span::styled(
                "▶ ",
                Style::default().fg(theme::AMBER_DIM()),
            ));
            spans.push(Span::styled(
                "fallback stream",
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        (None, _) => {
            let hint = if pinned_icecast {
                "queue empty · pinned"
            } else if is_browser {
                "queue empty"
            } else {
                "queue empty · browser-only"
            };
            spans.push(Span::styled(hint, Style::default().fg(theme::TEXT_FAINT())));
        }
    }

    if let Some(suffix) = suffix_text {
        let style = if pinned_icecast {
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC)
        };
        spans.push(Span::styled(suffix.to_string(), style));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Skip-vote meter. Caps the dot row at 8 cells so a 20-pair threshold
/// doesn't overflow the rail; the literal `votes/threshold` count below
/// remains authoritative.
fn skip_meter_spans(progress: &super::super::audio::svc::SkipProgress) -> Vec<Span<'static>> {
    const MAX_DOTS: u32 = 8;
    let shown = progress.threshold.min(MAX_DOTS).max(1);
    let votes_shown = progress.votes.min(shown);
    let mut dots = String::with_capacity(shown as usize);
    for i in 0..shown {
        dots.push(if i < votes_shown { '●' } else { '○' });
    }
    vec![
        Span::raw("  "),
        Span::styled(
            "skip  ",
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(dots, Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            format!("  {}/{}", progress.votes, progress.threshold),
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ]
}

/// One entry in the YouTube "next" list. Number, title, then a dim score
/// right-aligned: `+N` (positive), `-N` (negative), `·` (zero).
fn queue_next_line(idx: usize, item: &QueueItemView, width: usize) -> Line<'static> {
    let n_text = format!("  {}  ", idx + 1);
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
        (
            "·".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )
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

fn track_title_para(text: &str, width: usize, lead: &str) -> Paragraph<'static> {
    let budget = width.saturating_sub(lead.chars().count());
    Paragraph::new(Line::from(vec![
        Span::raw(lead.to_string()),
        Span::styled(
            truncate_chars(text, budget),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ]))
}

fn track_meta_para(text: &str, width: usize, lead: &str) -> Paragraph<'static> {
    let budget = width.saturating_sub(lead.chars().count());
    Paragraph::new(Line::from(vec![
        Span::raw(lead.to_string()),
        Span::styled(
            truncate_chars(text, budget),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]))
}

fn inset_left(area: Rect, cols: u16) -> Rect {
    let cols = cols.min(area.width);
    Rect {
        x: area.x + cols,
        y: area.y,
        width: area.width.saturating_sub(cols),
        height: area.height,
    }
}

fn vote_vibe_inline_span(vote: &VoteCardView<'_>) -> Span<'static> {
    let next = vote.vote_counts.winner_or(vote.current_genre);
    let current_label =
        crate::app::common::primitives::genre_label(vote.current_genre).to_ascii_lowercase();
    let next_label = crate::app::common::primitives::genre_label(next).to_ascii_lowercase();
    let ends = compact_vote_duration(vote.ends_in);
    Span::styled(
        format!("{current_label} → {next_label} · {ends}"),
        Style::default().fg(theme::TEXT_BRIGHT()),
    )
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

fn draw_progress_line(frame: &mut Frame, area: Rect, elapsed_secs: u64, duration_secs: u64) {
    if area.width == 0 || duration_secs == 0 {
        return;
    }
    let elapsed = elapsed_secs.min(duration_secs);
    let elapsed_str = format!("{}:{:02}", elapsed / 60, elapsed % 60);
    let total_str = format!("{}:{:02}", duration_secs / 60, duration_secs % 60);
    let time_w = elapsed_str.len() + total_str.len() + 2;
    let bar_w = (area.width as usize).saturating_sub(time_w);
    if bar_w == 0 {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                elapsed_str,
                Style::default().fg(theme::AMBER()),
            ))),
            area,
        );
        return;
    }

    let progress = (elapsed as f64 / duration_secs as f64).clamp(0.0, 1.0);
    let dot = ((bar_w as f64 * progress) as usize).min(bar_w.saturating_sub(1));
    let bar_before = "─".repeat(dot);
    let bar_after = "─".repeat(bar_w.saturating_sub(dot + 1));
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(elapsed_str, Style::default().fg(theme::AMBER())),
            Span::raw(" "),
            Span::styled(bar_before, Style::default().fg(theme::BORDER_DIM())),
            Span::styled("●", Style::default().fg(theme::AMBER_GLOW())),
            Span::styled(bar_after, Style::default().fg(theme::BORDER_DIM())),
            Span::raw(" "),
            Span::styled(total_str, Style::default().fg(theme::TEXT_FAINT())),
        ])),
        area,
    );
}

fn draw_elapsed_line(frame: &mut Frame, area: Rect, elapsed_secs: u64) {
    if area.width == 0 {
        return;
    }
    let elapsed = format!("{}:{:02}", elapsed_secs / 60, elapsed_secs % 60);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(elapsed, Style::default().fg(theme::AMBER())),
            Span::styled(" live", Style::default().fg(theme::TEXT_FAINT())),
        ])),
        area,
    );
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
    use crate::app::vote::svc::{Genre, VoteCount};
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
    fn vote_vibe_inline_renders_current_arrow_next_and_countdown() {
        let counts = VoteCount {
            lofi: 1,
            ambient: 3,
            classic: 0,
            jazz: 0,
        };
        let view = VoteCardView {
            vote_counts: &counts,
            current_genre: Genre::Lofi,
            my_vote: None,
            ends_in: Duration::from_secs(9 * 60),
        };

        let span = vote_vibe_inline_span(&view);
        assert_eq!(span.content.as_ref(), "lofi → ambient · 9m");
    }
}
