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
use late_core::models::user::{
    AudioSource, IcecastStream, RadioStation, RightSidebarComponent, RightSidebarComponentSetting,
};

// The pinned core block above the panel list: online count + clock on the
// first row, connected friends (or the AFK indicator) on the second. Both
// rows are always reserved so the panels below never shift when presence
// changes.
const TIME_HEIGHT: u16 = 2;
const RULE_HEIGHT: u16 = 1;
const VISUALIZER_HEIGHT: u16 = 4;
// Full music stage: volume rows (2) + three dock entries (title +
// now-playing, 6) + labeled rule (1) + detail area (6) + keybind footer
// (1). Constant for ALL active sources — chrome must not move between
// states; `music_stage_chrome_rows_never_move` locks this in tests.
const MUSIC_STAGE_HEIGHT: u16 = 16;
// Detail area under the labeled rule: the active source's controls, padded
// to exactly this many rows. Sized for radio (five station rows + the
// Nightride attribution row).
const MUSIC_DETAIL_HEIGHT: u16 = 6;
const MUSIC_QUEUE_HEIGHT: u16 = 3;
// Bonsai is kept fixed when shown; the preview renderer scales the tree to
// whatever height it gets.
const BONSAI_MIN_HEIGHT: u16 = 10;
// Daily games: fixed, stable chrome (see `daily/panel.rs`).
const DAILY_HEIGHT: u16 = crate::app::daily::panel::DAILY_PANEL_HEIGHT;

// The visible credit Nightride asked for; rendered as the last detail row
// while the radio source is active.
const RADIO_ATTRIBUTION: &str = "nightride.fm · live";

