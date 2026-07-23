use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

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

/// Ambient equalizer for the sidebar's music stage, always on while the
/// stage is visible. No audio data and no stored state: bar heights are
/// synthesized from the `wall_tick`-derived paid frame (the app's
/// marquee_tick), so the same tick renders the same frame at any loop
/// cadence. Heights step once per anim_half `/2` edge, which is exactly
/// the edge tick() pays a frame on; sub-edge ticks render identically.
/// The one nod to audio state: a muted paired client shows a steady flat
/// line, the meter at rest (the cadence keeps running, its frames just
/// diff to nothing).
pub(crate) fn render_eq(frame: &mut Frame, area: Rect, wall_tick: usize, muted: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width as usize;
    let anim_frame = wall_tick / 2;

    let mut lines = Vec::with_capacity(area.height as usize);
    // Center the band in whatever height the stage gives us; a shorter
    // area clips the bottom rows (Paragraph drops overflow).
    for _ in 0..(area.height as usize).saturating_sub(EQ_ROWS) / 2 {
        lines.push(Line::from(""));
    }

    if muted {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "─".repeat(width),
            Style::default().fg(theme::AMBER_DIM()),
        )));
        lines.push(Line::from(""));
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let bars = width.div_ceil(BAR_STRIDE);
    let levels: Vec<u16> = (0..bars).map(|b| bar_level(b, bars, anim_frame)).collect();
    let caps: Vec<u16> = (0..bars).map(|b| cap_level(b, bars, anim_frame)).collect();

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
