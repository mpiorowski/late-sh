use serde::{Deserialize, Serialize};

use crate::canvas::{Canvas, Pos};
use crate::color::RgbColor;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanvasOp {
    PaintCell {
        pos: Pos,
        ch: char,
        fg: RgbColor,
    },
    ClearCell {
        pos: Pos,
    },
    PaintRegion {
        cells: Vec<CellWrite>,
    },
    ShiftRow {
        y: usize,
        kind: RowShift,
    },
    ShiftCol {
        x: usize,
        kind: ColShift,
    },
    /// Replace the entire canvas. Used for large structural edits (undo /
    /// redo, paste of big regions) where itemizing per-cell writes would be
    /// more expensive than just shipping a snapshot. Safe on SP; WS plan
    /// will want to avoid this path for high-frequency edits.
    Replace {
        canvas: Canvas,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellWrite {
    Paint { pos: Pos, ch: char, fg: RgbColor },
    Clear { pos: Pos },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RowShift {
    PushLeft { to_x: usize },
    PushRight { from_x: usize },
    PullFromLeft { to_x: usize },
    PullFromRight { from_x: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColShift {
    PushUp { to_y: usize },
    PushDown { from_y: usize },
    PullFromUp { to_y: usize },
    PullFromDown { from_y: usize },
}

impl Canvas {
    pub fn apply(&mut self, op: &CanvasOp) {
        match op {
            CanvasOp::PaintCell { pos, ch, fg } => {
                let _ = self.put_glyph_colored(*pos, *ch, *fg);
            }
            CanvasOp::ClearCell { pos } => self.clear_cell(*pos),
            CanvasOp::PaintRegion { cells } => {
                for write in cells {
                    match write {
                        CellWrite::Paint { pos, ch, fg } => {
                            let _ = self.put_glyph_colored(*pos, *ch, *fg);
                        }
                        CellWrite::Clear { pos } => self.clear_cell(*pos),
                    }
                }
            }
            CanvasOp::ShiftRow { y, kind } => match kind {
                RowShift::PushLeft { to_x } => self.push_left(*y, *to_x),
                RowShift::PushRight { from_x } => self.push_right(*y, *from_x),
                RowShift::PullFromLeft { to_x } => self.pull_from_left(*y, *to_x),
                RowShift::PullFromRight { from_x } => self.pull_from_right(*y, *from_x),
            },
            CanvasOp::ShiftCol { x, kind } => match kind {
                ColShift::PushUp { to_y } => self.push_up(*x, *to_y),
                ColShift::PushDown { from_y } => self.push_down(*x, *from_y),
                ColShift::PullFromUp { to_y } => self.pull_from_up(*x, *to_y),
                ColShift::PullFromDown { from_y } => self.pull_from_down(*x, *from_y),
            },
            CanvasOp::Replace { canvas } => *self = canvas.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn red() -> RgbColor {
        RgbColor::new(255, 0, 0)
    }

    #[test]
    fn paint_cell_op_writes_colored_glyph() {
        let mut canvas = Canvas::with_size(8, 4);
        canvas.apply(&CanvasOp::PaintCell {
            pos: Pos { x: 2, y: 1 },
            ch: 'A',
            fg: red(),
        });
        assert_eq!(canvas.get(Pos { x: 2, y: 1 }), 'A');
        assert_eq!(canvas.fg(Pos { x: 2, y: 1 }), Some(red()));
    }

    #[test]
    fn paint_region_applies_paint_and_clear_entries() {
        let mut canvas = Canvas::with_size(8, 4);
        canvas.set_colored(Pos { x: 0, y: 0 }, 'Q', red());
        canvas.apply(&CanvasOp::PaintRegion {
            cells: vec![
                CellWrite::Clear {
                    pos: Pos { x: 0, y: 0 },
                },
                CellWrite::Paint {
                    pos: Pos { x: 1, y: 0 },
                    ch: 'Z',
                    fg: red(),
                },
            ],
        });
        assert_eq!(canvas.get(Pos { x: 0, y: 0 }), ' ');
        assert_eq!(canvas.get(Pos { x: 1, y: 0 }), 'Z');
    }

    #[test]
    fn shift_row_dispatches_to_push_left() {
        let mut canvas = Canvas::with_size(8, 4);
        canvas.set(Pos { x: 0, y: 0 }, 'A');
        canvas.set(Pos { x: 1, y: 0 }, 'B');
        canvas.set(Pos { x: 2, y: 0 }, 'C');
        canvas.apply(&CanvasOp::ShiftRow {
            y: 0,
            kind: RowShift::PushLeft { to_x: 1 },
        });
        assert_eq!(canvas.get(Pos { x: 0, y: 0 }), 'B');
        assert_eq!(canvas.get(Pos { x: 1, y: 0 }), ' ');
        assert_eq!(canvas.get(Pos { x: 2, y: 0 }), 'C');
    }

    #[test]
    fn canvas_op_serde_roundtrip() {
        let op = CanvasOp::PaintRegion {
            cells: vec![CellWrite::Paint {
                pos: Pos { x: 1, y: 2 },
                ch: '🌱',
                fg: red(),
            }],
        };
        let j = serde_json::to_string(&op).unwrap();
        let back: CanvasOp = serde_json::from_str(&j).unwrap();
        assert_eq!(op, back);
    }
}
