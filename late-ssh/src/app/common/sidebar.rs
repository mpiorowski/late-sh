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
use crate::app::vote::ui::VoteCardView;
use late_core::models::user::AudioSource;

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

    const TIME_HEIGHT: u16 = 1;
    const RULE_HEIGHT: u16 = 1;
    const VISUALIZER_HEIGHT: u16 = 6;
    // Music stage: volume + youtube block + icecast block (with vote), both
    // always visible.
    const MUSIC_STAGE_HEIGHT: u16 = 17;
    // Reserve as if the tree is always Blossom (the tallest: 15 art rows + 1
    // footer). Sized down would clip mature trees; sized up wastes rail.
    const BONSAI_MIN_HEIGHT: u16 = 16;
    // Cat: 3 art rows + 1 footer row.
    const CAT_HEIGHT: u16 = 4;

    let fixed_without_active = TIME_HEIGHT
        + RULE_HEIGHT
        + VISUALIZER_HEIGHT
        + RULE_HEIGHT
        + MUSIC_STAGE_HEIGHT
        + RULE_HEIGHT;
    let cat_budget = CAT_HEIGHT + RULE_HEIGHT;
    let show_cat = fixed_without_active + cat_budget + BONSAI_MIN_HEIGHT <= area.height;

    // Vertical real estate, top to bottom. The cat is lower priority than
    // bonsai: hide it before squeezing the tree below its visible size.
    let mut constraints = vec![
        Constraint::Length(TIME_HEIGHT),        // time
        Constraint::Length(RULE_HEIGHT),        // ── rule
        Constraint::Length(VISUALIZER_HEIGHT),  // visualizer
        Constraint::Length(RULE_HEIGHT),        // ── rule
        Constraint::Length(MUSIC_STAGE_HEIGHT), // active stage + peek strip
        Constraint::Length(RULE_HEIGHT),        // ── rule
    ];
    if show_cat {
        constraints.push(Constraint::Length(CAT_HEIGHT)); // cat
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
        props.youtube_source_count,
        props.icecast_source_count,
    );

    draw_horizontal_rule(frame, inset(layout[5]));

    let mut bonsai_idx = 6;
    if show_cat {
        let cat_area = inset(layout[6]);
        if props.cat_available {
            crate::app::cat::ui::draw_cat_inline(frame, cat_area, props.cat);
        } else {
            draw_cat_locked(frame, cat_area);
        }
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
    if area.width == 0 || area.height < 4 {
        return;
    }

    // Active source follows the saved preference alone, not whether the
    // browser is currently paired. Saved pref is the source of truth — the
    // sidebar should reflect it from the first frame, before the browser
    // has finished pairing.
    let yt_active = paired_browser_source == AudioSource::Youtube;

    let rows = Layout::vertical([
        Constraint::Length(1), // 0:  volume
        Constraint::Length(1), // 1:  vol keybind hints
        Constraint::Length(1), // 2:  yt title
        Constraint::Length(1), // 3:  yt track (channel - title, one line)
        Constraint::Length(1), // 4:  progress
        Constraint::Length(1), // 5:  skip meter (blank when no skip vote)
        Constraint::Length(1), // 6:  next ⌄
        Constraint::Min(2),    // 7:  next items (absorbs spare space)
        Constraint::Length(1), // 8:  booth/swap keybind hints
        Constraint::Length(1), // 9:  ice title
        Constraint::Length(1), // 10: ice track (artist - title, one line)
        Constraint::Length(1), // 11: ice progress / elapsed
        Constraint::Length(1), // 12: vibe → next · ends
        Constraint::Length(3), // 13: vote rows (draw_vote_inline splits internally)
    ])
    .split(area);

    draw_volume_row(frame, rows[0], paired_client);
    draw_keybind_row(frame, rows[1], &[("m", "mute"), ("-=", "vol")]);
    draw_youtube_block(
        frame,
        [rows[2], rows[3], rows[4], rows[5], rows[6], rows[7]],
        queue,
        yt_active,
        youtube_source_count,
    );
    draw_keybind_row(frame, rows[8], &[("v+v", "queue"), ("v+x", "swap")]);
    draw_icecast_block(
        frame,
        [rows[9], rows[10], rows[11], rows[12], rows[13]],
        vote,
        icecast_source_count,
        now_playing,
        !yt_active,
    );
}

