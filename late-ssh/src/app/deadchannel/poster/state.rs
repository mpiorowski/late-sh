//! PRINTS: the poster, a 32 by 12 picture a runner paints and hangs on
//! their profile (GAME.md, "The look"). The one free-painted surface in the
//! game, so the brush is gated the way the look is: the tailor's seven
//! tints until the runner has an Old Signal mark, every colour after.
//!
//! A `Draft` is a session-local `dartboard_core::Canvas` edited through
//! `dartboard_editor` as a library: no server, no ops, no swatch strip, no
//! floating brush. Keys arrive as a closed `Edit` (the shop's own set) and
//! become the editor's actions here. Pure: nothing here knows whether the
//! draft was saved; `dirty` compares against what the panel opened on.

use std::cell::Cell;

use dartboard_core::{Canvas, CellValue, Pos, RgbColor};
use dartboard_editor::{
    AppPointerEvent, EditorAction, EditorSession, MoveDir, Viewport, handle_editor_action,
    handle_editor_pointer, paste_text_block,
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use ratatui::layout::Rect;

use crate::app::common::color_picker::ColorPicker;
use crate::app::deadchannel::city::ui::tint_rgb;
use crate::app::deadchannel::runner::state::{TINTS, Tint};

pub const POSTER_WIDTH: usize = 32;
pub const POSTER_HEIGHT: usize = 12;

/// Cells of static the test card carries, wherever its seed puts them.
const TEST_CARD_STATIC: usize = 14;

/// What the brush can be dipped in, and what it is dipped in now. The
/// palette is the runner's marks at open: none is the tailor's seven, one
/// Old Signal mark opens every colour (and the picker that mixes it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Brush {
    /// `marks == 0`: one of the seven tints, painted as its RGB.
    Tints(Tint),
    /// `marks >= 1`: any colour; the picker while it is open.
    Full {
        color: RgbColor,
        picker: Option<ColorPicker>,
    },
}

impl Brush {
    /// The brush a runner with `marks` Old Signal marks starts with.
    pub fn for_marks(marks: i32) -> Self {
        match marks {
            0 => Brush::Tints(TINTS[TINTS.len() - 1]),
            _ => Brush::Full {
                color: tint_paint(TINTS[TINTS.len() - 1]),
                picker: None,
            },
        }
    }

    /// The colour the next stroke is painted in.
    pub fn color(&self) -> RgbColor {
        match self {
            Brush::Tints(tint) => tint_paint(*tint),
            Brush::Full { color, .. } => *color,
        }
    }
}

/// A tint as the paint it is on the canvas: the city's own RGB for it.
pub fn tint_paint(tint: Tint) -> RgbColor {
    let [r, g, b] = tint_rgb(tint).map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8);
    RgbColor::new(r, g, b)
}

/// The shop's keys, as the editor's actions see them. Everything the
/// panel offers on the canvas and nothing else: no pan, no push or pull,
/// no border, no swatches past the primary one copy and paste use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Move(MoveDir),
    /// Grow the selection from where the cursor stood.
    Extend(MoveDir),
    /// The row below, the column kept (Enter).
    NextLine,
    /// A typed character: painted at the cursor, or over the selection.
    Type(char),
    /// Backspace: the cell behind the cursor, or the selection.
    EraseBack,
    /// Delete: the cell under the cursor, or the selection.
    EraseHere,
    Copy,
    Cut,
    Paste,
    Deselect,
}

#[derive(Debug, Clone)]
pub struct Draft {
    pub canvas: Canvas,
    pub editor: EditorSession,
    pub brush: Brush,
    /// The next burn key burns: the first press arms, any other key
    /// disarms (the ledge's two presses).
    pub burn_armed: bool,
    /// What the panel opened on, or what was saved or burned since:
    /// `dirty` is the canvas differing from it.
    hung: Canvas,
    /// The panel body the renderer last drew this draft into, in frame
    /// cells: where a click lands on the canvas and on the picker.
    drawn_at: Cell<Option<Rect>>,
}

impl Draft {
    /// A draft over the poster on the wall (`None` is a blank one), the
    /// brush cut to `marks`. A stored canvas of another size is clipped to
    /// 32 by 12, so the panel, the profile, and the web always hold one.
    pub fn new(stored: Option<Canvas>, marks: i32) -> Self {
        let canvas = match stored {
            Some(stored) => fit(&stored),
            None => blank(),
        };
        Self {
            hung: canvas.clone(),
            canvas,
            editor: EditorSession::default(),
            brush: Brush::for_marks(marks),
            burn_armed: false,
            drawn_at: Cell::new(None),
        }
    }

