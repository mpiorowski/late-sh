use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::theme;
use crate::app::audio::{
    client_state::ClientAudioState,
    svc::{QueueItemView, QueueSnapshot},
};
use crate::app::chat::state::ActiveFriend;
use crate::app::presence::ui::friend_names_joined;
use late_core::models::user::{AudioSource, RadioSlots, RadioStation};

// The stock `visualizer` tile's rows over the dock: a decorative band
// (`viz::render_eq`) synthesized from the wall tick, not audio. It only
// dances while a client is actually paired; muted flatlines it, and an
// unpaired session gets a pointer to the guide instead.
pub(crate) const MUSIC_VIZ_HEIGHT: u16 = 3;
// The dock: volume row (1) + the two sources as an
// accordion (10) + keybind footer (1). Constant for BOTH active sources, so
// the panels below never shift; `music_stage_height_is_constant` locks this
// in tests. On radio: title + track + station heading (with its provider
// credit) + `RADIO_SLOTS` slot rows, then the youtube title + track. On
// youtube: the radio title alone, then the youtube title + track +
// `MUSIC_YOUTUBE_DETAIL_HEIGHT` detail rows.
pub(crate) const MUSIC_DOCK_HEIGHT: u16 = 13;
const MUSIC_QUEUE_HEIGHT: u16 = 4;
// YouTube detail rows: progress, skip meter, `next` header, the queue rows.
const MUSIC_YOUTUBE_DETAIL_HEIGHT: u16 = 3 + MUSIC_QUEUE_HEIGHT;
/// Conservative lower bound on any marquee rail width on a Home tile. Real
/// rails are 22-24 columns; using the smaller bound means overflow (and so
/// "still animating") can only be over-reported, never missed.
const MARQUEE_RAIL_MIN: usize = 20;
/// Queue detail rows lose ~6 columns to the index and vote score.
const MARQUEE_QUEUE_RAIL_MIN: usize = MARQUEE_RAIL_MIN - 6;

/// Inputs for [`home_marquee_scrolling`], mirroring what the draw path
/// feeds its marquee rows.
pub(crate) struct HomeMarqueeInputs<'a> {
    /// Whether a music tile is up on the current layout.
    pub music_visible: bool,
    pub active_friends: &'a [ActiveFriend],
    pub radio_now_playing: Option<&'a str>,
    pub selected_station: RadioStation,
    pub source: AudioSource,
    pub queue: Option<&'a QueueSnapshot>,
}

/// True when any Home marquee row currently overflows its rail and is
/// therefore scrolling. The render gate treats that as continuous animation;
/// hold phases are not modeled (tightening pass material). Must stay in sync
/// with the rows the draw path feeds through `marquee_text`: the friends
/// row, the youtube track row, and per source the radio track row or the
/// youtube queue detail rows.
pub(crate) fn home_marquee_scrolling(inputs: &HomeMarqueeInputs<'_>) -> bool {
    use crate::app::common::marquee::marquee_scrolls;

    if marquee_scrolls(
        &friend_names_joined(inputs.active_friends),
        MARQUEE_RAIL_MIN,
    ) {
        return true;
    }
    if !inputs.music_visible {
        return false;
    }
    // The radio track row renders only while radio is the source.
    if inputs.source == AudioSource::Radio
        && marquee_scrolls(
            inputs
                .radio_now_playing
                .unwrap_or(inputs.selected_station.label()),
            MARQUEE_RAIL_MIN,
        )
    {
        return true;
    }
    let Some(queue) = inputs.queue else {
        return false;
    };
    if marquee_scrolls(&youtube_track_text(queue), MARQUEE_RAIL_MIN) {
        return true;
    }
    // Queue detail rows (current + up next) render only for the youtube source.
    inputs.source == AudioSource::Youtube
        && queue.current.iter().chain(queue.queue.iter()).any(|item| {
            let title = item
                .title
                .clone()
                .unwrap_or_else(|| format!("yt:{}", item.video_id));
            marquee_scrolls(&title, MARQUEE_QUEUE_RAIL_MIN)
        })
}

/// Inputs for the music dock, bundled so the pure line builder is easy to
/// drive from tests.
pub(crate) struct MusicStageProps<'a> {
    pub paired_client: Option<&'a ClientAudioState>,
    pub queue: &'a QueueSnapshot,
    pub source: AudioSource,
    pub selected_station: RadioStation,
    pub radio_slots: RadioSlots,
    pub radio_now_playing: Option<&'a str>,
    pub youtube_source_count: usize,
    pub radio_source_count: usize,
    /// Free-running frame counter driving the marquee on now-playing rows
    /// too long for the rail.
    pub marquee_tick: usize,
}

