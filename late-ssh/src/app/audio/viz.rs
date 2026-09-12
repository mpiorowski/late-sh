use crate::app::{audio::client_state::ClientAudioState, common::theme};
use late_core::audio::VizFrame;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::{Duration, Instant};

/// Rows the equalizer band is drawn at; the music stage pins this height
/// and a taller area centers the band vertically.
const EQ_ROWS: usize = 3;
/// Vertical resolution per cell: the ▁..█ ramp.
const SUBCELLS: u16 = 8;
/// Full band height in sub-cells.
const MAX_LEVEL: u16 = EQ_ROWS as u16 * SUBCELLS;
/// Bars are one column wide with a one-column gap: the gap is what makes
/// the strip read as an equalizer instead of a solid block wall.
const BAR_STRIDE: usize = 2;
/// Sub-cell fill glyphs, index = filled eighths of the cell.
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Paid frames a peak cap hangs before falling back onto its bar: the cap
/// is the max bar level over this trailing window, so it needs no state.
const CAP_HOLD_FRAMES: usize = 6;

/// A spectrum the client stopped refreshing for this long no longer
/// describes what is playing: the client muted, switched to YouTube,
/// dropped its pair socket, or never analyzes at all. The strip falls back
/// to the ambient band. The CLI sends at ~15 Hz, so this is ~11 frames.
const SPECTRUM_STALE_AFTER: Duration = Duration::from_millis(750);
/// Share of the gap to a louder band closed in one client frame: a hit
/// lands almost at once.
const BAND_ATTACK: f32 = 0.6;
/// Share of the gap to a quieter band closed in one client frame: decays
/// ease down instead of flickering.
const BAND_RELEASE: f32 = 0.3;
/// How far a peak cap falls per client frame, as a share of the full band.
const PEAK_FALL: f32 = 0.04;

/// Deterministic per-bar phase in [0, τ): an integer hash spread over the
/// circle so neighbouring bars never move in lockstep.
fn bar_phase(seed: usize) -> f32 {
    let hashed = (seed as u32).wrapping_mul(2_654_435_761);
    (hashed >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

/// Synthesized bar level in 1..=[`MAX_LEVEL`] for one paid frame. Not
/// audio: two incommensurate per-bar oscillators plus a slow swell
/// travelling across the strip (the shared rhythm), shaped by a
/// bass-heavy envelope so the left of the band runs taller, the way a
/// real spectrum sits. Never zero: the band always reads as live.
fn bar_level(bar: usize, bars: usize, anim_frame: usize) -> u16 {
    let t = anim_frame as f32;
    let fast = (t * 0.51 + bar_phase(bar)).sin();
    let slow = (t * 0.173 + bar_phase(bar + 101)).sin();
    let swell = (t * 0.071 - bar as f32 * 0.9).sin();
    let position = bar as f32 / bars.max(1) as f32;
    let envelope = 1.0 - 0.35 * position;
    let unit = 0.42 + 0.30 * fast + 0.18 * slow + 0.10 * swell;
    ((unit.max(0.04) * envelope * MAX_LEVEL as f32) as u16).clamp(1, MAX_LEVEL)
}

/// Peak cap for a bar: the highest level it hit over the trailing
/// [`CAP_HOLD_FRAMES`] paid frames, so a spike leaves a marker that hangs
/// above the bar and then drops back onto it.
fn cap_level(bar: usize, bars: usize, anim_frame: usize) -> u16 {
    (0..=CAP_HOLD_FRAMES)
        .map(|back| bar_level(bar, bars, anim_frame.saturating_sub(back)))
        .max()
        .unwrap_or(1)
}

/// The smoothed spectrum the eq draws, each value in 0..=1: the eight
/// analyzer bands low to high, and a falling peak cap per band that never
/// sits below its band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LiveBands {
    pub(crate) levels: [f32; 8],
    pub(crate) peaks: [f32; 8],
}

/// The paired client's spectrum as this session last heard it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spectrum {
    bands: LiveBands,
    received_at: Instant,
}

impl Spectrum {
    /// Folds one client frame in. The first frame of a run lands as-is;
    /// later ones ease toward the new bands and let the caps fall.
    pub(crate) fn next(current: Option<Spectrum>, frame: &VizFrame, now: Instant) -> Spectrum {
        let targets = frame.bands.map(band_target);
        let bands = match current {
            None => LiveBands {
                levels: targets,
                peaks: targets,
            },
            Some(spectrum) => {
                let levels: [f32; 8] = std::array::from_fn(|i| {
                    smooth(spectrum.bands.levels[i], targets[i])
                });
                let peaks = std::array::from_fn(|i| {
                    levels[i].max(spectrum.bands.peaks[i] - PEAK_FALL)
                });
                LiveBands { levels, peaks }
            }
        };
        Spectrum {
            bands,
            received_at: now,
        }
    }

    pub(crate) fn is_stale(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.received_at) > SPECTRUM_STALE_AFTER
    }

    pub(crate) fn bands(&self) -> LiveBands {
        self.bands
    }
}

/// A client band as a drawable target. Frames come off the network, so a
/// non-finite or out-of-range value lands at the nearest edge of 0..=1.
fn band_target(band: f32) -> f32 {
    match band.is_finite() {
        true => band.clamp(0.0, 1.0),
        false => 0.0,
    }
}

fn smooth(current: f32, target: f32) -> f32 {
    let rate = match target > current {
        true => BAND_ATTACK,
        false => BAND_RELEASE,
    };
    current + (target - current) * rate
}