pub(crate) struct SidebarProps<'a> {
    /// Ordered panels with their on/off state. Render order is top to bottom;
    /// the clock is always pinned above this list.
    pub components: &'a [RightSidebarComponentSetting],
    pub visualizer: &'a Visualizer,
    pub now_playing: Option<&'a NowPlaying>,
    pub paired_client: Option<&'a ClientAudioState>,
    pub bonsai: &'a BonsaiState,
    pub bonsai_v2: &'a BonsaiV2State,
    pub use_bonsai_v2: bool,
    pub audio_beat: f32,
    pub clock_text: &'a str,
    /// YouTube queue snapshot — drives the music stage's active panel and
    /// peek strip. Fed from the same watch channel as the booth modal.
    pub queue_snapshot: &'a QueueSnapshot,
    /// Count of users whose saved audio source is YouTube. Rendered as the
    /// YouTube block's title-bar tag; connection shape is ignored.
    pub youtube_source_count: usize,
    /// Count of users whose saved audio source is Icecast. Rendered as the
    /// Icecast block's title-bar tag.
    pub icecast_source_count: usize,
    /// Count of users whose saved audio source is the direct radio preset
    /// (the default for users who never picked one). Rendered as the radio
    /// block's title-bar tag.
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
    /// v+1..5 while Radio is active).
    pub selected_radio_station: RadioStation,
    /// Live `Artist - Title` for the selected radio station from the
    /// Nightride metadata SSE; the dock row falls back to the station
    /// display name while this is absent.
    pub radio_now_playing: Option<&'a str>,
    /// AFK message from /brb; None = not AFK.
    pub afk: Option<&'a str>,
    /// Daily correspondence games: my matches, lobby activity, glow.
    pub daily: &'a crate::app::daily::state::DailyState,
    /// Humans currently connected (bots excluded), for the core presence row.
    pub online_count: usize,
    /// Connected friends, compacted into the core block's friends row.
    pub active_friend_names: &'a [String],
    /// Free-running frame counter for the music stage's marquee rows.
    pub marquee_tick: usize,
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

    // Responsiveness: the core block (clock + presence) is pinned at the
    // top, then enabled panels render in the user's chosen order. When space
    // runs short panels are dropped by `shrink_priority` (ambience first,
    // music stage last), not by list position. Every panel renders at its
    // full height or not at all. Leftover rows go to the Bonsai panel (the
    // one flexible panel — the tree renderer scales to whatever height it
    // gets); otherwise they collect just above the final panel, which sticks
    // to the bottom of the rail.
    let visible = visible_components(props.components, area.height);
    let bonsai_visible = visible.contains(&RightSidebarComponent::Bonsai);

    // Vertical real estate, top to bottom: the core block, then each visible
    // panel (rule + body at its fixed height; Bonsai's body is a Min so it
    // absorbs the slack). Without a visible Bonsai panel, the flexible
    // spacer sits between the final panel's rule and body, so the rule stays
    // in the natural flow under the panel above while the body sticks to the
    // bottom of the rail. Every panel renders at its full height or not at
    // all — nothing is clipped.
    let last = visible.len().saturating_sub(1);
    let mut constraints = vec![Constraint::Length(TIME_HEIGHT)];
    for (idx, component) in visible.iter().enumerate() {
        constraints.push(Constraint::Length(RULE_HEIGHT)); // ── rule
        if idx == last && !bonsai_visible {
            constraints.push(Constraint::Fill(1)); // drop the last body to the bottom
        }
        constraints.push(if *component == RightSidebarComponent::Bonsai {
            Constraint::Min(component_height(*component))
        } else {
            Constraint::Length(component_height(*component))
        });
    }
    if visible.is_empty() {
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

    // Core block: presence + clock, then friends (or the AFK indicator).
    draw_core_block(
        frame,
        inset(layout[i]),
        props.clock_text,
        props.afk,
        props.online_count,
        props.active_friend_names,
    );
    i += 1;

    for (idx, component) in visible.iter().enumerate() {
        // Each panel's separator rule doubles as its section title
        // (`── lobby ────`), so panels don't spend a body row on a name.
        // The lobby label glows while it's the viewer's turn in any match or
        // a finished match's result is waiting to be acknowledged.
        let rule_active = *component == RightSidebarComponent::Daily
            && (props
                .daily
                .my_matches()
                .iter()
                .any(|item| props.daily.my_turn(item))
                || !props.daily.my_finished().is_empty());
        draw_panel_rule(
            frame,
            inset(layout[i]),
            panel_rule_label(*component),
            rule_active,
        );
        i += 1;
        if idx == last && !bonsai_visible {
            i += 1; // skip the spacer that drops the last body to the bottom
        }
        let body = inset(layout[i]);
        i += 1;
        match component {
            RightSidebarComponent::Visualizer => {
                // Visualizer: borderless inline render.
                props.visualizer.render_inline(frame, body);
            }
            RightSidebarComponent::Music => {
                draw_music_stage(
                    frame,
                    body,
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
                        marquee_tick: props.marquee_tick,
                    },
                );
            }
            RightSidebarComponent::Bonsai => {
                if props.use_bonsai_v2 {
                    crate::app::bonsai_v2::render::draw_bonsai_inline(
                        frame,
                        body,
                        props.bonsai_v2,
                        props.audio_beat,
                    );
                } else {
                    crate::app::bonsai::ui::draw_bonsai_inline(
                        frame,
                        body,
                        props.bonsai,
                        props.audio_beat,
                    );
                }
            }
            RightSidebarComponent::Daily => {
                crate::app::daily::panel::draw_daily_inline(frame, body, props.daily);
            }
        }
    }
}

/// Rows a panel needs to render (excluding its rule). A panel shows at this
/// full height or not at all; the music stage in particular is never clipped
/// to a partial viewport. Bonsai is the exception in the other direction:
/// this is its minimum, and it grows into whatever the rail has left over
/// (the tree renderer scales to its viewport).
fn component_height(component: RightSidebarComponent) -> u16 {
    match component {
        RightSidebarComponent::Visualizer => VISUALIZER_HEIGHT,
        RightSidebarComponent::Music => MUSIC_STAGE_HEIGHT,
        RightSidebarComponent::Bonsai => BONSAI_MIN_HEIGHT,
        RightSidebarComponent::Daily => DAILY_HEIGHT,
    }
}

/// How eagerly a panel is dropped when the rail runs out of rows: higher
/// drops first. Deliberately independent of display order — reordering the
/// sidebar changes where panels sit, not which ones survive a short
/// terminal. Ambience (visualizer, bonsai) goes first; the music stage is
/// the last panel standing.
fn shrink_priority(component: RightSidebarComponent) -> u8 {
    match component {
        RightSidebarComponent::Visualizer => 4, // first to go
        RightSidebarComponent::Bonsai => 3,
        RightSidebarComponent::Daily => 2,
        RightSidebarComponent::Music => 0, // last panel standing
    }
}

