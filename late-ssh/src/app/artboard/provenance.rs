use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use dartboard_core::{Canvas, CanvasOp, CellValue, ColShift, Pos, RowShift};
use late_core::MutexRecover;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "ArtboardProvenanceWire", from = "ArtboardProvenanceWire")]
pub struct ArtboardProvenance {
    cells: HashMap<Pos, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct ArtboardProvenanceWire {
    cells: Vec<(Pos, String)>,
}

impl From<ArtboardProvenance> for ArtboardProvenanceWire {
    fn from(value: ArtboardProvenance) -> Self {
        let mut cells: Vec<_> = value.cells.into_iter().collect();
        cells.sort_by_key(|(pos, _)| (pos.y, pos.x));
        Self { cells }
    }
}

impl From<ArtboardProvenanceWire> for ArtboardProvenance {
    fn from(value: ArtboardProvenanceWire) -> Self {
        Self {
            cells: value.cells.into_iter().collect(),
        }
    }
}

pub type SharedArtboardProvenance = Arc<Mutex<ArtboardProvenance>>;

impl ArtboardProvenance {
    pub fn shared(self) -> SharedArtboardProvenance {
        Arc::new(Mutex::new(self))
    }

    pub fn username_at<'a>(&'a self, canvas: &Canvas, pos: Pos) -> Option<&'a str> {
        let origin = canvas.glyph_origin(pos)?;
        self.cells.get(&origin).map(String::as_str)
    }

    pub fn set_username(&mut self, pos: Pos, username: impl Into<String>) {
        self.cells.insert(pos, username.into());
    }

    pub fn apply_op(&mut self, before: &Canvas, op: &CanvasOp, username: &str) {
        match op {
            CanvasOp::PaintCell { pos, ch, .. } => {
                let mut canvas = before.clone();
                self.paint_on(&mut canvas, *pos, *ch, username);
            }
            CanvasOp::ClearCell { pos } => {
                let mut canvas = before.clone();
                self.clear_on(&mut canvas, *pos);
            }
            CanvasOp::PaintRegion { cells } => {
                let mut canvas = before.clone();
                for write in cells {
                    match write {
                        dartboard_core::CellWrite::Paint { pos, ch, .. } => {
                            self.paint_on(&mut canvas, *pos, *ch, username);
                        }
                        dartboard_core::CellWrite::Clear { pos } => {
                            self.clear_on(&mut canvas, *pos)
                        }
                    }
                }
            }
            CanvasOp::ShiftRow { y, kind } => self.shift_row(before, *y, *kind),
            CanvasOp::ShiftCol { x, kind } => self.shift_col(before, *x, *kind),
            CanvasOp::Replace { canvas } => self.replace_from(before, canvas, username),
        }
    }

    fn paint_on(&mut self, canvas: &mut Canvas, pos: Pos, ch: char, username: &str) {
        self.clear_glyph_at(canvas, pos);
        if Canvas::display_width(ch) == 2 && pos.x + 1 < canvas.width {
            self.clear_glyph_at(
                canvas,
                Pos {
                    x: pos.x + 1,
                    y: pos.y,
                },
            );
        }
        let _ = canvas.put_glyph(pos, ch);
        if ch == ' ' {
            return;
        }
        self.cells.insert(pos, username.to_string());
    }

    fn clear_on(&mut self, canvas: &mut Canvas, pos: Pos) {
        self.clear_glyph_at(canvas, pos);
        canvas.clear_cell(pos);
    }

    fn clear_glyph_at(&mut self, canvas: &Canvas, pos: Pos) {
        if let Some(origin) = canvas.glyph_origin(pos) {
            self.cells.remove(&origin);
        }
    }

    fn shift_row(&mut self, before: &Canvas, y: usize, kind: RowShift) {
        let mut glyphs = self.glyph_authors(before);
        for glyph in &mut glyphs {
            if glyph.pos.y != y {
                continue;
            }
            match kind {
                RowShift::PushLeft { to_x } if glyph.pos.x <= to_x => {
                    if glyph.pos.x == 0 {
                        glyph.alive = false;
                    } else {
                        glyph.pos.x -= 1;
                    }
                }
                RowShift::PushRight { from_x } if glyph.pos.x >= from_x => glyph.pos.x += 1,
                RowShift::PullFromLeft { to_x } => {
                    let remove_origin = before.glyph_origin(Pos { x: to_x, y });
                    if Some(glyph.origin) == remove_origin {
                        glyph.alive = false;
                    } else if glyph.pos.x < to_x {
                        glyph.pos.x += 1;
                    }
                }
                RowShift::PullFromRight { from_x } => {
                    let remove_origin = before.glyph_origin(Pos { x: from_x, y });
                    if Some(glyph.origin) == remove_origin {
                        glyph.alive = false;
                    } else if glyph.pos.x > from_x {
                        glyph.pos.x -= 1;
                    }
                }
                _ => {}
            }
        }
        self.rebuild_from_glyphs(glyphs);
    }

    fn shift_col(&mut self, before: &Canvas, x: usize, kind: ColShift) {
        let mut glyphs = self.glyph_authors(before);
        for glyph in &mut glyphs {
            let covers_x = x >= glyph.pos.x && x < glyph.pos.x + glyph.width;
            if !covers_x {
                continue;
            }
            match kind {
                ColShift::PushUp { to_y } if glyph.pos.y <= to_y => {
                    if glyph.pos.y == 0 {
                        glyph.alive = false;
                    } else {
                        glyph.pos.y -= 1;
                    }
                }
                ColShift::PushDown { from_y } if glyph.pos.y >= from_y => glyph.pos.y += 1,
                ColShift::PullFromUp { to_y } => {
                    let remove_origin = before.glyph_origin(Pos { x, y: to_y });
                    if Some(glyph.origin) == remove_origin {
                        glyph.alive = false;
                    } else if glyph.pos.y < to_y {
                        glyph.pos.y += 1;
                    }
                }
                ColShift::PullFromDown { from_y } => {
                    let remove_origin = before.glyph_origin(Pos { x, y: from_y });
                    if Some(glyph.origin) == remove_origin {
                        glyph.alive = false;
                    } else if glyph.pos.y > from_y {
                        glyph.pos.y -= 1;
                    }
                }
                _ => {}
            }
        }
        self.rebuild_from_glyphs(glyphs);
    }

    fn replace_from(&mut self, before: &Canvas, after: &Canvas, username: &str) {
        let mut next = HashMap::new();
        for (pos, cell) in after.iter() {
            let (ch, width) = match cell {
                CellValue::Narrow(ch) => (*ch, 1),
                CellValue::Wide(ch) => (*ch, 2),
                CellValue::WideCont => continue,
            };
            let keep_existing = before.glyph_at(*pos).zip(after.glyph_at(*pos)).is_some_and(
                |(before_glyph, after_glyph)| {
                    before_glyph.ch == ch
                        && before_glyph.width == width
                        && before_glyph.fg == after_glyph.fg
                },
            );
            if keep_existing {
                if let Some(existing) = self.cells.get(pos).cloned() {
                    next.insert(*pos, existing);
                }
            } else {
                next.insert(*pos, username.to_string());
            }
        }
        self.cells = next;
    }

    fn glyph_authors(&self, canvas: &Canvas) -> Vec<AuthoredGlyph> {
        let mut glyphs = Vec::new();
        for (pos, cell) in canvas.iter() {
            let (width, username) = match cell {
                CellValue::Narrow(_) => (1, self.cells.get(pos).cloned()),
                CellValue::Wide(_) => (2, self.cells.get(pos).cloned()),
                CellValue::WideCont => continue,
            };
            let Some(username) = username else { continue };
            glyphs.push(AuthoredGlyph {
                origin: *pos,
                pos: *pos,
                width,
                username,
                alive: true,
            });
        }
        glyphs.sort_by_key(|glyph| (glyph.pos.y, glyph.pos.x));
        glyphs
    }

    fn rebuild_from_glyphs(&mut self, glyphs: Vec<AuthoredGlyph>) {
        self.cells.clear();
        for glyph in glyphs {
            if glyph.alive {
                self.cells.insert(glyph.pos, glyph.username);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AuthoredGlyph {
    origin: Pos,
    pos: Pos,
    width: usize,
    username: String,
    alive: bool,
}

pub fn clone_shared_provenance(shared: &SharedArtboardProvenance) -> ArtboardProvenance {
    shared.lock_recover().clone()
}

pub fn apply_shared_op(
    shared: &SharedArtboardProvenance,
    before: &Canvas,
    op: &CanvasOp,
    username: &str,
) {
    shared.lock_recover().apply_op(before, op, username);
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartboard_core::{CanvasOp, RgbColor};

    #[test]
    fn paint_cell_tracks_last_writer() {
        let mut provenance = ArtboardProvenance::default();
        let before = Canvas::with_size(8, 4);

        provenance.apply_op(
            &before,
            &CanvasOp::PaintCell {
                pos: Pos { x: 2, y: 1 },
                ch: 'A',
                fg: RgbColor::new(1, 2, 3),
            },
            "mat",
        );

        let mut after = before.clone();
        after.set(Pos { x: 2, y: 1 }, 'A');
        assert_eq!(
            provenance.username_at(&after, Pos { x: 2, y: 1 }),
            Some("mat")
        );
    }

    #[test]
    fn clear_cell_removes_last_writer() {
        let mut provenance = ArtboardProvenance::default();
        let mut before = Canvas::with_size(8, 4);
        before.set(Pos { x: 2, y: 1 }, 'A');
        provenance.set_username(Pos { x: 2, y: 1 }, "mat");

        provenance.apply_op(
            &before,
            &CanvasOp::ClearCell {
                pos: Pos { x: 2, y: 1 },
            },
            "mat",
        );

        let mut after = before.clone();
        after.clear(Pos { x: 2, y: 1 });
        assert_eq!(provenance.username_at(&after, Pos { x: 2, y: 1 }), None);
    }

    #[test]
    fn replace_preserves_unchanged_authors_and_retags_changed_cells() {
        let mut provenance = ArtboardProvenance::default();
        let mut before = Canvas::with_size(8, 4);
        before.set(Pos { x: 1, y: 1 }, 'A');
        before.set(Pos { x: 2, y: 1 }, 'B');
        provenance.set_username(Pos { x: 1, y: 1 }, "alice");
        provenance.set_username(Pos { x: 2, y: 1 }, "bob");

        let mut after = before.clone();
        after.set(Pos { x: 2, y: 1 }, 'C');

        provenance.apply_op(
            &before,
            &CanvasOp::Replace {
                canvas: after.clone(),
            },
            "carol",
        );

        assert_eq!(
            provenance.username_at(&after, Pos { x: 1, y: 1 }),
            Some("alice")
        );
        assert_eq!(
            provenance.username_at(&after, Pos { x: 2, y: 1 }),
            Some("carol")
        );
    }
}
