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
use crate::app::audio::{
    client_state::ClientAudioState,
    stations,
    svc::{QueueItemView, QueueSnapshot},
    viz::Visualizer,
};
use crate::app::bonsai::state::BonsaiState;
use crate::app::bonsai_v2::state::BonsaiV2State;
use crate::app::pet::state::PetState;
use late_core::models::user::{AudioSource, IcecastStream, RadioStation};

const TIME_HEIGHT: u16 = 1;
const RULE_HEIGHT: u16 = 1;
const VISUALIZER_HEIGHT: u16 = 6;
// Full music stage: volume rows (2) + three dock entries (title +
// now-playing, 6) + labeled rule (1) + detail area (5) + keybind footer
// (1). Constant for ALL active sources — chrome must not move between
// states; `music_stage_chrome_rows_never_move` locks this in tests.
const MUSIC_STAGE_HEIGHT: u16 = 15;
// Detail area under the labeled rule: the active source's controls, padded
// to exactly this many rows.
const MUSIC_DETAIL_HEIGHT: u16 = 5;
// Smallest useful viewport over the music stage before it is hidden entirely.
const MUSIC_STAGE_MIN_VISIBLE_HEIGHT: u16 = 4;
const MUSIC_QUEUE_HEIGHT: u16 = 2;
// Bonsai is kept fixed when shown; spare height now belongs to the music stage.
const BONSAI_MIN_HEIGHT: u16 = 16;
// Cat: 3 art rows + 1 footer row.
const CAT_HEIGHT: u16 = 4;

// The visible credit Nightride asked for; rendered as the last detail row
// while the radio source is active.
const RADIO_ATTRIBUTION: &str = "nightride.fm · live";

pub(crate) struct SidebarProps<'a> {
    pub visualizer: &'a Visualizer,
    pub now_playing: Option<&'a NowPlaying>,
    pub paired_client: Option<&'a ClientAudioState>,
    pub bonsai: &'a BonsaiState,
    pub bonsai_v2: &'a BonsaiV2State,
    pub use_bonsai_v2: bool,
    pub cat: &'a PetState,
    pub pet_available: bool,
    pub audio_beat: f32,
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
    /// Count of users whose saved audio source is the direct radio preset.
    /// Rendered as the radio block's title-bar tag.
    pub radio_source_count: usize,
    /// Per-user paired-browser audio source preference (mirrors
    /// `users.settings.audio_source`, cycled by v+x). Picks which source
    /// owns the music stage's detail area; the dock rows stay constant.
    pub paired_browser_source: AudioSource,
    /// Per-user Icecast stream selection (`users.settings.icecast_stream`,
    /// v+1/2 while Icecast is active). The icecast dock row shows THIS
    /// stream's now-playing track.
    pub selected_icecast_stream: IcecastStream,
    /// Per-user radio station selection (`users.settings.radio_station`,
    /// v+1..4 while Radio is active).
    pub selected_radio_station: RadioStation,
    /// Live `Artist - Title` for the selected radio station from the
    /// Nightride metadata SSE; the dock row falls back to the station
    /// display name while this is absent.
    pub radio_now_playing: Option<&'a str>,
    /// AFK message from /brb; None = not AFK.
    pub afk: Option<&'a str>,
}

