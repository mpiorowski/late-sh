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
//! client until Ctrl+R.
//!
//! This backend never trusts contiguity across a cell that is not a single
//! ASCII byte: the cell after it gets an absolute cursor move, and a cell the
//! model considers wide is blanked first so a narrower rendering leaves a
//! styled space rather than whatever was there. The terminal's error stays
//! inside the glyph's own cells.
//!
//! The rule is deliberately broad. Box drawing and block elements are East
//! Asian Ambiguous width, drawn two columns wide by CJK-locale terminals, so
//! a border row can shift exactly the way an emoji row does; limiting the
//! re-anchor to emoji would fix one client and not the other. What the
//! breadth costs is kept small instead: after a single codepoint the cursor
//! can only be off by a column and is still on its row, so the next cell on
//! that row takes a column-only move (`CSI n G`, about six bytes) rather than
//! a full row-and-column move. Only a multi-codepoint grapheme, which a
//! per-codepoint terminal can draw several columns wide and past the row
//! end, loses the row too and forces the full move. `terminal_backend_test`
//! pins the bytes a bordered full frame costs.

use std::io::{self, Write};

use crossterm::{
    cursor::{MoveTo, MoveToColumn},
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

/// How far a cell can leave the client's cursor from where the model says it
/// is, once the terminal has drawn it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
    /// One printable ASCII byte: every terminal agrees it is one column.
    Ascii,
    /// One non-ASCII codepoint (box drawing, accented letters, CJK): a
    /// terminal may size it a column off, ambiguous width drawn wide or a
    /// wide glyph drawn narrow, never more. A cell followed by another on the
    /// same row is not in the last column, so a one-column overrun cannot
    /// wrap, and the cursor is still on the row.
    Codepoint,
    /// A multi-codepoint grapheme (VS16 emoji, ZWJ sequence, flag pair,
    /// combining marks): a per-codepoint terminal can draw it several
    /// columns wide, past the row end and onto the next row, so nothing
    /// about the cursor is known afterwards.
    Grapheme,
}

fn classify(cell: &Cell) -> Glyph {
    let symbol = cell.symbol();
    if symbol.len() == 1 {
        return Glyph::Ascii;
    }
    match symbol.chars().count() {
        1 => Glyph::Codepoint,
        _ => Glyph::Grapheme,
    }
}

/// What is known about where the client's cursor sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cursor {
    /// Exactly here: nothing but ASCII has been written since the last move.
    At(Position),
    /// Somewhere on this row: a single codepoint left the column wherever
    /// the terminal's width for it landed.
    OnRow(u16),
    /// Nowhere known: a multi-codepoint grapheme may have run onto another
    /// row, or nothing has been written yet.
    Unknown,
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
        let mut cursor = Cursor::Unknown;
        for (x, y, cell) in content {
            match cursor {
                Cursor::At(at) if at == Position { x, y } => {}
                Cursor::OnRow(row) if row == y => queue!(self.inner, MoveToColumn(x))?,
                Cursor::At(_) | Cursor::OnRow(_) | Cursor::Unknown => {
                    queue!(self.inner, MoveTo(x, y))?;
                }
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

            let after = match classify(cell) {
                Glyph::Ascii => {
                    queue!(self.inner, Print(cell.symbol()))?;
                    cursor = Cursor::At(Position {
                        x: x.saturating_add(1),
                        y,
                    });
                    continue;
                }
                Glyph::Codepoint => Cursor::OnRow(y),
                Glyph::Grapheme => Cursor::Unknown,
            };
            let width = usize::from(cell.cell_width());
            if width > 1 {
                // Blank every cell the model reserves for the glyph, in the
                // glyph's own style, then draw it from the left edge again.
                // The spaces are ASCII, so the row is certain, but this is
                // one move per wide glyph and the full move costs nothing
                // worth a second rule.
                queue!(self.inner, Print(" ".repeat(width)), MoveTo(x, y))?;
            }
            queue!(self.inner, Print(cell.symbol()))?;
            cursor = after;
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
