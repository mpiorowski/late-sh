use crate::app::{audio::client_state::ClientAudioState, common::theme, tick::ANIM_HALF_TICK};
use late_core::audio::VizFrame;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use std::time::{Duration, Instant};

/// Rows the sidebar and music-tile equalizer is drawn at; the music stage
/// pins this height and a taller area centers the band vertically.
const EQ_ROWS: usize = 3;
/// Vertical resolution per cell: the ▁..█ ramp.
const SUBCELLS: usize = 8;
/// Bars are one column wide with a one-column gap: the gap is what makes
/// the strip read as an equalizer instead of a solid block wall.
const BAR_STRIDE: usize = 2;
/// Sub-cell fill glyphs, index = filled eighths of the cell.
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// A spectrum the client stopped refreshing for this long no longer
/// describes what is playing: the client muted, switched to a source it
/// cannot capture, dropped its pair socket, or never analyzes at all. The
/// strip falls back to the ambient band. The CLI sends at ~15 Hz, so this is
/// ~11 frames.
const SPECTRUM_STALE_AFTER: Duration = Duration::from_millis(750);
/// Share of the gap to a louder band closed in one client frame: a hit
/// lands almost at once.
const BAND_ATTACK: f32 = 0.6;
/// Share of the gap to a quieter band closed in one client frame: decays
/// ease down instead of flickering.
const BAND_RELEASE: f32 = 0.3;
/// Seconds a peak cap hangs where its bar struck it before it lets go.
const CAP_HOLD_SECS: f32 = 0.5;
/// How hard a released cap accelerates down, in full bands per second
/// squared: from the top of the band it lands about 0.8s after letting go.
const CAP_GRAVITY: f32 = 3.0;

/// How far a peak cap has fallen, in bands, `age_secs` after its bar struck
/// it: nothing through the hold, then a gravity fall that starts slow and
/// speeds up.
fn cap_drop(age_secs: f32) -> f32 {
    let falling = (age_secs - CAP_HOLD_SECS).max(0.0);
    0.5 * CAP_GRAVITY * falling * falling
}

/// Deterministic per-bar phase in [0, τ): an integer hash spread over the
/// circle so neighbouring bars never move in lockstep.
fn bar_phase(seed: usize) -> f32 {
    let hashed = (seed as u32).wrapping_mul(2_654_435_761);
    (hashed >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

/// Synthesized bar height in (0, 1] for one paid frame. Not audio: two
/// incommensurate per-bar oscillators plus a slow swell travelling across
/// the strip (the shared rhythm), shaped by a bass-heavy envelope so the
/// left of the band runs taller, the way a real spectrum sits. Never zero:
/// the band always reads as live.
fn ambient_unit(bar: usize, bars: usize, anim_frame: usize) -> f32 {
    let t = anim_frame as f32;
    let fast = (t * 0.51 + bar_phase(bar)).sin();
    let slow = (t * 0.173 + bar_phase(bar + 101)).sin();
    let swell = (t * 0.071 - bar as f32 * 0.9).sin();
    let position = bar as f32 / bars.max(1) as f32;
    let envelope = 1.0 - 0.35 * position;
    let unit = 0.42 + 0.30 * fast + 0.18 * slow + 0.10 * swell;
    (unit.max(0.04) * envelope).min(1.0)
}

/// Ambient peak cap for a bar, in 0..=1: the highest a cap struck on any
/// recent paid frame still hangs, so a spike leaves a marker that holds
/// above the bar and then falls back onto it, the way a live cap does.
/// Stateless: the wall tick alone decides it.
fn ambient_cap_unit(bar: usize, bars: usize, anim_frame: usize) -> f32 {
    let frame_secs = ANIM_HALF_TICK.as_secs_f32();
    (0..=anim_frame)
        .map(|back| (back, cap_drop(back as f32 * frame_secs)))
        .take_while(|(_, drop)| *drop < 1.0)
        .map(|(back, drop)| ambient_unit(bar, bars, anim_frame - back) - drop)
        .fold(0.0, f32::max)
}

/// The smoothed spectrum the eq draws, each value in 0..=1: the eight
/// analyzer bands low to high, and a peak cap per band that never sits
/// below its band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LiveBands {
    pub(crate) levels: [f32; 8],
    pub(crate) peaks: [f32; 8],
}

/// A live band's peak cap: the height its bar last struck, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cap {
    struck: f32,
    struck_at: Instant,
}

impl Cap {
    fn height_at(&self, now: Instant) -> f32 {
        let age = now.saturating_duration_since(self.struck_at);
        self.struck - cap_drop(age.as_secs_f32())
    }
}

/// Folds a band's new level into its cap: a bar at or above where the cap
/// has fallen to strikes it afresh, a lower one leaves it falling from its
/// last strike.
fn fold_cap(cap: Option<Cap>, level: f32, now: Instant) -> Cap {
    match cap {
        Some(cap) if cap.height_at(now) > level => cap,
        Some(_) | None => Cap {
            struck: level,
            struck_at: now,
        },
    }
}

/// The paired client's spectrum as this session last heard it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spectrum {
    bands: LiveBands,
    caps: [Cap; 8],
    received_at: Instant,
}