    /// The canvas differs from what hangs on the wall.
    pub fn dirty(&self) -> bool {
        self.canvas != self.hung
    }

    /// `saved` hangs on the wall now. Paint laid on after it was sent stays
    /// dirty.
    pub fn saved(&mut self, saved: Canvas) {
        self.hung = saved;
    }

    /// The wall is bare: the canvas with it.
    pub fn burned(&mut self) {
        self.canvas = blank();
        self.hung = blank();
        self.editor.clear_selection();
    }

    pub fn arm_burn(&mut self) {
        self.burn_armed = true;
    }

    pub fn disarm_burn(&mut self) {
        self.burn_armed = false;
    }

    /// Whether a selection is held: typed keys fill it, the erase keys
    /// clear it.
    pub fn selecting(&self) -> bool {
        self.editor.mode.is_selecting() && self.editor.selection_anchor.is_some()
    }

    pub fn edit(&mut self, edit: Edit) {
        let action = match (edit, self.selecting()) {
            (Edit::Move(dir), _) => EditorAction::Move {
                dir,
                extend_selection: false,
            },
            (Edit::Extend(dir), _) => EditorAction::Move {
                dir,
                extend_selection: true,
            },
            (Edit::NextLine, _) => EditorAction::MoveDownLine,
            (Edit::Type(ch), true) => EditorAction::FillSelectionOrCell(ch),
            (Edit::Type(ch), false) => EditorAction::InsertChar(ch),
            (Edit::EraseBack | Edit::EraseHere, true) => EditorAction::FillSelectionOrCell(' '),
            (Edit::EraseBack, false) => EditorAction::Backspace,
            (Edit::EraseHere, false) => EditorAction::Delete,
            (Edit::Copy, _) => EditorAction::CopySelection,
            (Edit::Cut, _) => EditorAction::CutSelection,
            (Edit::Paste, _) => EditorAction::PastePrimarySwatch,
            (Edit::Deselect, _) => EditorAction::ClearSelection,
        };
        let color = self.brush.color();
        handle_editor_action(&mut self.editor, &mut self.canvas, action, color);
    }

    /// Text from outside the keys (a glyph from the picker, a terminal
    /// paste) laid at the cursor in the brush's colour, each line starting
    /// under the cursor. On one line the cursor moves past it, as if typed.
    pub fn lay_text(&mut self, text: &str) {
        self.editor.clear_selection();
        let color = self.brush.color();
        paste_text_block(&self.editor, &mut self.canvas, text, color);
        if text.contains('\n') {
            return;
        }
        let width: usize = text.chars().map(Canvas::display_width).sum();
        for _ in 0..width {
            self.editor.move_right(&self.canvas);
        }
    }

    /// The colour key: the next tint on the tailor's seven, the picker on
    /// every colour (it stays open until applied or closed).
    pub fn colour_key(&mut self) {
        match &mut self.brush {
            Brush::Tints(tint) => {
                let at = TINTS
                    .iter()
                    .position(|candidate| candidate == tint)
                    .expect("a tint is one of the seven");
                *tint = TINTS[(at + 1) % TINTS.len()];
            }
            Brush::Full { color, picker } => {
                if picker.is_none() {
                    *picker = Some(ColorPicker::open(*color));
                }
            }
        }
    }

    pub fn picker(&self) -> Option<&ColorPicker> {
        match &self.brush {
            Brush::Tints(_) => None,
            Brush::Full { picker, .. } => picker.as_ref(),
        }
    }

    pub fn picker_mut(&mut self) -> Option<&mut ColorPicker> {
        match &mut self.brush {
            Brush::Tints(_) => None,
            Brush::Full { picker, .. } => picker.as_mut(),
        }
    }

    /// Enter in the picker: its working colour is the brush now.
    pub fn apply_picker(&mut self) {
        if let Brush::Full { color, picker } = &mut self.brush
            && let Some(open) = picker.take()
        {
            *color = open.color;
        }
    }