/// Pick which enabled panels fit, in render order, given the available height.
/// Panels are kept most-important-first (`shrink_priority`); a panel that
/// doesn't fit is skipped rather than ending the walk, so one tall panel
/// can't shadow a short one that would still fit.
fn visible_components(
    components: &[RightSidebarComponentSetting],
    height: u16,
) -> Vec<RightSidebarComponent> {
    let mut remaining = height.saturating_sub(TIME_HEIGHT);
    let enabled: Vec<RightSidebarComponent> = components
        .iter()
        .filter(|setting| setting.enabled)
        .map(|setting| setting.component)
        .collect();

    let mut by_priority = enabled.clone();
    by_priority.sort_by_key(|component| shrink_priority(*component));
    let mut keep = Vec::new();
    for component in by_priority {
        let need = RULE_HEIGHT + component_height(component);
        if need <= remaining {
            remaining -= need;
            keep.push(component);
        }
    }

    // Survivors render in the user's display order.
    enabled
        .into_iter()
        .filter(|component| keep.contains(component))
        .collect()
}

/// The pinned two-row core block at the top of the rail. Presence is chrome
/// now, not a panel: row one is the online count (left) and the clock
/// (right); row two is connected friends, or the AFK indicator while away.
/// Both rows always render so the panel list below never shifts.
fn draw_core_block(
    frame: &mut Frame,
    area: Rect,
    clock_text: &str,
    afk: Option<&str>,
    online_count: usize,
    active_friend_names: &[String],
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let row = |offset: u16| Rect {
        x: area.x,
        y: area.y + offset,
        width: area.width,
        height: 1,
    };

    // Row 0 — presence left, clock right.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("● ", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                online_count.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" here", Style::default().fg(theme::TEXT_DIM())),
        ])),
        row(0),
    );
    let mut parts = clock_text.rsplitn(2, ' ');
    let time = parts.next().unwrap_or(clock_text);
    let label = parts.next();
    // Native `⊙` (U+2299 circled dot operator). Reliably mono across
    // terminals, reads as a small clock face without competing with digits.
    let mut clock_spans: Vec<Span<'static>> =
        vec![Span::styled("⊙ ", Style::default().fg(theme::AMBER_DIM()))];
    clock_spans.push(Span::styled(
        time.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    if let Some(label) = label {
        clock_spans.push(Span::raw(" "));
        clock_spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(clock_spans)).right_aligned(),
        row(0),
    );

    if area.height < 2 {
        return;
    }

    // Row 1 — AFK wins the row while away; otherwise connected friends.
    // Blank when neither: the reserved row is what keeps chrome stable.
    if let Some(msg) = afk {
        let label = if msg.is_empty() {
            "away".to_string()
        } else {
            format!("away · {msg}")
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("🌙 ", Style::default().fg(theme::AMBER_DIM())),
                Span::styled(
                    label,
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::ITALIC),
                ),
            ])),
            row(1),
        );
    } else if !active_friend_names.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    ACTIVE_FRIEND_MARKER,
                    Style::default()
                        .fg(theme::BADGE_GOLD())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    compact_friend_names(active_friend_names, area.width as usize),
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            row(1),
        );
    }
}

const ACTIVE_FRIEND_MARKER: &str = "★";
const ACTIVE_FRIEND_NAME_LIMIT: usize = 4;

fn compact_friend_names(names: &[String], width: usize) -> String {
    let mut pieces: Vec<String> = names
        .iter()
        .take(ACTIVE_FRIEND_NAME_LIMIT)
        .map(|name| format!("@{}", truncate_chars(name, 10)))
        .collect();
    if names.len() > ACTIVE_FRIEND_NAME_LIMIT {
        pieces.push(format!("+{}", names.len() - ACTIVE_FRIEND_NAME_LIMIT));
    }
    truncate_chars(
        &pieces.join(" "),
        width.saturating_sub(ACTIVE_FRIEND_MARKER.chars().count() + 1),
    )
}

/// Section name rendered into each panel's separator rule. Keeps panel
/// bodies free of title rows: the divider IS the title.
fn panel_rule_label(component: RightSidebarComponent) -> &'static str {
    match component {
        RightSidebarComponent::Visualizer => "visualizer",
        RightSidebarComponent::Music => "music",
        RightSidebarComponent::Bonsai => "bonsai",
        RightSidebarComponent::Daily => "lobby",
    }
}

