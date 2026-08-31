//! Render helpers for the splash whisper. Everything here is a pure
//! function of (state, tick, seed): the corruption is deterministic
//! theater, stateless like the sidebar equalizer, so the same tick paints
//! the same frame at any loop cadence.
//!
//! Hard rule (GAME.md, First contact): aesthetic, never system. The
//! static reads as voiced interference (block glyphs in theme colors),
//! never as a real terminal failure.

use ratatui::Frame;
use ratatui::layout::Rect;

use super::state::WhisperState;
use crate::app::common::theme;

/// One frame of whisper theater, precomputed for the splash renderer.
pub(crate) struct WhisperFrame {
    /// The typed prefix of the voiced line, cursor included; empty while
    /// the whisper has not started.
    pub(crate) line: String,
    /// The skip hint as it should draw this frame: the original while
    /// intact, part-dissolved into static after input, `None` once gone.
    pub(crate) hint: Option<String>,
    /// A live static surge, 0.0 (fresh burst) to 1.0 (faded).
    pub(crate) surge: Option<f32>,
    pub(crate) seed: u64,
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

/// A static surge over the whole splash: scattered block cells whose
/// density decays as the burst fades. Painted over whatever is there; the
/// underlying frame heals untouched the moment the surge ends.
pub(crate) fn draw_static_surge(
    frame: &mut Frame,
    area: Rect,
    tick: usize,
    seed: u64,
    progress: f32,
) {
    let density = 0.18 * (1.0 - progress);
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
