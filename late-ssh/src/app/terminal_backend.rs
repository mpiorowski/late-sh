//! The SSH terminal backend: ratatui's crossterm backend with one change in
//! `draw`, so a glyph the client's terminal sizes differently from
//! unicode-width can never shift the rest of its row.
//!
//! ratatui writes a row as one contiguous run and only repositions the
//! cursor when a cell is not exactly one column past the previous one. The
//! server sizes graphemes with unicode-width (a ZWJ flag is 2, a VS16 emoji
//! is 2); a terminal doing per-codepoint wcwidth draws them as 1, 3, or 4,
//! and the server has no way to know which. Every cell written after the
//! disagreement lands in the wrong column, and the diff never rewrites what
//! it believes are unchanged padding cells, so the stray tail stays on the
//! client until Ctrl+L.
//!
//! This backend never trusts contiguity across a cell that is not a single
//! ASCII byte: the cell after it gets an absolute cursor move, and a cell the
//! model considers wide is blanked first so a narrower rendering leaves a
//! styled space rather than whatever was there. The terminal's error stays
//! inside the glyph's own cells. The cost is one cursor move (under ten
//! bytes) per non-ASCII cell in a diff.

use std::io::{self, Write};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{
        Attribute as CrosstermAttribute, Color as CrosstermColor, Colors as CrosstermColors, Print,
        SetAttribute, SetBackgroundColor, SetColors, SetForegroundColor, SetUnderlineColor,
    },
};
use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend, IntoCrossterm, WindowSize},
    buffer::{Cell, CellWidth},
    layout::{Position, Size},
    style::{Color, Modifier},
};

pub(super) struct GlyphIsolatingBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> GlyphIsolatingBackend<W> {
    pub(super) const fn new(writer: W) -> Self {
        Self {
            inner: CrosstermBackend::new(writer),
        }
    }
}

/// The only symbol whose width every terminal agrees on: one printable ASCII
/// byte. Everything else (box drawing, accented letters, CJK, emoji and its
/// sequences) is written with the cursor re-anchored afterwards.
fn is_plain_ascii(cell: &Cell) -> bool {
    cell.symbol().len() == 1
}

impl<W: Write> Backend for GlyphIsolatingBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        let mut underline_color = Color::Reset;
        let mut modifier = Modifier::empty();
        // Where the client's cursor is known to sit, or `None` once a
        // non-ASCII glyph left it wherever that terminal decided.
        let mut cursor: Option<Position> = None;
        for (x, y, cell) in content {
            let contiguous = matches!(cursor, Some(p) if p.x == x && p.y == y);
            if !contiguous {
                queue!(self.inner, MoveTo(x, y))?;
            }
            if cell.modifier != modifier {
                queue_modifier_diff(&mut self.inner, modifier, cell.modifier)?;
                modifier = cell.modifier;
            }
            if cell.fg != fg || cell.bg != bg {
                queue!(
                    self.inner,
                    SetColors(CrosstermColors::new(
                        cell.fg.into_crossterm(),
                        cell.bg.into_crossterm(),
                    ))
                )?;
                fg = cell.fg;
                bg = cell.bg;
            }
            if cell.underline_color != underline_color {
                queue!(
                    self.inner,
                    SetUnderlineColor(cell.underline_color.into_crossterm())
                )?;
                underline_color = cell.underline_color;
            }

            if is_plain_ascii(cell) {
                queue!(self.inner, Print(cell.symbol()))?;
                cursor = Some(Position {
                    x: x.saturating_add(1),
                    y,
                });
                continue;
            }
            let width = usize::from(cell.cell_width());
            if width > 1 {
                // Blank every cell the model reserves for the glyph, in the
                // glyph's own style, then draw it from the left edge again.
                queue!(self.inner, Print(" ".repeat(width)), MoveTo(x, y))?;
            }
            queue!(self.inner, Print(cell.symbol()))?;
            cursor = None;
        }

        queue!(
            self.inner,
            SetForegroundColor(CrosstermColor::Reset),
            SetBackgroundColor(CrosstermColor::Reset),
            SetUnderlineColor(CrosstermColor::Reset),
            SetAttribute(CrosstermAttribute::Reset),
        )
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        self.inner.append_lines(n)
    }

    fn size(&self) -> io::Result<Size> {
        self.inner.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

/// The attribute changes that take a terminal from `from` to `to`; the same
/// sequence ratatui's crossterm backend emits, which keeps its `ModifierDiff`
/// private.
fn queue_modifier_diff<W: Write>(w: &mut W, from: Modifier, to: Modifier) -> io::Result<()> {
    let removed = from - to;
    if removed.contains(Modifier::REVERSED) {
        queue!(w, SetAttribute(CrosstermAttribute::NoReverse))?;
    }
    let reset_intensity = removed.contains(Modifier::BOLD) || removed.contains(Modifier::DIM);
    if reset_intensity {
        // Bold and Dim are both reset by applying the Normal intensity, so
        // whichever of the two survives has to be reapplied.
        queue!(w, SetAttribute(CrosstermAttribute::NormalIntensity))?;
        if to.contains(Modifier::DIM) {
            queue!(w, SetAttribute(CrosstermAttribute::Dim))?;
        }
        if to.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(CrosstermAttribute::Bold))?;
        }
    }
    if removed.contains(Modifier::ITALIC) {
        queue!(w, SetAttribute(CrosstermAttribute::NoItalic))?;
    }
    if removed.contains(Modifier::UNDERLINED) {
        queue!(w, SetAttribute(CrosstermAttribute::NoUnderline))?;
    }
    if removed.contains(Modifier::CROSSED_OUT) {
        queue!(w, SetAttribute(CrosstermAttribute::NotCrossedOut))?;
    }
    if removed.contains(Modifier::HIDDEN) {
        queue!(w, SetAttribute(CrosstermAttribute::NoHidden))?;
    }
    if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CrosstermAttribute::NoBlink))?;
    }

    let added = to - from;
    if added.contains(Modifier::REVERSED) {
        queue!(w, SetAttribute(CrosstermAttribute::Reverse))?;
    }
    if added.contains(Modifier::BOLD) && !reset_intensity {
        queue!(w, SetAttribute(CrosstermAttribute::Bold))?;
    }
    if added.contains(Modifier::ITALIC) {
        queue!(w, SetAttribute(CrosstermAttribute::Italic))?;
    }
    if added.contains(Modifier::UNDERLINED) {
        queue!(w, SetAttribute(CrosstermAttribute::Underlined))?;
    }
    if added.contains(Modifier::DIM) && !reset_intensity {
        queue!(w, SetAttribute(CrosstermAttribute::Dim))?;
    }
    if added.contains(Modifier::CROSSED_OUT) {
        queue!(w, SetAttribute(CrosstermAttribute::CrossedOut))?;
    }
    if added.contains(Modifier::HIDDEN) {
        queue!(w, SetAttribute(CrosstermAttribute::Hidden))?;
    }
    if added.contains(Modifier::SLOW_BLINK) {
        queue!(w, SetAttribute(CrosstermAttribute::SlowBlink))?;
    }
    if added.contains(Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CrosstermAttribute::RapidBlink))?;
    }
    Ok(())
}