    /// The picker shut, the brush as it was.
    pub fn close_picker(&mut self) {
        if let Brush::Full { picker, .. } = &mut self.brush {
            *picker = None;
        }
    }

    /// A pointer on the canvas (frame cells, 0-based): a press moves the
    /// cursor, a drag selects. Nothing happens off the canvas or before the
    /// panel was drawn.
    pub fn point(&mut self, pointer: AppPointerEvent) {
        let Some(at) = self.canvas_at() else {
            return;
        };
        // The panel draws the poster from its top left, cut to the body, and
        // never scrolls. The editor scrolls its own origin when the keys walk
        // the cursor past a cut-off body, so a click reads from the top left
        // again or it lands on a cell the pointer is not over.
        self.editor.viewport_origin = Pos { x: 0, y: 0 };
        self.editor.set_viewport(
            Viewport {
                x: at.x,
                y: at.y,
                width: at.width,
                height: at.height,
            },
            &self.canvas,
        );
        let color = self.brush.color();
        handle_editor_pointer(&mut self.editor, &mut self.canvas, pointer, color);
    }

    /// The renderer's record of the panel body it drew this draft into.
    pub fn record_drawn_at(&self, body: Rect) {
        self.drawn_at.set(Some(body));
    }

    pub fn drawn_at(&self) -> Option<Rect> {
        self.drawn_at.get()
    }

    /// The canvas's cells in the last drawn panel: the body's top left,
    /// cut to the rows the screen had.
    pub fn canvas_at(&self) -> Option<Rect> {
        self.drawn_at.get().map(canvas_rect)
    }
}

/// Where the canvas sits in a panel body: its top left, 32 by 12 or what
/// the body has of it.
pub fn canvas_rect(body: Rect) -> Rect {
    Rect {
        x: body.x,
        y: body.y,
        width: (POSTER_WIDTH as u16).min(body.width),
        height: (POSTER_HEIGHT as u16).min(body.height),
    }
}

/// An empty poster.
pub fn blank() -> Canvas {
    Canvas::with_size(POSTER_WIDTH, POSTER_HEIGHT)
}

/// `stored` on a 32 by 12 canvas: every glyph that fits, in its colour.
pub fn fit(stored: &Canvas) -> Canvas {
    let mut canvas = blank();
    for y in 0..POSTER_HEIGHT.min(stored.height) {
        for x in 0..POSTER_WIDTH.min(stored.width) {
            let pos = Pos { x, y };
            let Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) = stored.cell(pos) else {
                continue;
            };
            match stored.fg(pos) {
                Some(fg) => canvas.put_glyph_colored(pos, ch, fg),
                None => canvas.put_glyph(pos, ch),
            };
        }
    }
    canvas
}

/// Any glyph on the poster but a space: a poster erased to nothing still
/// stores, and bare paper is no picture.
pub fn has_ink(canvas: &Canvas) -> bool {
    canvas.iter().any(|(_, cell)| match cell {
        CellValue::Narrow(ch) | CellValue::Wide(ch) => !ch.is_whitespace(),
        CellValue::WideCont => false,
    })
}

/// The poster a runner without one shows: seven bars in the seven tints,
/// a broadcast's test card, with a little static the seed scatters (the
/// user, so a runner's card is the same on every open).
pub fn test_card(seed: u64) -> Canvas {
    let mut canvas = blank();
    // Seven bars across 32 columns: the first ones a column wider.
    let mut x = 0;
    for (bar, tint) in TINTS.iter().enumerate() {
        let width = POSTER_WIDTH / TINTS.len() + usize::from(bar < POSTER_WIDTH % TINTS.len());
        for column in x..x + width {
            for y in 0..POSTER_HEIGHT {
                canvas.put_glyph_colored(Pos { x: column, y }, '█', tint_paint(*tint));
            }
        }
        x += width;
    }
    let mut rng = StdRng::seed_from_u64(seed);
    for _ in 0..TEST_CARD_STATIC {
        let pos = Pos {
            x: rng.gen_range(0..POSTER_WIDTH),
            y: rng.gen_range(0..POSTER_HEIGHT),
        };
        let fg = canvas.fg(pos).expect("every cell of the card is a bar");
        let shade = match rng.gen_range(0..3) {
            0 => '░',
            1 => '▒',
            _ => '▓',
        };
        canvas.put_glyph_colored(pos, shade, fg);
    }
    canvas
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