/// `── label ────` separator-with-title above each panel. `active` swaps the
/// label to bold amber for attention (the lobby's your-turn glow).
fn draw_panel_rule(frame: &mut Frame, area: Rect, label: &str, active: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let label_style = if active {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::ITALIC)
    };
    let width = area.width as usize;
    let used = 3 + label.chars().count() + 1;
    let trail = width.saturating_sub(used).max(1);
    let line = Line::from(vec![
        Span::styled("── ".to_string(), Style::default().fg(theme::BORDER_DIM())),
        Span::styled(label.to_string(), label_style),
        Span::raw(" "),
        Span::styled("─".repeat(trail), Style::default().fg(theme::BORDER_DIM())),
    ]);
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
    /// Free-running frame counter driving the marquee on now-playing rows
    /// too long for the rail.
    marquee_tick: usize,
}

/// Music stage: fixed dock + fixed detail area. Rows 0-1 volume, rows 2-7
/// a three-source dock in order radio → youtube → icecast (title bar +
/// now-playing line per source; radio leads because it is the default
/// source for new users), row 8 a labeled rule naming the active source,
/// rows 9-13 the active source's controls padded to a constant height,
/// row 14 the keybind footer.
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
/// in dock order (radio → youtube → icecast), so the amber `▌` accent
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
        "radio",
        Some(&props.radio_source_count.to_string()),
        source == AudioSource::Radio,
    ));
    let station_name = stations::radio_station_display_name(props.selected_station);
    lines.push(dock_track_line(
        width,
        Some(props.radio_now_playing.unwrap_or(station_name)),
        source == AudioSource::Radio,
        props.marquee_tick,
    ));
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
        props.marquee_tick,
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
        props.marquee_tick,
    ));

    lines.push(labeled_rule_line(width, source_label(source)));

    let mut detail = match source {
        AudioSource::Youtube => youtube_detail_lines(width, props.queue, props.marquee_tick),
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
/// stays dim. `None` renders the icecast `no signal` placeholder. Tracks
/// longer than the rail scroll (marquee) so the full name stays readable.
fn dock_track_line(width: u16, track: Option<&str>, active: bool, tick: usize) -> Line<'static> {
    match track {
        Some(text) => {
            let style = if active {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            Line::from(Span::styled(
                crate::app::common::marquee::marquee_text(text, width as usize, tick),
                style,
            ))
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
        Span::styled("─".repeat(trail), Style::default().fg(theme::BORDER_DIM())),
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
fn youtube_detail_lines(width: u16, queue: &QueueSnapshot, tick: usize) -> Vec<Line<'static>> {
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
                lines.push(queue_next_line(idx, item, width, tick));
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

    for (stream, key) in [
        (IcecastStream::Chill, "v1"),
        (IcecastStream::Classical, "v2"),
    ] {
        lines.push(selector_row_line(
            width,
            stations::icecast_stream_display_name(stream),
            key,
            stream == selected,
        ));
    }
    lines
}

/// Radio detail rows (exactly 6): five station selector rows, then the
/// Nightride attribution row (the visible credit Nightride asked for).
fn radio_detail_lines(width: u16, selected: RadioStation) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = [
        (RadioStation::Chillsynth, "v1"),
        (RadioStation::Nightride, "v2"),
        (RadioStation::Datawave, "v3"),
        (RadioStation::Spacesynth, "v4"),
        (RadioStation::Ambient, "v5"),
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
/// right-aligned: `+N` (positive), `-N` (negative), `·` (zero). Long titles
/// scroll (marquee) inside their budget.
fn queue_next_line(idx: usize, item: &QueueItemView, width: usize, tick: usize) -> Line<'static> {
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
    let title_text = crate::app::common::marquee::marquee_text(&title, title_budget, tick);
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
                marquee_tick: 0,
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
            assert!(texts[2].starts_with("▌ radio"), "{source:?}");
            assert!(texts[4].starts_with("▌ youtube"), "{source:?}");
            assert!(texts[6].starts_with("▌ icecast"), "{source:?}");
            assert!(texts[8].starts_with("── "), "{source:?}");
            assert!(texts[8].contains(source_label(source)), "{source:?}");
            assert!(texts[15].contains("v+x source"), "{source:?}");
        }
    }

    #[test]
    fn music_stage_dock_rows_always_show_now_playing() {
        for source in ALL_SOURCES {
            let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
            assert_eq!(texts[3], "chillsynth", "{source:?}");
            assert_eq!(texts[5], "fallback stream", "{source:?}");
            assert_eq!(texts[7], "no signal", "{source:?}");
        }
    }

    #[test]
    fn music_stage_dock_rows_keep_listener_counts() {
        for source in ALL_SOURCES {
            let texts: Vec<String> = stage_lines(source).iter().map(line_text).collect();
            assert!(texts[2].trim_end().ends_with('1'), "{source:?}");
            assert!(texts[4].trim_end().ends_with('3'), "{source:?}");
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
        // Detail rows 9..14: progress/blank, chill, classical, padding.
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
        // Detail rows 9..14: five selectors then the attribution row.
        assert!(texts[9].starts_with("○ chillsynth"));
        assert!(texts[10].starts_with("○ nightride"));
        assert!(texts[11].starts_with("● datawave"));
        assert!(texts[11].trim_end().ends_with("v3"));
        assert!(texts[12].starts_with("○ spacesynth"));
        assert!(texts[13].starts_with("○ ambient"));
        assert!(texts[13].trim_end().ends_with("v5"));
        assert!(texts[14].contains("nightride.fm"));
        // The selected station also names the radio dock row.
        assert_eq!(texts[3], "datawave");
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
                marquee_tick: 0,
            },
        );
        let texts: Vec<String> = lines.iter().map(line_text).collect();
        assert_eq!(texts[3], "An Artist - A Track");
    }

    fn on(component: RightSidebarComponent) -> RightSidebarComponentSetting {
        RightSidebarComponentSetting {
            component,
            enabled: true,
        }
    }

    fn off(component: RightSidebarComponent) -> RightSidebarComponentSetting {
        RightSidebarComponentSetting {
            component,
            enabled: false,
        }
    }

    #[test]
    fn visible_components_respects_order() {
        let components = [
            on(RightSidebarComponent::Bonsai),
            on(RightSidebarComponent::Music),
            on(RightSidebarComponent::Visualizer),
            on(RightSidebarComponent::Daily),
        ];
        // Tall enough for everything: order is preserved exactly.
        assert_eq!(
            visible_components(&components, 100),
            vec![
                RightSidebarComponent::Bonsai,
                RightSidebarComponent::Music,
                RightSidebarComponent::Visualizer,
                RightSidebarComponent::Daily,
            ]
        );
    }

    #[test]
    fn visible_components_skips_disabled() {
        let components = [
            off(RightSidebarComponent::Visualizer),
            on(RightSidebarComponent::Music),
            off(RightSidebarComponent::Daily),
            on(RightSidebarComponent::Bonsai),
        ];
        assert_eq!(
            visible_components(&components, 100),
            vec![RightSidebarComponent::Music, RightSidebarComponent::Bonsai]
        );
    }

    #[test]
    fn visible_components_drops_by_priority_not_position() {
        // Music sits at the TOP of the display order. With room for only one
        // panel, music survives (lowest shrink priority) even though the old
        // cut-from-the-top rule would have dropped it first.
        let components = [
            on(RightSidebarComponent::Music),
            on(RightSidebarComponent::Bonsai),
        ];
        let height = TIME_HEIGHT + RULE_HEIGHT + MUSIC_STAGE_HEIGHT + 1;
        assert_eq!(
            visible_components(&components, height),
            vec![RightSidebarComponent::Music]
        );
    }

    #[test]
    fn visible_components_skips_unfit_panel_without_stopping() {
        // Bonsai (10) doesn't fit but the visualizer (4) below the cut still
        // does: the walk skips bonsai instead of ending, so lower-priority
        // panels that fit are kept.
        let components = [
            on(RightSidebarComponent::Visualizer),
            on(RightSidebarComponent::Music),
            on(RightSidebarComponent::Bonsai),
        ];
        let height =
            TIME_HEIGHT + RULE_HEIGHT + MUSIC_STAGE_HEIGHT + RULE_HEIGHT + VISUALIZER_HEIGHT;
        assert_eq!(
            visible_components(&components, height),
            vec![
                RightSidebarComponent::Visualizer,
                RightSidebarComponent::Music,
            ]
        );
    }
}
