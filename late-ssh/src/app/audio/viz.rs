use crate::app::{audio::client_state::ClientAudioState, common::theme, tick::ANIM_HALF_TICK};
use late_core::audio::{VIZ_BANDS, VizFrame};
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
/// Where a band at the running mean lands on the meter: below half, so a
/// steady passage sits low and a hit has room to jump.
const LEVEL_CENTER: f32 = 0.35;
/// Meter height per running swing: a band one typical swing above the mean
/// sits this far above the center.
const LEVEL_SPREAD: f32 = 0.2;
/// The smallest swing the stretch divides by (about 2.4 dB of the CLI's
/// 60 dB meter), so a near-steady signal is not blown up into noise.
const LEVEL_MIN_SWING: f32 = 0.04;
/// Time constant of the running mean and swing: the meter settles into a
/// new passage over a few seconds.
const LEVEL_SETTLE_SECS: f32 = 3.0;
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

/// The smoothed spectrum the eq draws, each value in 0..=1: the analyzer
/// bands low to high, and a peak cap per band that never sits below its
/// band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LiveBands {
    pub(crate) levels: [f32; VIZ_BANDS],
    pub(crate) peaks: [f32; VIZ_BANDS],
}

/// The auto-level: a running mean of the bands and their typical swing
/// around it. The meter is drawn against these, so a quiet track and a loud
/// one both move across the whole band, a flat passage spreads out, and the
/// spectrum keeps its shape: a band above the mean still stands above one
/// below it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Level {
    mean: f32,
    swing: f32,
}

impl Level {
    fn first(bands: &[f32; VIZ_BANDS]) -> Level {
        let (mean, swing) = mean_and_swing(bands);
        Level { mean, swing }
    }

    /// Settles toward this frame's mean and swing by how long it has been
    /// since the last one.
    fn next(self, bands: &[f32; VIZ_BANDS], elapsed: Duration) -> Level {
        let (mean, swing) = mean_and_swing(bands);
        let settle = 1.0 - (-elapsed.as_secs_f32() / LEVEL_SETTLE_SECS).exp();
        Level {
            mean: self.mean + (mean - self.mean) * settle,
            swing: self.swing + (swing - self.swing) * settle,
        }
    }

    /// A band's meter height against this level.
    fn meter(&self, band: f32) -> f32 {
        let swings = (band - self.mean) / self.swing.max(LEVEL_MIN_SWING);
        (LEVEL_CENTER + swings * LEVEL_SPREAD).clamp(0.0, 1.0)
    }
}

/// A frame's mean band, and the mean distance of its bands from that mean.
fn mean_and_swing(bands: &[f32; VIZ_BANDS]) -> (f32, f32) {
    let mean = bands.iter().sum::<f32>() / VIZ_BANDS as f32;
    let swing = bands.iter().map(|band| (band - mean).abs()).sum::<f32>() / VIZ_BANDS as f32;
    (mean, swing)
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
    caps: [Cap; VIZ_BANDS],
    level: Level,
    received_at: Instant,
}

impl Spectrum {
    /// Folds one client frame in. The frame is metered against the running
    /// level first. The first frame of a run lands as-is; later ones ease
    /// toward the new bands, and each cap holds where its bar last struck
    /// before it falls. Caps and the level age on `now`, not on frame count,
    /// so both keep their speed whatever the client's cadence.
    pub(crate) fn next(current: Option<Spectrum>, frame: &VizFrame, now: Instant) -> Spectrum {
        let bands = frame.bands.map(band_target);
        let (level, levels, caps) = match current {
            None => {
                let level = Level::first(&bands);
                let levels = bands.map(|band| level.meter(band));
                (level, levels, levels.map(|band| fold_cap(None, band, now)))
            }
            Some(spectrum) => {
                let elapsed = now.saturating_duration_since(spectrum.received_at);
                let level = spectrum.level.next(&bands, elapsed);
                let levels: [f32; VIZ_BANDS] = std::array::from_fn(|i| {
                    smooth(spectrum.bands.levels[i], level.meter(bands[i]))
                });
                let caps =
                    std::array::from_fn(|i| fold_cap(Some(spectrum.caps[i]), levels[i], now));
                (level, levels, caps)
            }
        };
        Spectrum {
            bands: LiveBands {
                levels,
                peaks: caps.map(|cap| cap.height_at(now)),
            },
            caps,
            level,
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

/// The spectrum's height under one bar, in 0..=1: the bands stretched
/// across however many bars the width holds, interpolated between
/// neighbours so a wide strip slopes instead of stepping.
fn spectrum_unit(bands: &[f32; VIZ_BANDS], bar: usize, bars: usize) -> f32 {
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

/// Bars and their ghosts up to the peak caps, filling `rows` rows (at least one) of `width`
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

    // Between a bar and its peak hangs the bar's ghost, a faint wash that
    // stays up where the bar struck and sinks back onto it.
    let ghost = theme::EQ_GHOST();
    (0..rows)
        .map(|row| {
            let row_style = row_style(row, rows);
            let cell_from_bottom = rows - 1 - row;
            let floor = cell_from_bottom * SUBCELLS;
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut run_text = String::new();
            let mut run_style = row_style;
            for col in 0..width {
                let bar = col / BAR_STRIDE;
                let fill = levels[bar].saturating_sub(floor).min(SUBCELLS);
                let ghost_fill = caps[bar].saturating_sub(floor).min(SUBCELLS);
                let cell = match (col % BAR_STRIDE == BAR_STRIDE - 1, fill, ghost_fill) {
                    (true, _, _) => Cell::Blank,
                    (false, 0, 0) => Cell::Blank,
                    (false, 0, ghost_fill) => {
                        Cell::Glyph(BLOCKS[ghost_fill], Style::default().fg(ghost))
                    }
                    // One cell holds one background, so the head carries the
                    // ghost only when the peak reaches past its top: a peak
                    // ending inside the cell would read as the whole cell.
                    (false, fill, _) if caps[bar] > floor + SUBCELLS => {
                        Cell::Glyph(BLOCKS[fill], row_style.bg(ghost))
                    }
                    (false, fill, _) => Cell::Glyph(BLOCKS[fill], row_style),
                };
                // Blanks extend whatever run is open so the spans stay
                // merged, unless that run paints a background: a gap
                // column must never pick up the ghost.
                let (glyph, style) = match cell {
                    Cell::Blank if run_style.bg.is_none() => (' ', run_style),
                    Cell::Blank => (' ', row_style),
                    Cell::Glyph(glyph, style) => (glyph, style),
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

/// One bar-column cell of the equalizer: open air, or a glyph in its style.
enum Cell {
    Blank,
    Glyph(char, Style),
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