pub(crate) fn draw_sidebar(frame: &mut Frame, area: Rect, props: &SidebarProps<'_>) {
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

    // Responsive priority on shrink: visualizer drops first, then cat, then
    // bonsai. The music stage is the top priority — cat and bonsai may only
    // take rows the FULL stage doesn't need, so the player never clips while
    // they are visible. Only once both are gone does the stage start clipping
    // from the bottom, and it disappears last. Spare rows go to music, not
    // the tree.
    let cost = |section: u16| RULE_HEIGHT + section;
    let h = area.height;
    let show_music = TIME_HEIGHT + cost(MUSIC_STAGE_MIN_VISIBLE_HEIGHT) <= h;
    let show_bonsai =
        show_music && TIME_HEIGHT + cost(MUSIC_STAGE_HEIGHT) + cost(BONSAI_MIN_HEIGHT) <= h;
    let show_cat = show_bonsai
        && TIME_HEIGHT + cost(MUSIC_STAGE_HEIGHT) + cost(BONSAI_MIN_HEIGHT) + cost(CAT_HEIGHT)
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

    // Time: right-aligned in the top row. Shows AFK indicator when away.
    draw_time_top(frame, inset(layout[i]), props.clock_text, props.afk);
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
            &MusicStageProps {
                now_playing: props.now_playing,
                paired_client: props.paired_client,
                queue: props.queue_snapshot,
                source: props.paired_browser_source,
                selected_stream: props.selected_icecast_stream,
                selected_station: props.selected_radio_station,
                radio_now_playing: props.radio_now_playing,
                youtube_source_count: props.youtube_source_count,
                icecast_source_count: props.icecast_source_count,
                radio_source_count: props.radio_source_count,
            },
        );
        i += 1;
    }

    if show_cat {
        draw_horizontal_rule(frame, inset(layout[i]));
        i += 1;
        let cat_area = inset(layout[i]);
        i += 1;
        if props.pet_available {
            crate::app::pet::ui::draw_cat_inline(frame, cat_area, props.cat);
        } else {
            draw_cat_locked(frame, cat_area);
        }
    }

    if show_bonsai {
        draw_horizontal_rule(frame, inset(layout[i]));
        i += 1;
        if props.use_bonsai_v2 {
            crate::app::bonsai_v2::render::draw_bonsai_inline(
                frame,
                inset(layout[i]),
                props.bonsai_v2,
                props.audio_beat,
            );
        } else {
            crate::app::bonsai::ui::draw_bonsai_inline(
                frame,
                inset(layout[i]),
                props.bonsai,
                props.audio_beat,
            );
        }
    }
}

fn draw_cat_locked(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let top = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(2) / 2,
        width: area.width,
        height: 1,
    };
    let bottom = Rect {
        x: area.x,
        y: top.y.saturating_add(1),
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "cat locked",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )))
        .centered(),
        top,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "CTRL-G",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " for shop",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]))
        .centered(),
        bottom,
    );
}

/// Top-of-rail time. Centered, `⊙` glyph in dim amber, optional timezone
/// label dimmed, time digits bold amber. When AFK, replaces the clock row with
/// an "away" indicator (glyph + "away" or "away — message" if provided).
fn draw_time_top(frame: &mut Frame, area: Rect, clock_text: &str, afk: Option<&str>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if let Some(msg) = afk {
        let mut spans: Vec<Span<'static>> =
            vec![Span::styled("🌙 ", Style::default().fg(theme::AMBER_DIM()))];
        let label = if msg.is_empty() {
            "away".to_string()
        } else {
            format!("away — {msg}")
        };
        spans.push(Span::styled(
            label,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::ITALIC),
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)).centered(), area);
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

/// Inputs for the music stage, bundled so the pure line builder is easy to
/// drive from tests.
struct MusicStageProps<'a> {
    now_playing: Option<&'a NowPlaying>,
    paired_client: Option<&'a ClientAudioState>,
    queue: &'a QueueSnapshot,
    source: AudioSource,
    selected_stream: IcecastStream,
    selected_station: RadioStation,
    radio_now_playing: Option<&'a str>,
    youtube_source_count: usize,
    icecast_source_count: usize,
    radio_source_count: usize,
}

/// Music stage: fixed dock + fixed detail area. Rows 0-1 volume, rows 2-7
/// a three-source dock in order youtube → radio → icecast (title bar +
/// now-playing line per source), row 8 a labeled rule naming the active
/// source, rows 9-13 the active source's controls padded to a constant
/// height, row 14 the keybind footer.
///
/// Two product rules (user requirements):
/// - Every source ALWAYS shows its now-playing line, even when inactive.
///   No submitted YouTube track renders "fallback stream", never "queue
///   empty" — the fallback is the steady state, not a placeholder.
/// - Chrome must not move between states: the stage is a constant
///   `MUSIC_STAGE_HEIGHT` tall and headers/rule/footer sit on the same
///   rows for all three sources.
///
/// The active source follows the saved preference alone, not whether a
/// client is currently paired — the sidebar reflects it from the first
/// frame, before the browser has finished pairing. `v+x` cycles sources
/// in dock order (youtube → radio → icecast), so the amber `▌` accent
/// walks down the dock as the user cycles.
fn draw_music_stage(frame: &mut Frame, area: Rect, props: &MusicStageProps<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let lines = music_stage_lines(area.width, props);
    frame.render_widget(Paragraph::new(lines), area);
}

