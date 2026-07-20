use crate::app::common::theme;
use late_core::audio::VizFrame;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

const REAL_BAND_ATTACK: f32 = 0.62;
const REAL_BAND_RELEASE: f32 = 0.28;
const REAL_RMS_ATTACK: f32 = 0.5;
const REAL_RMS_RELEASE: f32 = 0.22;
const IDLE_BAND_DECAY: f32 = 0.94;

pub struct Visualizer {
    bands: [f32; 8],
    rms: f32,
    has_viz: bool,
    // Beat detection (volume-independent rhythm tracking)
    rms_avg: f32,
    beat: f32,
    // Procedural indicator (YouTube source — no real frequency data, cross-origin
    // iframe). Slow breathing sine wave. Does NOT pretend to be audio-reactive;
    // kept visually distinct (AMBER_DIM) so a glance separates it from real bars.
    // See CONTEXT.md §10 / §18.
    procedural_active: bool,
    procedural_phase: f32,
}

impl Default for Visualizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Visualizer {
    pub fn new() -> Self {
        Self {
            bands: [0.0; 8],
            rms: 0.0,
            has_viz: false,
            rms_avg: 0.0,
            beat: 0.0,
            procedural_active: false,
            procedural_phase: 0.0,
        }
    }

    pub fn update(&mut self, frame: &VizFrame) {
        let had_viz = self.has_viz;
        self.has_viz = true;
        let target_rms = frame.rms.clamp(0.0, 1.0);
        self.rms = if had_viz {
            Self::smooth_value(self.rms, target_rms, REAL_RMS_ATTACK, REAL_RMS_RELEASE)
        } else {
            target_rms
        };
        for (i, band) in frame.bands.iter().enumerate() {
            let target = band.clamp(0.0, 1.0);
            self.bands[i] = if had_viz {
                Self::smooth_value(self.bands[i], target, REAL_BAND_ATTACK, REAL_BAND_RELEASE)
            } else {
                target
            };
        }

        // Beat detection: a relative spike above the running average triggers
        // a beat regardless of absolute volume level.
        self.beat *= 0.9;
        if self.rms_avg > 0.001 && frame.rms / self.rms_avg > 1.3 {
            self.beat = 1.0;
        }
        self.rms_avg = self.rms_avg * 0.95 + frame.rms * 0.05;
    }

    pub fn rms(&self) -> f32 {
        self.rms
    }

    /// Volume-independent beat intensity (0..1), decays after each detected beat.
    pub fn beat(&self) -> f32 {
        self.beat
    }

    pub fn tick_idle(&mut self) {
        if !self.has_viz {
            return;
        }
        self.rms = (self.rms * 0.96).max(0.0);
        for band in &mut self.bands {
            *band = (*band * IDLE_BAND_DECAY).max(0.0);
        }
        self.beat = (self.beat * 0.9).max(0.0);
    }

    pub fn set_procedural_active(&mut self, active: bool) {
        self.procedural_active = active;
    }

    pub fn tick_procedural(&mut self) {
        if !self.procedural_active {
            return;
        }
        self.procedural_phase += 0.08;
        if self.procedural_phase > std::f32::consts::TAU * 1024.0 {
            self.procedural_phase -= std::f32::consts::TAU * 1024.0;
        }
    }

    fn procedural_bands(&self) -> [f32; 8] {
        // Layered sines: a primary traveling wave, a faster shimmer offset
        // per-band, and a slow global breath. The phases multiply (1.0, 1.7,
        // 0.35) are deliberately incommensurate so the pattern doesn't repeat
        // visibly inside a few seconds.
        let mut out = [0.0f32; 8];
        let breath = 0.05 * (self.procedural_phase * 0.35).sin();
        for (i, slot) in out.iter_mut().enumerate() {
            let p = self.procedural_phase + (i as f32) * 0.55;
            let primary = 0.20 * p.sin();
            let shimmer = 0.07 * (p * 1.7 + (i as f32) * 0.31).sin();
            *slot = (0.5 + primary + shimmer + breath).clamp(0.05, 0.95);
        }
        out
    }

