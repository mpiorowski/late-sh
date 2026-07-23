use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

/// One wavelength of the ambient wave in braille dot columns (2 dot
/// columns per terminal cell, so 8 cells). The scroll offset wraps on
/// this period, so a full cycle repeats every `2 * WAVE_LENGTH_DOTS`
/// wall ticks.
pub(crate) const WAVE_LENGTH_DOTS: usize = 16;

/// Braille dot bits by (column 0-1, row 0-3) within one cell.
const BRAILLE_DOT_BITS: [[u8; 4]; 2] = [
    [0x01, 0x02, 0x04, 0x40],
    [0x08, 0x10, 0x20, 0x80],
];

/// Ambient music wave for the sidebar's music stage: a thin braille line
/// scrolling left, always on while the stage is visible. Purely
/// decorative — no audio data, no pairing state, no stored phase;
/// everything derives from `wall_tick` (the app's marquee_tick), so the
/// same tick renders the same frame at any loop cadence. The offset
/// advances one dot column (half a cell) per anim_half `/2` edge, which
/// is exactly the edge tick() pays a frame on.
pub(crate) fn render_wave(frame: &mut Frame, area: Rect, wall_tick: usize) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let height = area.height as usize;
    let dot_rows = height * 4;
    let dot_cols = width * 2;
    let offset = (wall_tick / 2) % WAVE_LENGTH_DOTS;
    let center = (dot_rows as f32 - 1.0) / 2.0;
    // Primary sine plus a second harmonic so the line reads organic
    // rather than textbook. Amplitudes are sized for the 3-row strip
    // (12 dot rows) and scale with whatever height the stage gives us.
    let swing_scale = dot_rows as f32 / 12.0;

    let mut cells = vec![0u8; width * height];
    let mut prev_y: Option<usize> = None;
    for dx in 0..dot_cols {
        let theta = (dx + offset) as f32 * (std::f32::consts::TAU / WAVE_LENGTH_DOTS as f32);
        let swing = swing_scale * (3.2 * theta.sin() + 1.3 * (2.0 * theta + 1.0).sin());
        let y = ((center - swing).round().max(0.0) as usize).min(dot_rows - 1);
        // Fill the vertical span toward the previous sample so steep
        // slopes stay a connected line instead of scattered dots.
        let (lo, hi) = match prev_y {
            Some(prev) if prev + 1 < y => (prev + 1, y),
            Some(prev) if y + 1 < prev => (y, prev - 1),
            _ => (y, y),
        };
        for dy in lo..=hi {
            cells[(dy / 4) * width + dx / 2] |= BRAILLE_DOT_BITS[dx % 2][dy % 4];
        }
        prev_y = Some(y);
    }

    let style = Style::default().fg(theme::AMBER_DIM());
    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        let text: String = (0..width)
            .map(|col| {
                char::from_u32(0x2800 + cells[row * width + col] as u32)
                    .expect("braille codepoint")
            })
            .collect();
        lines.push(Line::from(Span::styled(text, style)));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "viz_test.rs"]
mod viz_test;