fn music_stage_lines(width: u16, props: &MusicStageProps<'_>) -> Vec<Line<'static>> {
    let source = props.source;
    let mut lines = Vec::with_capacity(MUSIC_STAGE_HEIGHT as usize);
    lines.push(volume_row_line(props.paired_client));
    lines.push(keybind_row_line(width, &[("m", "mute"), ("-=", "vol")]));

    lines.push(stage_title_line(
        width,
        "youtube",
        Some(&props.youtube_source_count.to_string()),
        source == AudioSource::Youtube,
    ));
    lines.push(dock_track_line(
        width,
        Some(&youtube_track_text(props.queue)),
        source == AudioSource::Youtube,
    ));
    lines.push(stage_title_line(
        width,
        "radio",
        Some(&props.radio_source_count.to_string()),
        source == AudioSource::Radio,
    ));
    let station_name = stations::radio_station_display_name(props.selected_station);
    lines.push(dock_track_line(
        width,
        Some(props.radio_now_playing.unwrap_or(station_name)),
        source == AudioSource::Radio,
    ));
    lines.push(stage_title_line(
        width,
        "icecast",
        Some(&props.icecast_source_count.to_string()),
        source == AudioSource::Icecast,
    ));
    lines.push(dock_track_line(
        width,
        props.now_playing.map(icecast_track_text).as_deref(),
        source == AudioSource::Icecast,
    ));

    lines.push(labeled_rule_line(width, source_label(source)));

    let mut detail = match source {
        AudioSource::Youtube => youtube_detail_lines(width, props.queue),
        AudioSource::Icecast => {
            icecast_detail_lines(width, props.now_playing, props.selected_stream)
        }
        AudioSource::Radio => radio_detail_lines(width, props.selected_station),
    };
    detail.truncate(MUSIC_DETAIL_HEIGHT as usize);
    let missing = MUSIC_DETAIL_HEIGHT as usize - detail.len();
    pad_blank_lines(&mut detail, missing as u16);
    lines.extend(detail);

    lines.push(keybind_row_line(
        width,
        &[("v+v", "queue"), ("v+x", "source")],
    ));
    lines
}

fn source_label(source: AudioSource) -> &'static str {
    match source {
        AudioSource::Youtube => "youtube",
        AudioSource::Icecast => "icecast",
        AudioSource::Radio => "radio",
    }
}

