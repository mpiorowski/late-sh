//! The piece picker's state machine: the list of every piece, a cursor on
//! one, and what picked it. Opened over the Zen page from an ascii tile
//! (the Settings Tweaks row uses the settings modal's own picker).

use late_core::models::user::AsciiPiece;

use crate::app::common::mouse::MouseState;

/// What a click in the picker can land on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Close,
    Row(usize),
}

#[derive(Default)]
pub(crate) struct PiecePickerState {
    pub(crate) mouse: MouseState<Target, ()>,
    open: bool,
    cursor: usize,
}

impl PiecePickerState {
    /// Open with the cursor on `current`, the piece the tile plays now.
    pub(crate) fn open(&mut self, current: AsciiPiece) {
        *self = Self::default();
        self.open = true;
        self.cursor = AsciiPiece::ALL
            .iter()
            .position(|piece| *piece == current)
            .expect("every piece is in ALL");
        self.mouse.reveal_selection();
    }

    pub(crate) fn close(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    /// The piece under the cursor.
    pub(crate) fn selected(&self) -> AsciiPiece {
        AsciiPiece::ALL[self.cursor]
    }

    pub(crate) fn move_cursor(&mut self, delta: isize) {
        let last = AsciiPiece::ALL.len() as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
        self.mouse.reveal_selection();
    }

    pub(crate) fn set_cursor(&mut self, index: usize) {
        self.cursor = index.min(AsciiPiece::ALL.len() - 1);
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
