use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

/// One period of the ambient wave in columns; the scroll offset wraps on
/// it, so a full cycle repeats every `2 * WAVE_PERIOD_COLS` wall ticks.
pub(crate) const WAVE_PERIOD_COLS: usize = 16;

/// Hand-drawn wave tile: one period over 3 rows in the UI's box-drawing
/// vocabulary (the same rounded glyphs the panel borders use), designed
/// to tile seamlessly: the right edge of each row continues into its
/// left edge. Rows must stay exactly [`WAVE_PERIOD_COLS`] wide (tested).
const WAVE_TILE: [&str; 3] = [
    "      ╭──╮      ",
    "   ╭──╯  ╰──╮   ",
    "───╯        ╰───",
];

/// Ambient music wave for the sidebar's music stage: a rounded stepped
/// sine scrolling left, always on while the stage is visible. No audio
/// data and no stored phase; the frame is the [`WAVE_TILE`] rotated by a
/// `wall_tick`-derived offset (the app's marquee_tick), so the same tick
/// renders the same frame at any loop cadence. The offset advances one
/// column per anim_half `/2` edge, which is exactly the edge tick() pays
/// a frame on. The one nod to audio state: a muted paired client shows a
/// steady flat line, the oscilloscope at rest (the scroll cadence keeps
/// running, its frames just diff to nothing).
pub(crate) fn render_wave(frame: &mut Frame, area: Rect, wall_tick: usize, muted: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let offset = (wall_tick / 2) % WAVE_PERIOD_COLS;
    let style = Style::default().fg(theme::AMBER_DIM());

    let mut lines = Vec::with_capacity(area.height as usize);
    // Center the 3-row tile in whatever height the stage gives us; a
    // shorter area clips the bottom rows (Paragraph drops overflow).
    for _ in 0..(area.height as usize).saturating_sub(WAVE_TILE.len()) / 2 {
        lines.push(Line::from(""));
    }
    if muted {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("─".repeat(width), style)));
        lines.push(Line::from(""));
    } else {
        for row in WAVE_TILE {
            let glyphs: Vec<char> = row.chars().collect();
            let text: String = (0..width)
                .map(|col| glyphs[(col + offset) % WAVE_PERIOD_COLS])
                .collect();
            lines.push(Line::from(Span::styled(text, style)));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "viz_test.rs"]
mod viz_test;