/// Dock now-playing row. The active source's track brightens; inactive
/// stays dim. `None` renders the icecast `no signal` placeholder.
fn dock_track_line(width: u16, track: Option<&str>, active: bool) -> Line<'static> {
    match track {
        Some(text) => {
            let style = if active {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            Line::from(Span::styled(truncate_chars(text, width as usize), style))
        }
        None => Line::from(Span::styled(
            "no signal",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    }
}

/// Labeled rule between dock and detail area: dim dashes around the active
/// source's name so the controls below read as belonging to it.
fn labeled_rule_line(width: u16, label: &str) -> Line<'static> {
    let used = 3 + label.chars().count() + 1;
    let trail = (width as usize).saturating_sub(used).max(1);
    Line::from(vec![
        Span::styled("── ".to_string(), Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ),
        Span::raw(" "),
        Span::styled(
            "─".repeat(trail),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ])
}

/// Selector row: `●`/`○` state glyph, lowercase display name, right-aligned
/// key hint. Inherits the deleted vote rows' visual language.
fn selector_row_line(width: u16, name: &str, key: &str, selected: bool) -> Line<'static> {
    let (glyph, glyph_style, name_style) = if selected {
        (
            "●",
            Style::default().fg(theme::AMBER_GLOW()),
            Style::default().fg(theme::TEXT()),
        )
    } else {
        (
            "○",
            Style::default().fg(theme::BORDER_DIM()),
            Style::default().fg(theme::TEXT_DIM()),
        )
    };
    let key_w = key.chars().count();
    let name_budget = (width as usize).saturating_sub(2 + key_w + 1);
    let name_text = truncate_chars(name, name_budget);
    let pad = (width as usize)
        .saturating_sub(2 + name_text.chars().count() + key_w)
        .max(1);
    Line::from(vec![
        Span::styled(glyph.to_string(), glyph_style),
        Span::raw(" "),
        Span::styled(name_text, name_style),
        Span::raw(" ".repeat(pad)),
        Span::styled(
            key.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Combined `Channel - Title` row for the current YouTube queue item;
/// `fallback stream` when nothing is submitted (the fallback is the steady
/// state, never "queue empty").
fn youtube_track_text(queue: &QueueSnapshot) -> String {
    let Some(current) = &queue.current else {
        return "fallback stream".to_string();
    };
    let title = current
        .title
        .clone()
        .unwrap_or_else(|| format!("yt:{}", current.video_id));
    match current.channel.as_deref() {
        Some(channel) if !channel.trim().is_empty() => {
            format!("{} - {}", channel.trim(), title)
        }
        _ if !current.submitter.is_empty() => format!("by {} - {}", current.submitter, title),
        _ => title,
    }
}

/// Combined `Artist - Title` row for the Icecast now-playing track.
fn icecast_track_text(now: &NowPlaying) -> String {
    match now.track.artist.as_deref() {
        Some(artist) if !artist.trim().is_empty() => {
            format!("{} - {}", artist.trim(), now.track.title)
        }
        _ => now.track.title.clone(),
    }
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

/// YouTube detail rows (≤ 5; caller pads): progress/elapsed, skip meter or
/// blank, `next ⌄`, then up to `MUSIC_QUEUE_HEIGHT` queue rows or
/// `· fallback next`. With nothing submitted, the fallback-stream hints.
fn youtube_detail_lines(width: u16, queue: &QueueSnapshot) -> Vec<Line<'static>> {
    let width = width as usize;
    let mut lines = Vec::with_capacity(MUSIC_DETAIL_HEIGHT as usize);

    if let Some(current) = &queue.current {
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
        } else {
            for (idx, item) in queue
                .queue
                .iter()
                .take(MUSIC_QUEUE_HEIGHT as usize)
                .enumerate()
            {
                lines.push(queue_next_line(idx, item, width));
            }
        }
    } else {
        lines.push(Line::from(Span::styled(
            "YouTube · 24/7",
            Style::default().fg(theme::TEXT_DIM()),
        )));
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
    }

    lines
}

/// Icecast detail rows (≤ 5; caller pads): progress/elapsed for the
/// selected stream, then the stream selector rows.
fn icecast_detail_lines(
    width: u16,
    now_playing: Option<&NowPlaying>,
    selected: IcecastStream,
) -> Vec<Line<'static>> {
    let mut lines = Vec::with_capacity(MUSIC_DETAIL_HEIGHT as usize);

    match now_playing {
        Some(now) => {
            let elapsed_secs = now.started_at.elapsed().as_secs();
            match now.track.duration_seconds {
                Some(duration) if duration > 0 => {
                    lines.push(progress_line(width, elapsed_secs, duration));
                }
                _ => lines.push(elapsed_line(elapsed_secs)),
            }
        }
        None => lines.push(Line::from("")),
    }

    for (stream, key) in [(IcecastStream::Chill, "v1"), (IcecastStream::Classical, "v2")] {
        lines.push(selector_row_line(
            width,
            stations::icecast_stream_display_name(stream),
            key,
            stream == selected,
        ));
    }
    lines
}

/// Radio detail rows (exactly 5): four station selector rows, then the
/// Nightride attribution row (the visible credit Nightride asked for).
fn radio_detail_lines(width: u16, selected: RadioStation) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = [
        (RadioStation::Chillsynth, "v1"),
        (RadioStation::Nightride, "v2"),
        (RadioStation::Datawave, "v3"),
        (RadioStation::Spacesynth, "v4"),
    ]
    .into_iter()
    .map(|(station, key)| {
        selector_row_line(
            width,
            stations::radio_station_display_name(station),
            key,
            station == selected,
        )
    })
    .collect();

    lines.push(Line::from(Span::styled(
        truncate_chars(RADIO_ATTRIBUTION, width as usize),
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    )));
    lines
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

    #[test]
    fn sidebar_clock_text_falls_back_to_utc_when_timezone_missing() {
        let clock = sidebar_clock_text(None);
        assert!(clock.starts_with("UTC "));
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn empty_queue() -> QueueSnapshot {
        QueueSnapshot {
            audio_mode: crate::app::audio::svc::AudioMode::Icecast,
            current: None,
            queue: Vec::new(),
            history: Vec::new(),
            skip_progress: None,
        }
    }

    fn stage_lines_with(
        source: AudioSource,
        selected_stream: IcecastStream,
        selected_station: RadioStation,
    ) -> Vec<Line<'static>> {
        let queue = empty_queue();
        music_stage_lines(
            21,
            &MusicStageProps {
                now_playing: None,
                paired_client: None,
                queue: &queue,
                source,
                selected_stream,
                selected_station,
                radio_now_playing: None,
                youtube_source_count: 3,
                icecast_source_count: 9,
                radio_source_count: 1,
            },
        )
    }

    fn stage_lines(source: AudioSource) -> Vec<Line<'static>> {
        stage_lines_with(source, IcecastStream::Chill, RadioStation::Chillsynth)
    }

    const ALL_SOURCES: [AudioSource; 3] = [
        AudioSource::Youtube,
        AudioSource::Icecast,
        AudioSource::Radio,
    ];

    #[test]
    fn music_stage_chrome_rows_never_move() {
        for source in ALL_SOURCES {
            let lines = stage_lines(source);
            let texts: Vec<String> = lines.iter().map(line_text).collect();
            assert_eq!(texts.len(), MUSIC_STAGE_HEIGHT as usize, "{source:?}");
            assert!(texts[2].starts_with("▌ youtube"), "{source:?}");
            assert!(texts[4].starts_with("▌ radio"), "{source:?}");
            assert!(texts[6].starts_with("▌ icecast"), "{source:?}");
            assert!(texts[8].starts_with("── "), "{source:?}");
            assert!(texts[8].contains(source_label(source)), "{source:?}");
            assert!(texts[14].contains("v+x source"), "{source:?}");
        }
    }

    #[test]
    fn music_stage_dock_rows_always_show_now_playing() {
        for source in ALL_SOURCES {
            let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
            assert_eq!(texts[3], "fallback stream", "{source:?}");
            assert_eq!(texts[5], "chillsynth", "{source:?}");
            assert_eq!(texts[7], "no signal", "{source:?}");
        }
    }

    #[test]
    fn music_stage_dock_rows_keep_listener_counts() {
        for source in ALL_SOURCES {
            let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
            assert!(texts[2].trim_end().ends_with('3'), "{source:?}");
            assert!(texts[4].trim_end().ends_with('1'), "{source:?}");
            assert!(texts[6].trim_end().ends_with('9'), "{source:?}");
        }
    }

    #[test]
    fn icecast_selector_rows_mark_selected_stream() {
        let texts: Vec<String> = stage_lines_with(
            AudioSource::Icecast,
            IcecastStream::Classical,
            RadioStation::Chillsynth,
        )
        .iter()
        .map(line_text)
        .collect();
        // Detail rows 9..13: progress/blank, chill, classical, padding.
        assert!(texts[10].starts_with("○ chill"));
        assert!(texts[10].trim_end().ends_with("v1"));
        assert!(texts[11].starts_with("● classical"));
        assert!(texts[11].trim_end().ends_with("v2"));
    }

    #[test]
    fn radio_selector_rows_mark_selected_station() {
        let texts: Vec<String> = stage_lines_with(
            AudioSource::Radio,
            IcecastStream::Chill,
            RadioStation::Datawave,
        )
        .iter()
        .map(line_text)
        .collect();
        // Detail rows 9..13: four selectors then the attribution row.
        assert!(texts[9].starts_with("○ chillsynth"));
        assert!(texts[10].starts_with("○ nightride"));
        assert!(texts[11].starts_with("● datawave"));
        assert!(texts[11].trim_end().ends_with("v3"));
        assert!(texts[12].starts_with("○ spacesynth"));
        assert!(texts[13].contains("nightride.fm"));
        // The selected station also names the radio dock row.
        assert_eq!(texts[5], "datawave");
    }

    #[test]
    fn radio_dock_row_prefers_sse_metadata() {
        let queue = empty_queue();
        let lines = music_stage_lines(
            21,
            &MusicStageProps {
                now_playing: None,
                paired_client: None,
                queue: &queue,
                source: AudioSource::Youtube,
                selected_stream: IcecastStream::Chill,
                selected_station: RadioStation::Chillsynth,
                radio_now_playing: Some("An Artist - A Track"),
                youtube_source_count: 3,
                icecast_source_count: 9,
                radio_source_count: 1,
            },
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts[5], "An Artist - A Track");
    }
}
