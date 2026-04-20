//! Reusable ratatui widget that renders a dartboard `Canvas`.
//!
//! `CanvasWidget` borrows a `CanvasWidgetState` and draws the canvas cells,
//! optional selection overlay, and optional floating-selection overlay into a
//! ratatui buffer. The widget carries only styling; per-session view data
//! lives in the state. Each consumer (standalone app, late-sh integration,
//! etc.) builds its own state per render, similar to how ratatui's
//! `Paragraph::new(text).style(...)` works.

use dartboard_core::{Canvas, CellValue, Pos, RgbColor};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::widgets::Widget;

/// Styling hooks for `CanvasWidget`. Each consumer supplies colors from its
/// own theme. Defaults are sensible for a dark terminal background.
#[derive(Debug, Clone, Copy)]
pub struct CanvasStyle {
    /// Background color painted for cells outside the canvas bounds.
    pub oob_bg: Color,
    /// Fallback foreground when a cell has no explicit `fg`.
    pub default_glyph_fg: Color,
    /// Background color for cells inside an active selection.
    pub selection_bg: Color,
    /// Foreground color for cells inside an active selection.
    pub selection_fg: Color,
    /// Background color for cells covered by a floating selection.
    pub floating_bg: Color,
}

impl Default for CanvasStyle {
    fn default() -> Self {
        Self {
            oob_bg: Color::Rgb(16, 16, 16),
            default_glyph_fg: Color::Rgb(136, 128, 120),
            selection_bg: Color::Rgb(64, 40, 24),
            selection_fg: Color::Rgb(208, 166, 89),
            floating_bg: Color::Rgb(32, 48, 64),
        }
    }
}

/// Selection shape as it appears to the renderer. Mirrors the shape used by
/// standalone dartboard; kept here (rather than in `dartboard-core`) because
/// the shape is strictly about how selection is drawn, not how it's stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionShape {
    Rect,
    Ellipse,
}

/// View of an in-flight selection. `anchor` is the fixed corner, `cursor` is
/// the moving corner.
#[derive(Debug, Clone, Copy)]
pub struct SelectionView {
    pub anchor: Pos,
    pub cursor: Pos,
    pub shape: SelectionShape,
}

impl SelectionView {
    /// Bounding rectangle (min/max inclusive) of the selection.
    fn bounds(self) -> ((usize, usize), (usize, usize)) {
        let min_x = self.anchor.x.min(self.cursor.x);
        let max_x = self.anchor.x.max(self.cursor.x);
        let min_y = self.anchor.y.min(self.cursor.y);
        let max_y = self.anchor.y.max(self.cursor.y);
        ((min_x, min_y), (max_x, max_y))
    }

    /// Whether `pos` falls inside the selected region.
    pub fn contains(&self, pos: Pos) -> bool {
        let ((min_x, min_y), (max_x, max_y)) = self.bounds();
        if pos.x < min_x || pos.x > max_x || pos.y < min_y || pos.y > max_y {
            return false;
        }
        match self.shape {
            SelectionShape::Rect => true,
            SelectionShape::Ellipse => {
                let w = max_x - min_x + 1;
                let h = max_y - min_y + 1;
                if w <= 1 || h <= 1 {
                    return true;
                }
                let px = pos.x as f64 + 0.5;
                let py = pos.y as f64 + 0.5;
                let cx = (min_x + max_x + 1) as f64 / 2.0;
                let cy = (min_y + max_y + 1) as f64 / 2.0;
                let rx = w as f64 / 2.0;
                let ry = h as f64 / 2.0;
                let dx = (px - cx) / rx;
                let dy = (py - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
        }
    }
}

/// View of a floating selection pinned to `anchor`. Consumers pass a flat
/// cells slice of length `width * height` (row-major). `None` entries are
/// rendered as background in opaque mode and skipped in transparent mode.
#[derive(Debug, Clone, Copy)]
pub struct FloatingView<'a> {
    pub width: usize,
    pub height: usize,
    pub cells: &'a [Option<CellValue>],
    pub anchor: Pos,
    pub transparent: bool,
    pub active_color: RgbColor,
}

impl<'a> FloatingView<'a> {
    fn cell(&self, cx: usize, cy: usize) -> Option<CellValue> {
        self.cells[cy * self.width + cx]
    }
}

/// Per-render data the widget reads. The `canvas` reference and optional
/// selection/floating views are what make each session's view distinct.
#[derive(Debug)]
pub struct CanvasWidgetState<'a> {
    pub canvas: &'a Canvas,
    pub viewport_origin: Pos,
    pub selection: Option<SelectionView>,
    pub floating: Option<FloatingView<'a>>,
}

impl<'a> CanvasWidgetState<'a> {
    pub fn new(canvas: &'a Canvas, viewport_origin: Pos) -> Self {
        Self {
            canvas,
            viewport_origin,
            selection: None,
            floating: None,
        }
    }

    pub fn selection(mut self, selection: SelectionView) -> Self {
        self.selection = Some(selection);
        self
    }

    pub fn floating(mut self, floating: FloatingView<'a>) -> Self {
        self.floating = Some(floating);
        self
    }
}

/// Widget that renders the canvas + overlays.
pub struct CanvasWidget<'a> {
    state: &'a CanvasWidgetState<'a>,
    style: CanvasStyle,
}

impl<'a> CanvasWidget<'a> {
    pub fn new(state: &'a CanvasWidgetState<'a>) -> Self {
        Self {
            state,
            style: CanvasStyle::default(),
        }
    }

    pub fn style(mut self, style: CanvasStyle) -> Self {
        self.style = style;
        self
    }
}

