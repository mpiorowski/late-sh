//! Render helpers for the splash whisper, the breakthrough, and the
//! corrupted clock and name. Everything here is a pure
//! function of (state, tick, seed): the corruption is deterministic
//! theater, stateless like the sidebar equalizer, so the same tick paints
//! the same frame at any loop cadence.
//!
//! Hard rule (GAME.md, First contact): aesthetic, never system. The
//! static reads as voiced interference (block glyphs in theme colors),
//! never as a real terminal failure.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::state::{BREAKTHROUGH_LINE, HauntState, WhisperState};
use crate::app::common::theme;

/// Peak share of cells a splash surge covers.
const SPLASH_SURGE_DENSITY: f32 = 0.18;
/// The breakthrough tears harder than the door knocks.
const BREAKTHROUGH_SURGE_DENSITY: f32 = 0.32;

/// One frame of whisper theater, precomputed for the splash renderer.
pub(crate) struct WhisperFrame {
    /// The typed prefix of the voiced line, cursor included; empty while
    /// the whisper has not started.
    pub(crate) line: String,
    /// The skip hint as it should draw this frame: the original while
    /// intact, part-dissolved into static once the voice starts, `None`
    /// once gone.
    pub(crate) hint: Option<String>,
    /// A live static surge, 0.0 (fresh burst) to 1.0 (faded).
    pub(crate) surge: Option<f32>,
    pub(crate) seed: u64,
}

/// The whisper theater for this frame, or `None` while the door is not
/// held. What `DrawContext` carries; the root only routes.
pub(crate) fn whisper_frame_for(
    haunt: &HauntState,
    splash_ticks: usize,
    hint: &str,
) -> Option<WhisperFrame> {
    haunt
        .whisper
        .as_ref()
        .map(|whisper| whisper_frame(whisper, splash_ticks, hint))
}

/// The sidebar clock as it should draw this frame: glitched while a
/// stage-1 burst is live, untouched otherwise. Draw-time transform only;
/// the clock text itself is never wrong.
pub(crate) fn apply_clock_glitch(haunt: &HauntState, marquee_tick: usize, clock: String) -> String {
    match haunt
        .clock_glitch
        .as_ref()
        .and_then(|glitch| glitch.corruption(marquee_tick))
    {
        Some(burst_seed) => glitched_clock(&clock, burst_seed),
        None => clock,
    }
}

/// Stage 2's live hit for the chat row builder: which message's author
/// label to corrupt this frame, and this wave's seed. Rides the rows cache
/// key, so start and heal each rebuild the rows exactly once.
///
/// Two sources, one slot: this session's own hit (it is being haunted) and
/// a beat it is witnessing on somebody else's name. Own first, because the
/// one moment stage 2 is built around is your eye on your own send.
pub(crate) fn name_flicker_for(
    haunt: &HauntState,
    marquee_tick: usize,
) -> Option<(uuid::Uuid, u64)> {
    haunt
        .name_flicker
        .as_ref()
        .and_then(|flicker| flicker.corruption(marquee_tick))
        .or_else(|| {
            haunt
                .witness
                .as_ref()
                .and_then(|hit| hit.corruption(marquee_tick))
        })
}

/// An author label with two or three of its name characters swapped for
/// glyph-alphabet characters (a heavier corruption than the clock's one
/// or two: stage 2 is meant to be hard to miss). Deterministic per burst
/// seed; only alphanumeric characters are touched, so badges, flags, and
/// spacing around the name stay intact (chrome stays legible, the name is
/// what flickers).
pub(crate) fn glitched_name(label: &str, burst_seed: u64) -> String {
    let targets: Vec<usize> = label
        .char_indices()
        .enumerate()
        .filter(|(_, (_, ch))| ch.is_alphanumeric())
        .map(|(char_index, _)| char_index)
        .collect();
    if targets.is_empty() {
        return label.to_string();
    }
    let swaps = (2 + (unit_hash(0, 2, burst_seed) < 0.5) as usize).min(targets.len());
    // Draw until the swaps are distinct; the attempt cap keeps the draw
    // deterministic and bounded on very short names.
    let mut swap_at: Vec<usize> = Vec::with_capacity(swaps);
    for n in 1..=16u64 {
        if swap_at.len() >= swaps {
            break;
        }
        let pick = targets[(unit_hash(n, 2, burst_seed) * targets.len() as f32) as usize];
        if !swap_at.contains(&pick) {
            swap_at.push(pick);
        }
    }
    label
        .chars()
        .enumerate()
        .map(|(i, ch)| match swap_at.contains(&i) {
            true => {
                let alphabet = crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
                alphabet[(unit_hash(i as u64, 3, burst_seed) * alphabet.len() as f32) as usize]
            }
            false => ch,
        })
        .collect()
}