    fn vertical_block(fill: f32) -> &'static str {
        // 9-step sub-cell vertical block: ' ' through '█' in 1/8 increments.
        const BLOCKS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
        let idx = (fill.clamp(0.0, 1.0) * 8.0).round() as usize;
        BLOCKS[idx.min(8)]
    }

    fn smooth_value(current: f32, target: f32, attack: f32, release: f32) -> f32 {
        let factor = if target > current { attack } else { release };
        (current + (target - current) * factor).clamp(0.0, 1.0)
    }

    /// Borderless visualizer for the merged shell. Renders bars only when
    /// audio is paired; otherwise shows a "no audio" hint plus the guide
    /// pairing hint. No block, no title — the rail's whitespace
    /// owns the separation.
    pub fn render_inline(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 || area.width == 0 {
            return;
        }
        if self.procedural_active {
            let lines = self.build_procedural_lines(area);
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
        if !self.has_viz {
            let faint = Style::default().fg(theme::TEXT_FAINT());
            let amber_italic = Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::ITALIC);
            let amber_key = Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD);

            let mut lines: Vec<Line<'static>> = Vec::with_capacity(area.height as usize);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("no audio paired", faint)));
            lines.push(Line::from(vec![
                Span::styled("? guide", amber_italic),
                Span::styled(" pair", faint),
            ]));
            if area.height >= 5 {
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("v+x", amber_key),
                    Span::styled(" source", faint),
                ]));
            }
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
        let lines = self.build_lines(area);
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn build_lines(&self, area: Rect) -> Vec<Line<'static>> {
        let height = area.height as usize;
        let width = area.width as usize;
        if height == 0 || width == 0 {
            return Vec::new();
        }

        // Thin bars with small gaps: n bars + (n-1) gaps = width
        // So 2n - 1 = width, n = (width + 1) / 2
        let band_count = width.div_ceil(2).max(1);
        let gap = 1usize;

        let mut bands = self.resample(&self.bands, band_count);
        let len = bands.len();
        for (i, band) in bands.iter_mut().enumerate() {
            *band = Self::tilt(*band, i, len);
        }

        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let cell_from_bottom = (height - row - 1) as f32;
            let mut spans: Vec<Span> = Vec::with_capacity(band_count * 2);

            for (i, &band) in bands.iter().enumerate().take(band_count) {
                let band = band.clamp(0.0, 1.0);
                let bar_height_cells = band * height as f32;
                let fill = (bar_height_cells - cell_from_bottom).clamp(0.0, 1.0);

                if fill <= 0.0 {
                    spans.push(Span::raw(" "));
                } else {
                    spans.push(Span::styled(
                        Self::vertical_block(fill),
                        Self::real_bar_style(fill, band),
                    ));
                }
                if gap > 0 && i + 1 < band_count {
                    spans.push(Span::raw(" ".repeat(gap)));
                }
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    fn real_bar_style(fill: f32, band: f32) -> Style {
        if band > 0.78 && fill > 0.5 {
            Style::default().fg(theme::AMBER_GLOW())
        } else if fill < 0.5 || band < 0.35 {
            Style::default().fg(theme::AMBER_DIM())
        } else {
            Style::default().fg(theme::AMBER())
        }
    }

    fn build_procedural_lines(&self, area: Rect) -> Vec<Line<'static>> {
        let height = area.height as usize;
        let width = area.width as usize;
        if height == 0 || width == 0 {
            return Vec::new();
        }

        let band_count = width.div_ceil(2).max(1);
        let gap = 1usize;

        // No tilt — the procedural pattern is decorative, not a frequency
        // spectrum. Tilting it would lean the whole wave to one side and read
        // as broken rather than stylized.
        let source = self.procedural_bands();
        let bands = self.resample(&source, band_count);
        let style = Style::default().fg(theme::AMBER_DIM());

        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            // Vertical cell index measured from the bottom (0 = bottom row).
            let cell_from_bottom = (height - row - 1) as f32;
            let mut spans: Vec<Span> = Vec::with_capacity(band_count * 2);

            for (i, &band) in bands.iter().enumerate().take(band_count) {
                let bar_height_cells = band.clamp(0.0, 1.0) * height as f32;
                // How much of THIS cell is filled by the bar (0..1).
                let fill = (bar_height_cells - cell_from_bottom).clamp(0.0, 1.0);

                if fill <= 0.0 {
                    spans.push(Span::raw(" "));
                } else {
                    spans.push(Span::styled(Self::vertical_block(fill), style));
                }

                if gap > 0 && i + 1 < band_count {
                    spans.push(Span::raw(" ".repeat(gap)));
                }
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    fn resample(&self, input: &[f32], target: usize) -> Vec<f32> {
        if input.is_empty() || target == 0 {
            return Vec::new();
        }
        if target == input.len() {
            return input.to_vec();
        }
        let max_index = (input.len() - 1) as f32;
        let mut out = Vec::with_capacity(target);
        for i in 0..target {
            let t = if target == 1 {
                0.0
            } else {
                i as f32 / (target - 1) as f32
            };
            let pos = t * max_index;
            let left = pos.floor() as usize;
            let right = pos.ceil() as usize;
            if left == right {
                out.push(input[left]);
            } else {
                let frac = pos - left as f32;
                out.push(input[left] + (input[right] - input[left]) * frac);
            }
        }
        out
    }

    fn tilt(value: f32, index: usize, count: usize) -> f32 {
        if count <= 1 {
            return value.clamp(0.0, 1.0);
        }
        let t = index as f32 / (count - 1) as f32;
        let weight = 0.65 + 0.35 * t;
        (value.clamp(0.0, 1.0) * weight).powf(1.1)
    }
}

#[cfg(test)]
#[path = "viz_test.rs"]
mod viz_test;