impl<'a> Widget for CanvasWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let canvas = self.state.canvas;
        let cw = canvas.width;
        let ch = canvas.height;
        let ox = self.state.viewport_origin.x;
        let oy = self.state.viewport_origin.y;
        let selection = self.state.selection;

        for dy in 0..area.height {
            for dx in 0..area.width {
                let x = ox + dx as usize;
                let y = oy + dy as usize;
                let cell = &mut buf[(area.x + dx, area.y + dy)];

                if x >= cw || y >= ch {
                    cell.set_bg(self.style.oob_bg);
                    continue;
                }

                let pos = Pos { x, y };
                let cell_value = canvas.cell(pos);
                let glyph_fg = canvas
                    .fg(pos)
                    .map(rgb_to_color)
                    .unwrap_or(self.style.default_glyph_fg);

                if selection.map(|s| s.contains(pos)).unwrap_or(false) {
                    cell.set_bg(self.style.selection_bg)
                        .set_fg(self.style.selection_fg);
                    if let Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) = cell_value {
                        cell.set_char(ch);
                    }
                } else if let Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) = cell_value {
                    cell.set_char(ch).set_fg(glyph_fg);
                }
            }
        }

        if let Some(floating) = self.state.floating {
            let active_fg = rgb_to_color(floating.active_color);
            for cy in 0..floating.height {
                for cx in 0..floating.width {
                    let canvas_x = floating.anchor.x + cx;
                    let canvas_y = floating.anchor.y + cy;

                    if canvas_x >= cw || canvas_y >= ch || canvas_x < ox || canvas_y < oy {
                        continue;
                    }

                    let dx = (canvas_x - ox) as u16;
                    let dy = (canvas_y - oy) as u16;
                    if dx >= area.width || dy >= area.height {
                        continue;
                    }

                    let cell = &mut buf[(area.x + dx, area.y + dy)];
                    match floating.cell(cx, cy) {
                        Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) => {
                            cell.set_char(ch)
                                .set_bg(self.style.floating_bg)
                                .set_fg(active_fg);
                        }
                        Some(CellValue::WideCont) => {
                            cell.set_bg(self.style.floating_bg);
                        }
                        None if !floating.transparent => {
                            cell.set_char(' ').set_bg(self.style.floating_bg);
                        }
                        None => {}
                    }
                }
            }
        }
    }
}

fn rgb_to_color(c: RgbColor) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartboard_core::{Canvas, CanvasOp, Pos, RgbColor};
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::widgets::Widget;

    fn blank_canvas(width: usize, height: usize) -> Canvas {
        Canvas::with_size(width, height)
    }

    #[test]
    fn renders_empty_canvas_without_panic() {
        let canvas = blank_canvas(4, 3);
        let state = CanvasWidgetState::new(&canvas, Pos { x: 0, y: 0 });
        let widget = CanvasWidget::new(&state);
        let area = Rect::new(0, 0, 4, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
    }

    #[test]
    fn renders_painted_cell_with_its_color() {
        let mut canvas = blank_canvas(4, 2);
        canvas.apply(&CanvasOp::PaintCell {
            pos: Pos { x: 1, y: 0 },
            ch: 'X',
            fg: RgbColor::new(200, 100, 50),
        });
        let state = CanvasWidgetState::new(&canvas, Pos { x: 0, y: 0 });
        let widget = CanvasWidget::new(&state);
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let cell = &buf[(1, 0)];
        assert_eq!(cell.symbol(), "X");
        assert_eq!(cell.fg, Color::Rgb(200, 100, 50));
    }

    #[test]
    fn out_of_bounds_area_gets_oob_bg() {
        let canvas = blank_canvas(2, 2);
        let state = CanvasWidgetState::new(&canvas, Pos { x: 0, y: 0 });
        let widget = CanvasWidget::new(&state);
        let area = Rect::new(0, 0, 4, 3);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        // Cell (3, 2) is outside the 2x2 canvas — should be OOB_BG.
        assert_eq!(buf[(3, 2)].bg, CanvasStyle::default().oob_bg);
    }

    #[test]
    fn selection_rect_highlights_bounded_cells() {
        let canvas = blank_canvas(5, 5);
        let state = CanvasWidgetState::new(&canvas, Pos { x: 0, y: 0 }).selection(SelectionView {
            anchor: Pos { x: 1, y: 1 },
            cursor: Pos { x: 2, y: 2 },
            shape: SelectionShape::Rect,
        });
        let widget = CanvasWidget::new(&state);
        let area = Rect::new(0, 0, 5, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        let style = CanvasStyle::default();
        assert_eq!(buf[(1, 1)].bg, style.selection_bg);
        assert_eq!(buf[(2, 2)].bg, style.selection_bg);
        assert_ne!(buf[(0, 0)].bg, style.selection_bg);
        assert_ne!(buf[(3, 3)].bg, style.selection_bg);
    }

    #[test]
    fn floating_view_stamps_cells_at_anchor() {
        let canvas = blank_canvas(5, 5);
        let cells = vec![
            Some(CellValue::Narrow('A')),
            None,
            Some(CellValue::Narrow('B')),
        ];
        let state = CanvasWidgetState::new(&canvas, Pos { x: 0, y: 0 }).floating(FloatingView {
            width: 3,
            height: 1,
            cells: &cells,
            anchor: Pos { x: 1, y: 0 },
            transparent: true,
            active_color: RgbColor::new(255, 0, 0),
        });
        let widget = CanvasWidget::new(&state);
        let area = Rect::new(0, 0, 5, 5);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);

        assert_eq!(buf[(1, 0)].symbol(), "A");
        assert_eq!(buf[(3, 0)].symbol(), "B");
    }
}