/// The whisper's splash overlay: the voiced line directly under the coffee
/// cup, and the pulsing static over everything. The dissolving skip hint rides
/// `WhisperFrame::hint` in the splash's own hint draw.
pub(crate) fn draw_splash_whisper(
    frame: &mut Frame,
    area: Rect,
    splash_bottom: u16,
    whisper: &WhisperFrame,
    splash_ticks: usize,
) {
    let line_y = splash_bottom + 1;
    if !whisper.line.is_empty() && line_y < area.bottom() {
        let line_area = Rect::new(area.x, line_y, area.width, 1);
        let line = Line::from(Span::styled(
            whisper.line.clone(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::ITALIC),
        ));
        frame.render_widget(Paragraph::new(line).centered(), line_area);
    }
    if let Some(progress) = whisper.surge {
        draw_static_surge(
            frame,
            area,
            splash_ticks,
            whisper.seed,
            progress,
            SPLASH_SURGE_DENSITY,
        );
    }
}

/// One frame of the breakthrough, precomputed for the root draw.
pub(crate) struct BreakthroughFrame {
    /// The typed prefix of the line, cursor included; empty while only the
    /// static has arrived.
    pub(crate) line: String,
    /// A live static surge, 0.0 (fresh burst) to 1.0 (faded).
    pub(crate) surge: Option<f32>,
    pub(crate) seed: u64,
    pub(crate) tick: usize,
}

/// The breakthrough for this frame, or `None` while it is not playing.
/// What `DrawContext` carries; the root only routes.
pub(crate) fn breakthrough_frame_for(
    haunt: &HauntState,
    marquee_tick: usize,
) -> Option<BreakthroughFrame> {
    let breakthrough = haunt.breakthrough.as_ref()?;
    let (typed, typing) = breakthrough.typed_chars(marquee_tick)?;
    let mut line: String = BREAKTHROUGH_LINE.chars().take(typed).collect();
    if typing {
        line.push(if marquee_tick % 4 < 2 { '█' } else { ' ' });
    }
    Some(BreakthroughFrame {
        line,
        surge: breakthrough.surge_progress(marquee_tick),
        seed: breakthrough.seed(),
        tick: marquee_tick,
    })
}

/// The breakthrough over the whole frame, sidebar and chrome included,
/// painted after every modal: the held door's static, heavier, and the
/// voiced line in a gap torn out of the middle of it. No border, no box:
/// a frame around it would read as the system talking.
pub(crate) fn draw_breakthrough(frame: &mut Frame, area: Rect, breakthrough: &BreakthroughFrame) {
    if let Some(progress) = breakthrough.surge {
        draw_static_surge(
            frame,
            area,
            breakthrough.tick,
            breakthrough.seed,
            progress,
            BREAKTHROUGH_SURGE_DENSITY,
        );
    }
    if breakthrough.line.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let width = (BREAKTHROUGH_LINE.chars().count() as u16)
        .saturating_add(4)
        .min(area.width);
    let height = 3.min(area.height);
    let gap = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height / 2).saturating_sub(1).min(area.height - height),
        width,
        height,
    );
    frame.render_widget(Clear, gap);
    let line_area = Rect::new(gap.x, gap.y + height / 2, gap.width, 1);
    let line = Line::from(Span::styled(
        breakthrough.line.clone(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::ITALIC),
    ));
    frame.render_widget(Paragraph::new(line).centered(), line_area);
}

