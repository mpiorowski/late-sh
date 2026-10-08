//! Keys on the picker, and its two doors: `open` from the focused Zen ascii
//! tile, `pick` hands the piece under the cursor to that tile.

use super::state::{PiecePickerState, Target};
use crate::app::input::{MouseButton, MouseEventKind, ParsedInput};
use crate::app::state::App;

/// Open on the focused Zen ascii tile's piece; nothing when the focused
/// tile is not an ascii tile.
pub(crate) fn open(app: &mut App) {
    let Some(piece) = app.zen.focused_piece() else {
        return;
    };
    app.piece_picker.open(piece);
}

/// The piece under the cursor lands on the tile that opened the picker,
/// saved with the layout, and the picker closes.
fn pick(app: &mut App) {
    let piece = app.piece_picker.selected();
    app.piece_picker.close();
    if app.zen.set_focused_piece(piece) {
        app.mark_zen_layout_dirty();
    }
}

pub(crate) fn close(app: &mut App) {
    app.piece_picker.close();
}

pub(crate) fn handle_input(app: &mut App, event: ParsedInput) {
    if let ParsedInput::Mouse(mouse) = event {
        if !app.interaction_mode.mouse_enabled() {
            return;
        }
        let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
            return;
        };
        match mouse.kind {
            MouseEventKind::ScrollUp => app.piece_picker.mouse.scroll(x, y, -3, app.size),
            MouseEventKind::ScrollDown => app.piece_picker.mouse.scroll(x, y, 3, app.size),
            MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
                match app.piece_picker.mouse.target(x, y, app.size) {
                    Some(Target::Close) => close(app),
                    Some(Target::Row(index)) => {
                        app.piece_picker.set_cursor(index);
                        pick(app);
                    }
                    None => {}
                }
                app.piece_picker.mouse.invalidate();
            }
            _ => {}
        }
        return;
    }
    let picker: &mut PiecePickerState = &mut app.piece_picker;
    match event {
        ParsedInput::Byte(0x1B) | ParsedInput::Byte(b'q') | ParsedInput::Char('q') => close(app),
        ParsedInput::Byte(b'\r') | ParsedInput::Byte(b' ') | ParsedInput::Char(' ') => pick(app),
        ParsedInput::Arrow(b'B') | ParsedInput::Byte(b'j') | ParsedInput::Char('j') => {
            picker.move_cursor(1)
        }
        ParsedInput::Arrow(b'A') | ParsedInput::Byte(b'k') | ParsedInput::Char('k') => {
            picker.move_cursor(-1)
        }
        _ => {}
    }
}