/// The spectrum's height under one bar, in 0..=1: the eight bands
/// stretched across however many bars the width holds, interpolated
/// between neighbours so a wide strip slopes instead of stepping.
pub(crate) fn spectrum_unit(bands: &[f32; 8], bar: usize, bars: usize) -> f32 {
    let last = bands.len() - 1;
    let position = match bars {
        0 | 1 => 0.0,
        _ => bar as f32 * last as f32 / (bars - 1) as f32,
    };
    let low = (position.floor() as usize).min(last);
    let high = (low + 1).min(last);
    let t = position - low as f32;
    bands[low] + (bands[high] - bands[low]) * t
}

/// A 0..=1 height as a bar level; every bar keeps its base pixel so a quiet
/// passage still reads as a meter, not as an empty strip.
fn unit_level(unit: f32) -> u16 {
    ((unit * MAX_LEVEL as f32).round() as u16).clamp(1, MAX_LEVEL)
}

/// What the equalizer strip should be saying about this session's audio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EqState {
    /// A client is paired, unmuted, and streaming the spectrum of what it
    /// plays: the bars are the music.
    Live(LiveBands),
    /// A client is paired and unmuted but sends no spectrum (YouTube, or a
    /// CLI without the analyzer): the band dances on the wall clock.
    Ambient,
    /// A client is paired and muted: a steady flat line, the meter at rest.
    Muted,
    /// Nothing is paired, so this session has no audio surface at all. A
    /// dancing band here would be claiming playback that cannot exist, so
    /// the strip points at the guide instead.
    Unpaired,
}

/// The one reading of pairing, mute, and spectrum every eq surface draws.
pub(crate) fn eq_state(
    paired_client: Option<&ClientAudioState>,
    live_bands: Option<LiveBands>,
) -> EqState {
    match (paired_client, live_bands) {
        (None, _) => EqState::Unpaired,
        (Some(client), _) if client.muted => EqState::Muted,
        (Some(_), Some(bands)) => EqState::Live(bands),
        (Some(_), None) => EqState::Ambient,
    }
}

/// Equalizer for the sidebar's music stage. A live spectrum draws as-is;
/// the ambient band has no stored state and is synthesized from the
/// `wall_tick`-derived paid frame (the app's marquee_tick), so the same
/// tick renders the same frame at any loop cadence. Both step once per
/// anim_half `/2` edge, which is exactly the edge tick() pays a frame on.
pub(crate) fn render_eq(frame: &mut Frame, area: Rect, wall_tick: usize, state: EqState) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let anim_frame = wall_tick / 2;
    let bars = width.div_ceil(BAR_STRIDE);

    let mut lines = Vec::with_capacity(area.height as usize);
    // Center the band in whatever height the stage gives us; a shorter
    // area clips the bottom rows (Paragraph drops overflow).
    for _ in 0..(area.height as usize).saturating_sub(EQ_ROWS) / 2 {
        lines.push(Line::from(""));
    }

    let (levels, caps): (Vec<u16>, Vec<u16>) = match state {
        EqState::Live(live) => (
            (0..bars)
                .map(|b| unit_level(spectrum_unit(&live.levels, b, bars)))
                .collect(),
            (0..bars)
                .map(|b| unit_level(spectrum_unit(&live.peaks, b, bars)))
                .collect(),
        ),
        EqState::Ambient => (
            (0..bars).map(|b| bar_level(b, bars, anim_frame)).collect(),
            (0..bars).map(|b| cap_level(b, bars, anim_frame)).collect(),
        ),
        EqState::Muted => {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "─".repeat(width),
                Style::default().fg(theme::AMBER_DIM()),
            )));
            lines.push(Line::from(""));
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
        EqState::Unpaired => {
            lines.push(Line::from(""));
            lines.push(
                Line::from(Span::styled(
                    "no audio here yet",
                    Style::default().fg(theme::TEXT_FAINT()),
                ))
                .centered(),
            );
            lines.push(
                Line::from(vec![
                    Span::styled("press ", Style::default().fg(theme::TEXT_FAINT())),
                    Span::styled("?", Style::default().fg(theme::AMBER())),
                    Span::styled(" to listen", Style::default().fg(theme::TEXT_FAINT())),
                ])
                .centered(),
            );
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
    };

    // Vertical gradient: bar heads glow, the base sits in embers. Caps
    // glow wherever they float so a falling peak stays visible.
    let row_styles = [
        Style::default().fg(theme::AMBER_GLOW()),
        Style::default().fg(theme::AMBER()),
        Style::default().fg(theme::AMBER_DIM()),
    ];
    let cap_style = Style::default().fg(theme::AMBER_GLOW());

    for (row, row_style) in row_styles.iter().enumerate() {
        let cell_from_bottom = (EQ_ROWS - 1 - row) as u16;
        let floor = cell_from_bottom * SUBCELLS;
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut run_text = String::new();
        let mut run_style = *row_style;
        for col in 0..width {
            // Gap columns and blanks extend whatever run is open so the
            // spans stay merged; only real glyphs force a style switch.
            let (glyph, style) = if col % BAR_STRIDE == BAR_STRIDE - 1 {
                (' ', run_style)
            } else {
                let bar = col / BAR_STRIDE;
                let fill = levels[bar].saturating_sub(floor).min(SUBCELLS);
                let cap_here =
                    caps[bar] > levels[bar] && (caps[bar] - 1) / SUBCELLS == cell_from_bottom;
                if fill > 0 {
                    (BLOCKS[fill as usize], *row_style)
                } else if cap_here {
                    ('▁', cap_style)
                } else {
                    (' ', run_style)
                }
            };
            if style != run_style && !run_text.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut run_text), run_style));
            }
            run_style = style;
            run_text.push(glyph);
        }
        if !run_text.is_empty() {
            spans.push(Span::styled(run_text, run_style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "viz_test.rs"]
mod viz_test;