pub(crate) fn whisper_frame(state: &WhisperState, tick: usize, hint: &str) -> WhisperFrame {
    let (typed, typing) = state.typed_chars(tick);
    let mut line: String = state.line().chars().take(typed).collect();
    if typing {
        // The same blink rhythm as the splash's own cursor: the whisper
        // types with the machine's hand, not a broken one.
        line.push(if tick % 4 < 2 { '█' } else { ' ' });
    }
    let hint = match state.dissolve_progress(tick) {
        None => Some(hint.to_string()),
        Some(progress) => dissolved_hint(hint, progress, state.seed()),
    };
    WhisperFrame {
        line,
        hint,
        surge: state.surge_progress(tick),
        seed: state.seed(),
    }
}

/// The skip hint dissolving into static: each glyph flips to a block at
/// its own hashed threshold, then to nothing. `None` once fully gone.
fn dissolved_hint(hint: &str, progress: f32, seed: u64) -> Option<String> {
    if progress >= 1.0 {
        return None;
    }
    let out: String = hint
        .chars()
        .enumerate()
        .map(|(i, ch)| {
            if ch.is_whitespace() {
                return ch;
            }
            let t = unit_hash(i as u64, 0, seed) * 0.7;
            if progress > t + 0.3 {
                ' '
            } else if progress > t {
                static_glyph(unit_hash(i as u64, 1, seed))
            } else {
                ch
            }
        })
        .collect();
    Some(out)
}

/// A static surge over `area`: scattered block cells whose density decays
/// from `peak_density` as the burst fades. Painted over whatever is there;
/// the underlying frame heals untouched the moment the surge ends.
fn draw_static_surge(
    frame: &mut Frame,
    area: Rect,
    tick: usize,
    seed: u64,
    progress: f32,
    peak_density: f32,
) {
    let density = peak_density * (1.0 - progress);
    if density <= 0.0 {
        return;
    }
    let buf = frame.buffer_mut();
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            let roll = unit_hash((u64::from(x) << 32) | u64::from(y), tick as u64, seed);
            if roll >= density {
                continue;
            }
            let Some(cell) = buf.cell_mut((x, y)) else {
                continue;
            };
            let shade = unit_hash((u64::from(y) << 32) | u64::from(x), tick as u64, seed);
            cell.set_char(static_glyph(shade));
            cell.set_fg(if shade < 0.5 {
                theme::TEXT_FAINT()
            } else {
                theme::TEXT_DIM()
            });
        }
    }
}

fn static_glyph(unit: f32) -> char {
    match (unit * 3.0) as u32 {
        0 => '░',
        1 => '▒',
        _ => '▓',
    }
}

/// Stage 1: the sidebar clock with one or two characters swapped for
/// glyph-alphabet characters. Deterministic per burst seed, so the same
/// burst paints the same corruption on every frame of its ~200ms hold.
/// Only the time itself (digits and the colon) is touched: the timezone
/// label staying intact is what makes the wrongness legible.
pub(crate) fn glitched_clock(clock: &str, burst_seed: u64) -> String {
    let targets: Vec<usize> = clock
        .char_indices()
        .enumerate()
        .filter(|(_, (_, ch))| ch.is_ascii_digit() || *ch == ':')
        .map(|(char_index, _)| char_index)
        .collect();
    if targets.is_empty() {
        return clock.to_string();
    }
    let swaps = 1 + (unit_hash(0, 0, burst_seed) < 0.4) as usize;
    let mut swap_at: Vec<usize> = (0..swaps)
        .map(|n| targets[(unit_hash(n as u64 + 1, 0, burst_seed) * targets.len() as f32) as usize])
        .collect();
    swap_at.dedup();
    clock
        .chars()
        .enumerate()
        .map(|(i, ch)| match swap_at.contains(&i) {
            true => {
                let alphabet = crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
                alphabet[(unit_hash(i as u64, 1, burst_seed) * alphabet.len() as f32) as usize]
            }
            false => ch,
        })
        .collect()
}

/// Deterministic hash of (key, tick, seed) spread over [0, 1).
fn unit_hash(key: u64, tick: u64, seed: u64) -> f32 {
    let mut h = key
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(tick.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(seed);
    h ^= h >> 30;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 27;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