/// Volume status row. 10-cell bar at 10% increments, amber fill on
/// BORDER_DIM rail, `NN%` trailing. Replaced by `muted` (italic dim) when
/// the paired client is muted, and `—` (dim) when nothing is paired.
fn draw_volume_row(frame: &mut Frame, area: Rect, paired_client: Option<&ClientAudioState>) {
    if area.width == 0 {
        return;
    }
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
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Keybind hint row. Renders `(key, label)` groups left-to-right in dim
/// chrome; drops trailing groups when the rail is too narrow rather than
/// truncating mid-word. Used twice on the music stage: volume keys under
/// the vol bar, and booth/swap keys between YouTube and Icecast.
fn draw_keybind_row(frame: &mut Frame, area: Rect, groups: &[(&str, &str)]) {
    if area.width == 0 {
        return;
    }
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
        if used + group_w > area.width as usize {
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

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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

/// YouTube block. Fixed 6-row footprint: title, track (`channel - title`
/// combined on one line, mirrors icecast's track row), progress, skip meter,
/// `next ⌄` header, queue list.
fn draw_youtube_block(
    frame: &mut Frame,
    rows: [Rect; 6],
    queue: &QueueSnapshot,
    active: bool,
    source_count: usize,
) {
    let width = rows[0].width as usize;

    // Always show the saved-source count as the tag — both blocks display it
    // regardless of active state so users can see source preference split at
    // a glance. The track body still carries fallback-state copy when
    // `queue.current.is_none()`.
    let tag_string = source_count.to_string();
    frame.render_widget(
        Paragraph::new(stage_title_line(
            rows[0].width,
            "youtube",
            Some(&tag_string),
            active,
        )),
        rows[0],
    );

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
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate_chars(&track_line, width),
                title_style,
            ))),
            rows[1],
        );

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
            draw_progress_line(frame, rows[2], elapsed_secs, (duration_ms as u64) / 1000);
        } else {
            draw_elapsed_line(frame, rows[2], elapsed_secs);
        }

        if let Some(progress) = &queue.skip_progress {
            frame.render_widget(
                Paragraph::new(Line::from(skip_meter_spans(progress))),
                rows[3],
            );
        }

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "next ⌄",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            rows[4],
        );

        let max_rows = rows[5].height as usize;
        if queue.queue.is_empty() {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "· fallback next",
                    Style::default().fg(theme::TEXT_FAINT()),
                ))),
                rows[5],
            );
        } else {
            let lines: Vec<Line<'static>> = queue
                .queue
                .iter()
                .take(max_rows)
                .enumerate()
                .map(|(idx, item)| queue_next_line(idx, item, width))
                .collect();
            frame.render_widget(Paragraph::new(lines), rows[5]);
        }
    } else {
        // No submitted track; the fallback stream is always on.
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("fallback stream", title_style))),
            rows[1],
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("YouTube · 24/7", meta_style))),
            rows[2],
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
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
            ])),
            rows[3],
        );
    }
}

/// Icecast block. 7-row footprint passed as 5 rects: title, track
/// (artist - title combined on one line, mirrors YouTube's two-row title +
/// channel split), progress, `vibe → next · ends` one-liner, and a 3-row
/// vote area that `draw_vote_inline` splits internally.
fn draw_icecast_block(
    frame: &mut Frame,
    rows: [Rect; 5],
    vote: &VoteCardView<'_>,
    source_count: usize,
    now_playing: Option<&NowPlaying>,
    active: bool,
) {
    // Mute/off status is communicated by the volume row above; the title
    // tag here is always the saved-source count, matching the YouTube block's
    // behavior.
    let tag_string = source_count.to_string();
    frame.render_widget(
        Paragraph::new(stage_title_line(
            rows[0].width,
            "icecast",
            Some(&tag_string),
            active,
        )),
        rows[0],
    );

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
    let width = rows[1].width as usize;

    if let Some(now) = now_playing {
        let track_line = match now.track.artist.as_deref() {
            Some(artist) if !artist.trim().is_empty() => {
                format!("{} - {}", artist.trim(), now.track.title)
            }
            _ => now.track.title.clone(),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate_chars(&track_line, width),
                title_style,
            ))),
            rows[1],
        );

        let elapsed_secs = now.started_at.elapsed().as_secs();
        match now.track.duration_seconds {
            Some(duration) if duration > 0 => {
                draw_progress_line(frame, rows[2], elapsed_secs, duration);
            }
            _ => draw_elapsed_line(frame, rows[2], elapsed_secs),
        }
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled("no signal", meta_style))),
            rows[1],
        );
    }

    let current_label =
        crate::app::common::primitives::genre_label(vote.current_genre).to_ascii_lowercase();
    let next_genre = vote.vote_counts.winner_or(vote.current_genre);
    let next_label = crate::app::common::primitives::genre_label(next_genre).to_ascii_lowercase();
    let ends = compact_vote_duration(vote.ends_in);

    let next_style = if active {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(current_label, title_style),
            Span::styled(" → ", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(next_label, next_style),
            Span::styled(" · ", Style::default().fg(theme::BORDER_DIM())),
            Span::styled(ends, Style::default().fg(theme::TEXT_FAINT())),
        ])),
        rows[3],
    );

    crate::app::vote::ui::draw_vote_inline(frame, rows[4], vote);
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
}