impl Spectrum {
    /// Folds one client frame in. The first frame of a run lands as-is;
    /// later ones ease toward the new bands, and each cap holds where its
    /// bar last struck before it falls. Caps age on `now`, not on frame
    /// count, so the fall keeps its speed whatever the client's cadence.
    pub(crate) fn next(current: Option<Spectrum>, frame: &VizFrame, now: Instant) -> Spectrum {
        let targets = frame.bands.map(band_target);
        let (levels, caps) = match current {
            None => (targets, targets.map(|level| fold_cap(None, level, now))),
            Some(spectrum) => {
                let levels: [f32; 8] =
                    std::array::from_fn(|i| smooth(spectrum.bands.levels[i], targets[i]));
                let caps =
                    std::array::from_fn(|i| fold_cap(Some(spectrum.caps[i]), levels[i], now));
                (levels, caps)
            }
        };
        Spectrum {
            bands: LiveBands {
                levels,
                peaks: caps.map(|cap| cap.height_at(now)),
            },
            caps,
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
fn spectrum_unit(bands: &[f32; 8], bar: usize, bars: usize) -> f32 {
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

/// A 0..=1 height as a bar level out of `max_level` sub-cells; every bar
/// keeps its base pixel so a quiet passage still reads as a meter, not as
/// an empty strip.
fn unit_level(unit: f32, max_level: usize) -> usize {
    ((unit * max_level as f32).round() as usize).clamp(1, max_level)
}

/// What the equalizer strip should be saying about this session's audio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum EqState {
    /// A client is paired, unmuted, and streaming the spectrum of what it
    /// plays: the bars are the music.
    Live(LiveBands),
    /// A client is paired and unmuted but sends no spectrum (YouTube where
    /// the CLI cannot capture the helper, or a CLI without the analyzer): the
    /// band dances on the wall clock.
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

/// The two ways an equalizer's bars move: the eq states that draw bars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Dance {
    Live(LiveBands),
    Ambient,
}

/// Bars and their peak caps filling `rows` rows (at least one) of `width`
/// columns, top row first. A live spectrum draws as-is; the ambient band
/// has no stored state and is synthesized from the `wall_tick`-derived paid
/// frame (the app's marquee_tick), so the same tick renders the same frame
/// at any loop cadence. Both step once per anim_half `/2` edge, which is
/// exactly the edge tick() pays a frame on.
pub(crate) fn dance_lines(
    dance: Dance,
    wall_tick: usize,
    width: usize,
    rows: usize,
) -> Vec<Line<'static>> {
    let anim_frame = wall_tick / 2;
    let bars = width.div_ceil(BAR_STRIDE);
    let max_level = rows * SUBCELLS;
    let (levels, caps): (Vec<usize>, Vec<usize>) = match dance {
        Dance::Live(live) => (0..bars)
            .map(|b| {
                (
                    unit_level(spectrum_unit(&live.levels, b, bars), max_level),
                    unit_level(spectrum_unit(&live.peaks, b, bars), max_level),
                )
            })
            .unzip(),
        Dance::Ambient => (0..bars)
            .map(|b| {
                (
                    unit_level(ambient_unit(b, bars, anim_frame), max_level),
                    unit_level(ambient_cap_unit(b, bars, anim_frame), max_level),
                )
            })
            .unzip(),
    };

    // Caps glow wherever they float so a falling peak stays visible.
    let cap_style = Style::default().fg(theme::AMBER_GLOW());
    (0..rows)
        .map(|row| {
            let row_style = row_style(row, rows);
            let cell_from_bottom = rows - 1 - row;
            let floor = cell_from_bottom * SUBCELLS;
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run_text = String::new();
            let mut run_style = row_style;
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
                        (BLOCKS[fill], row_style)
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
            Line::from(spans)
        })
        .collect()
}

/// Vertical gradient: bar heads glow, the middle burns, the base sits in
/// embers.
fn row_style(row: usize, rows: usize) -> Style {
    let position = row as f32 / rows as f32;
    let color = if position < 0.25 {
        theme::AMBER_GLOW()
    } else if position < 0.6 {
        theme::AMBER()
    } else {
        theme::AMBER_DIM()
    };
    Style::default().fg(color)
}

/// Equalizer for the sidebar's music stage and the Zen music tile: the
/// dancing band at [`EQ_ROWS`] rows, or the muted and unpaired strips.
pub(crate) fn render_eq(frame: &mut Frame, area: Rect, wall_tick: usize, state: EqState) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;

    let mut lines = Vec::with_capacity(area.height as usize);
    // Center the band in whatever height the stage gives us; a shorter
    // area clips the bottom rows (Paragraph drops overflow).
    for _ in 0..(area.height as usize).saturating_sub(EQ_ROWS) / 2 {
        lines.push(Line::from(""));
    }

    let dance = match state {
        EqState::Live(live) => Dance::Live(live),
        EqState::Ambient => Dance::Ambient,
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
    lines.extend(dance_lines(dance, wall_tick, width, EQ_ROWS));
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "viz_test.rs"]
mod viz_test;