/// The music dock: volume, the two sources as an accordion in the fixed order
/// radio → youtube (radio leads because it is the default source for new
/// users), and the keybind footer.
///
/// Each source's rows sit directly under its own title bar:
/// - On radio: `radio` title, the current station's track, a rule naming
///   the station, the pinned slot rows, the attribution, then `youtube`
///   with its track as a peek at what the booth is playing.
/// - On youtube: `radio` collapses to its title bar (listener count only),
///   then `youtube`, its track, and the queue detail.
///
/// Product rules (user requirements):
/// - Both title bars always show their listener count.
/// - The YouTube track is always visible, also from radio. The radio track
///   shows only while radio is the source. No submitted YouTube track
///   renders "fallback stream", never "queue empty": the fallback is the
///   steady state, not a placeholder.
/// - The dock is a constant `MUSIC_DOCK_HEIGHT` tall for both sources.
///
/// The active source follows the saved preference alone, not whether a
/// client is currently paired: the dock reflects it from the first frame,
/// before the browser has finished pairing. `v+x` toggles between the two
/// sources, so the amber `▌` accent hops between the title bars.
fn draw_music_dock(frame: &mut Frame, area: Rect, props: &MusicStageProps<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = music_stage_lines(area.width, props);
    frame.render_widget(Paragraph::new(lines), area);
}

