use crate::app::common::theme;
use late_core::audio::VizFrame;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub struct Visualizer {
    bands: [f32; 8],
    rms: f32,
    has_viz: bool,
    // Beat detection (volume-independent rhythm tracking)
    rms_avg: f32,
    beat: f32,
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
        }
    }

    pub fn update(&mut self, frame: &VizFrame) {
        self.has_viz = true;
        self.rms = frame.rms;
        for (i, band) in frame.bands.iter().enumerate() {
            self.bands[i] = band.clamp(0.0, 1.0);
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
        self.beat = (self.beat * 0.9).max(0.0);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let border = if self.has_viz {
            theme::BORDER_ACTIVE()
        } else {
            theme::BORDER()
        };

        let block = Block::default()
            .title(" Visualizer ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        if !self.has_viz {
            let dim = Style::default().fg(theme::TEXT_DIM());
            let key = Style::default().fg(theme::AMBER());
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled("No audio paired", dim)),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Type ", dim),
                    Span::styled("/music", key),
                    Span::styled(" in chat", dim),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Enter", key),
                    Span::styled(" cli  ", dim),
                    Span::styled("p", key),
                    Span::styled(" web", dim),
                ]),
            ];
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        let lines = self.build_lines(inner);
        frame.render_widget(Paragraph::new(lines), inner);
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
        let band_width = 1usize;
        let gap = 1usize;

        let mut bands = self.resample(&self.bands, band_count);
        let len = bands.len();
        for (i, band) in bands.iter_mut().enumerate() {
            *band = Self::tilt(*band, i, len);
        }

        let mut lines = Vec::with_capacity(height);
        for row in 0..height {
            let level = height - row;
            let mut spans: Vec<Span> = Vec::with_capacity(band_count * 2);

            for (i, &band) in bands.iter().enumerate().take(band_count) {
                let band = band.clamp(0.0, 1.0);
                let bar_height = (band * height as f32).floor() as usize;
                let bar_height = bar_height.min(height);
                let filled = level <= bar_height;

                let (ch, style) = if filled {
                    ('█', Style::default().fg(theme::AMBER()))
                } else {
                    (' ', Style::default())
                };

                spans.push(Span::styled(ch.to_string().repeat(band_width), style));
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
mod tests {
    use super::*;

    #[test]
    fn resample_same_size() {
        let viz = Visualizer::new();
        let input = vec![1.0, 2.0, 3.0];
        let result = viz.resample(&input, 3);
        assert_eq!(result, input);
    }

    #[test]
    fn resample_upsample() {
        let viz = Visualizer::new();
        let input = vec![0.0, 1.0];
        let result = viz.resample(&input, 3);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[2], 1.0);
        assert!((result[1] - 0.5).abs() < 0.001);
    }

    #[test]
    fn resample_downsample() {
        let viz = Visualizer::new();
        let input = vec![0.0, 0.5, 1.0];
        let result = viz.resample(&input, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], 0.0);
        assert_eq!(result[1], 1.0);
    }

    #[test]
    fn resample_empty() {
        let viz = Visualizer::new();
        let result = viz.resample(&[], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn resample_zero_target() {
        let viz = Visualizer::new();
        let result = viz.resample(&[1.0, 2.0], 0);
        assert!(result.is_empty());
    }

    #[test]
    fn tilt_clamps_output() {
        let result = Visualizer::tilt(2.0, 0, 8);
        assert!(result <= 1.0);
    }

    #[test]
    fn tilt_single_element() {
        let result = Visualizer::tilt(0.5, 0, 1);
        assert!((0.0..=1.0).contains(&result));
    }

    #[test]
    fn tilt_increases_with_index() {
        let low = Visualizer::tilt(0.5, 0, 8);
        let high = Visualizer::tilt(0.5, 7, 8);
        assert!(high > low);
    }

    #[test]
    fn tick_idle_decays_rms() {
        let mut viz = Visualizer::new();
        viz.has_viz = true;
        viz.rms = 1.0;
        viz.tick_idle();
        assert!(viz.rms < 1.0);
        assert!(viz.rms > 0.0);
    }

    #[test]
    fn tick_idle_no_op_without_viz() {
        let mut viz = Visualizer::new();
        viz.rms = 1.0;
        viz.tick_idle();
        assert_eq!(viz.rms, 1.0); // unchanged because has_viz is false
    }
}