fn music_stage_lines(width: u16, props: &MusicStageProps<'_>) -> Vec<Line<'static>> {
    let source = props.source;
    let station_label = props.selected_station.label();
    let mut lines = Vec::with_capacity(MUSIC_DOCK_HEIGHT as usize);
    lines.push(volume_row_line(props.paired_client));
    // The keys that move the bar above, named where it is read; they are
    // the house's on every page (`app/input.rs`), not the dock's.
    lines.push(keybind_row_line(width, VOLUME_KEYS));

    lines.push(stage_title_line(
        width,
        "radio",
        Some(&props.radio_source_count.to_string()),
        source == AudioSource::Radio,
    ));
    match source {
        AudioSource::Radio => {
            lines.push(dock_track_line(
                width,
                Some(props.radio_now_playing.unwrap_or(station_label)),
                true,
                props.marquee_tick,
            ));
            // The heading names the current station, which may be off-slot
            // (picked from the stations modal), and credits its provider.
            lines.push(station_heading_line(width, props.selected_station));
            lines.extend(radio_detail_lines(
                width,
                props.selected_station,
                props.radio_slots,
            ));
        }
        AudioSource::Youtube => {}
    }

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
    match source {
        AudioSource::Youtube => {
            let mut detail = youtube_detail_lines(width, props.queue, props.marquee_tick);
            let missing = MUSIC_YOUTUBE_DETAIL_HEIGHT as usize - detail.len();
            pad_blank_lines(&mut detail, missing as u16);
            lines.extend(detail);
        }
        AudioSource::Radio => {}
    }

    // The footer names the active source's own action first; `v+x` is on
    // both so the row never loses the way across. Labels stay short enough
    // for both groups to fit a 21-column rail.
    let footer: &[(&str, &str)] = match source {
        AudioSource::Radio => &[("v+r", "tune"), ("v+x", "source")],
        AudioSource::Youtube => &[("v+v", "queue"), ("v+x", "source")],
    };
    lines.push(keybind_row_line(width, footer));
    lines
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

/// Heading above the radio slot rows: `darksynth · nightride`, the current
/// station's name and its provider's credit. On a narrow rail the provider
/// gives way first; the station label is never cut.
fn station_heading_line(width: u16, station: RadioStation) -> Line<'static> {
    let label = station.label();
    let credit_room =
        (width as usize).saturating_sub(label.chars().count() + " · ".chars().count());
    let faint = Style::default()
        .fg(theme::TEXT_FAINT())
        .add_modifier(Modifier::ITALIC);
    Line::from(vec![
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC),
        ),
        Span::styled(" · ".to_string(), faint),
        Span::styled(
            truncate_chars(station.provider().label(), credit_room),
            faint,
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
    match youtube_track(queue) {
        Some(track) => track,
        None => "fallback stream".to_string(),
    }
}

/// `Channel - Title` for the current YouTube queue item; `None` while the
/// fallback plays, which has no track of its own to name.
pub(crate) fn youtube_track(queue: &QueueSnapshot) -> Option<String> {
    let current = queue.current.as_ref()?;
    let title = current
        .title
        .clone()
        .unwrap_or_else(|| format!("yt:{}", current.video_id));
    Some(match current.channel.as_deref() {
        Some(channel) if !channel.trim().is_empty() => {
            format!("{} - {}", channel.trim(), title)
        }
        _ if !current.submitter.is_empty() => format!("by {} - {}", current.submitter, title),
        _ => title,
    })
}

/// `- = volume  m mute`: the paired client's volume and mute keys, under
/// the bar they move.
pub(crate) const VOLUME_KEYS: &[(&str, &str)] = &[("- =", "volume"), ("m", "mute")];

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

/// YouTube detail rows (≤ `MUSIC_YOUTUBE_DETAIL_HEIGHT`; caller pads): progress/elapsed, skip meter or
/// blank, `next ⌄`, then up to `MUSIC_QUEUE_HEIGHT` queue rows or
/// `· fallback next`. With nothing submitted, the fallback-stream hints.
fn youtube_detail_lines(width: u16, queue: &QueueSnapshot, tick: usize) -> Vec<Line<'static>> {
    let width = width as usize;
    let mut lines = Vec::with_capacity(MUSIC_YOUTUBE_DETAIL_HEIGHT as usize);

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

/// Radio slot rows (exactly `RADIO_SLOTS`): one row per pinned slot
/// (`v1`..`v5`, `●` on the current station, an empty slot points at the
/// stations modal). A current station that is not pinned lights no slot
/// row; the heading above still names it.
fn radio_detail_lines(width: u16, selected: RadioStation, slots: RadioSlots) -> Vec<Line<'static>> {
    slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let key = format!("v{}", index + 1);
            match slot {
                Some(station) => {
                    selector_row_line(width, station.label(), &key, station == selected)
                }
                None => selector_row_line(width, "pin via v+r", &key, false),
            }
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

/// The current track for the saved source, one line: the same text the
/// dock row shows for that source.
pub(crate) fn current_track_text(
    source: AudioSource,
    queue: &QueueSnapshot,
    station: RadioStation,
    radio_now_playing: Option<&str>,
) -> String {
    match source {
        AudioSource::Radio => radio_now_playing
            .map(str::to_string)
            .unwrap_or_else(|| station.label().to_string()),
        AudioSource::Youtube => youtube_track_text(queue),
    }
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

/// The narrowest tile the music dock reads in: the station rows with
/// their keys.
const MUSIC_DOCK_MIN_COLS: u16 = 18;

/// Home's music tile: tall enough, the dock (volume, stations, the keys);
/// shorter, the track over the station. The equalizer is the visualizer
/// tile's.
pub(crate) fn draw_music_tile(frame: &mut Frame, area: Rect, props: &MusicStageProps<'_>) {
    if area.height >= MUSIC_DOCK_HEIGHT && area.width >= MUSIC_DOCK_MIN_COLS {
        draw_music_dock(frame, area, props);
        return;
    }
    let track = current_track_text(
        props.source,
        props.queue,
        props.selected_station,
        props.radio_now_playing,
    );
    let station = station_text(props.source, props.selected_station);
    draw_music_short(frame, area, &track, &station);
}

/// A music tile too short for the dock: the track over the station,
/// centred in the tile, the station first to go when the tile is a single
/// row.
fn draw_music_short(frame: &mut Frame, area: Rect, track: &str, station: &str) {
    if area.height == 0 {
        return;
    }
    let text_rows: u16 = area.height.min(2);
    let top = area.y + (area.height - text_rows) / 2;
    let track = Line::from(vec![
        Span::styled("♪ ", Style::default().fg(theme::AMBER())),
        Span::styled(track.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
    ])
    .centered();
    let station = Line::from(Span::styled(
        station.to_string(),
        Style::default().fg(theme::TEXT_DIM()),
    ))
    .centered();
    frame.render_widget(
        Paragraph::new(vec![track, station]),
        Rect::new(area.x, top, area.width, text_rows),
    );
}

/// The player's second row: the source, then the station or stream it is
/// tuned to. YouTube has no station, so it stays one word.
fn station_text(source: AudioSource, station: RadioStation) -> String {
    match source {
        AudioSource::Radio => format!("radio · {}", station.label()),
        AudioSource::Youtube => "youtube".to_string(),
    }
}

#[cfg(test)]
#[path = "music_dock_test.rs"]
mod music_dock_test;
