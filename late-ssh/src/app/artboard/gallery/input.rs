//! Keys and mouse for the gallery side of the Artboard page: the rail, the
//! piece list, the full-frame piece, and the hang flow. `page.rs` calls in
//! here whenever the gallery claims input (`State::gallery_claims_input`);
//! the board's own view-mode keys stay in `page.rs`.

use dartboard_editor::{AppKey, AppKeyCode, AppModifiers};

use crate::app::artboard::input::app_pointer_event_from_mouse;
use crate::app::artboard::state::State;
use crate::app::artboard::svc::ArtboardSnapshotKind;
use crate::app::input::{MouseButton, MouseEventKind, ParsedInput};

use super::state::{Focus, HangFlow, RailActivation, RailRow};

/// What the page must do after a gallery key: nothing, or one of the rail
/// actions that need the page (the live board, the ban gate, the archive
/// lists).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GalleryAction {
    Ignored,
    Handled,
    /// The Board row: the live board, whatever archive was up.
    FocusBoard,
    BeginHang,
    OpenArchive(ArtboardSnapshotKind),
}

fn from_activation(activation: RailActivation) -> GalleryAction {
    match activation {
        RailActivation::FocusCanvas => GalleryAction::FocusBoard,
        RailActivation::OpenGallery => GalleryAction::Handled,
        RailActivation::BeginHang => GalleryAction::BeginHang,
        RailActivation::OpenArchive(kind) => GalleryAction::OpenArchive(kind),
    }
}

pub fn handle_key(state: &mut State, screen_size: (u16, u16), byte: u8) -> GalleryAction {
    match state.gallery().hang() {
        HangFlow::Framing => return handle_framing_key(state, screen_size, byte),
        HangFlow::Confirm { .. } => return handle_confirm_key(state, byte),
        HangFlow::Submitting => return GalleryAction::Handled,
        HangFlow::Idle => {}
    }
    if byte == b'\t' {
        return handle_tab(state);
    }
    match state.gallery().focus() {
        Focus::Canvas => GalleryAction::Ignored,
        Focus::Rail => handle_rail_key(state, byte),
        Focus::List => handle_list_key(state, byte),
        Focus::Piece => handle_piece_key(state, byte),
        Focus::Archive => handle_archive_key(state, byte),
    }
}

/// Tab backs out of a pane to the rail. On the rail itself Tab never
/// arrives: it is the page switch there (`app/input.rs`).
fn handle_tab(state: &mut State) -> GalleryAction {
    match state.gallery().focus() {
        Focus::Canvas | Focus::Rail => GalleryAction::Ignored,
        Focus::List | Focus::Archive => {
            state.gallery_mut().focus_rail();
            GalleryAction::Handled
        }
        Focus::Piece => {
            state.gallery_mut().close_piece();
            state.gallery_mut().focus_rail();
            GalleryAction::Handled
        }
    }
}

pub fn handle_arrow(state: &mut State, screen_size: (u16, u16), key: u8) -> GalleryAction {
    match state.gallery().hang() {
        HangFlow::Framing => return framing_arrow(state, screen_size, key, false),
        HangFlow::Confirm { .. } | HangFlow::Submitting => return GalleryAction::Handled,
        HangFlow::Idle => {}
    }
    match state.gallery().focus() {
        Focus::Canvas => GalleryAction::Ignored,
        Focus::Rail => match key {
            b'A' => {
                state.gallery_mut().rail_move(-1);
                GalleryAction::Handled
            }
            b'B' => {
                state.gallery_mut().rail_move(1);
                GalleryAction::Handled
            }
            b'C' => from_activation(state.gallery_mut().rail_activate()),
            b'D' => {
                state.gallery_mut().focus_canvas();
                GalleryAction::Handled
            }
            _ => GalleryAction::Ignored,
        },
        Focus::List => match key {
            b'A' => {
                state.gallery_mut().list_move(-1);
                GalleryAction::Handled
            }
            b'B' => {
                state.gallery_mut().list_move(1);
                GalleryAction::Handled
            }
            b'C' => {
                state.gallery_mut().open_selected_piece();
                GalleryAction::Handled
            }
            b'D' => {
                state.gallery_mut().focus_rail();
                GalleryAction::Handled
            }
            _ => GalleryAction::Ignored,
        },
        Focus::Piece => match key {
            b'A' | b'D' => {
                state.gallery_mut().list_move(-1);
                GalleryAction::Handled
            }
            b'B' | b'C' => {
                state.gallery_mut().list_move(1);
                GalleryAction::Handled
            }
            _ => GalleryAction::Ignored,
        },
        Focus::Archive => match key {
            b'A' => {
                state.archive_move(-1);
                GalleryAction::Handled
            }
            b'B' => {
                state.archive_move(1);
                GalleryAction::Handled
            }
            b'C' => {
                state.gallery_mut().focus_canvas();
                GalleryAction::Handled
            }
            b'D' => {
                state.close_archive_list();
                GalleryAction::Handled
            }
            _ => GalleryAction::Ignored,
        },
    }
}

pub fn handle_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> GalleryAction {
    // Keys arrive as bytes through `handle_key` after this; only the rich
    // events the flow needs are taken here, everything else is `Ignored`
    // so the byte path still sees Enter, Esc, and the title's letters.
    match state.gallery().hang() {
        HangFlow::Framing => return handle_framing_event(state, screen_size, event),
        HangFlow::Confirm { .. } => {
            return match event {
                ParsedInput::Paste(bytes) => {
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        for ch in text.chars() {
                            state.gallery_mut().title_push(ch);
                        }
                    }
                    GalleryAction::Handled
                }
                ParsedInput::Mouse(_) => GalleryAction::Handled,
                _ => GalleryAction::Ignored,
            };
        }
        HangFlow::Submitting => {
            return match event {
                ParsedInput::Mouse(_) => GalleryAction::Handled,
                _ => GalleryAction::Ignored,
            };
        }
        HangFlow::Idle => {}
    }

    if state.gallery().focus() == Focus::Archive {
        return handle_archive_event(state, event);
    }

    match event {
        ParsedInput::Mouse(mouse) => {
            let left_down = matches!(mouse.kind, MouseEventKind::Down)
                && matches!(mouse.button, Some(MouseButton::Left));
            if left_down && let Some(row) = state.gallery().rail_row_at(mouse.x, mouse.y) {
                let gallery = state.gallery_mut();
                gallery.rail_select(row);
                gallery.focus_rail();
                return match row {
                    RailRow::Board => {
                        gallery.focus_canvas();
                        GalleryAction::Handled
                    }
                    RailRow::Gallery(_) => {
                        gallery.focus_list();
                        GalleryAction::Handled
                    }
                    RailRow::Hang => GalleryAction::BeginHang,
                    RailRow::Archive(kind) => GalleryAction::OpenArchive(kind),
                };
            }
            if !state.gallery().shows_gallery_pane() {
                return GalleryAction::Ignored;
            }
            if left_down && let Some(index) = state.gallery().list_index_at(mouse.x, mouse.y) {
                let gallery = state.gallery_mut();
                gallery.list_select(index);
                gallery.focus_list();
                return GalleryAction::Handled;
            }
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    state.gallery_mut().list_move(-1);
                    GalleryAction::Handled
                }
                MouseEventKind::ScrollDown => {
                    state.gallery_mut().list_move(1);
                    GalleryAction::Handled
                }
                _ => GalleryAction::Handled,
            }
        }
        ParsedInput::PageUp if state.gallery().focus() == Focus::List => {
            state.gallery_mut().list_page(-1);
            GalleryAction::Handled
        }
        ParsedInput::PageDown if state.gallery().focus() == Focus::List => {
            state.gallery_mut().list_page(1);
            GalleryAction::Handled
        }
        ParsedInput::Home if state.gallery().focus() == Focus::List => {
            state.gallery_mut().list_move(isize::MIN / 2);
            GalleryAction::Handled
        }
        ParsedInput::End if state.gallery().focus() == Focus::List => {
            state.gallery_mut().list_move(isize::MAX / 2);
            GalleryAction::Handled
        }
        _ => GalleryAction::Ignored,
    }
}

fn handle_rail_key(state: &mut State, byte: u8) -> GalleryAction {
    match byte {
        b'j' | b'J' => {
            state.gallery_mut().rail_move(1);
            GalleryAction::Handled
        }
        b'k' | b'K' => {
            state.gallery_mut().rail_move(-1);
            GalleryAction::Handled
        }
        b'\r' | b'\n' | b'l' | b'L' => from_activation(state.gallery_mut().rail_activate()),
        // Esc on the rail is nobody's: the board is one Enter away, never
        // one Esc.
        _ => GalleryAction::Ignored,
    }
}

/// The archive list in the rail's place: the cursor is the time machine,
/// the board follows it.
fn handle_archive_key(state: &mut State, byte: u8) -> GalleryAction {
    match byte {
        b'j' | b'J' => {
            state.archive_move(1);
            GalleryAction::Handled
        }
        b'k' | b'K' => {
            state.archive_move(-1);
            GalleryAction::Handled
        }
        b'\r' | b'\n' | b'l' | b'L' => {
            state.gallery_mut().focus_canvas();
            GalleryAction::Handled
        }
        0x1B | b'h' | b'H' | b'q' | b'Q' => {
            state.close_archive_list();
            GalleryAction::Handled
        }
        _ => GalleryAction::Ignored,
    }
}

fn handle_archive_event(state: &mut State, event: &ParsedInput) -> GalleryAction {
    match event {
        ParsedInput::Mouse(mouse) => {
            let left_down = matches!(mouse.kind, MouseEventKind::Down)
                && matches!(mouse.button, Some(MouseButton::Left));
            if left_down && let Some(index) = state.archive_index_at(mouse.x, mouse.y) {
                state.archive_select(index);
                return GalleryAction::Handled;
            }
            if state.gallery().rail_line_at(mouse.x, mouse.y).is_none() {
                return GalleryAction::Ignored;
            }
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    state.archive_move(-1);
                    GalleryAction::Handled
                }
                MouseEventKind::ScrollDown => {
                    state.archive_move(1);
                    GalleryAction::Handled
                }
                _ => GalleryAction::Handled,
            }
        }
        ParsedInput::PageUp => {
            state.archive_page(-1);
            GalleryAction::Handled
        }
        ParsedInput::PageDown => {
            state.archive_page(1);
            GalleryAction::Handled
        }
        ParsedInput::Home => {
            state.archive_move(isize::MIN / 2);
            GalleryAction::Handled
        }
        ParsedInput::End => {
            state.archive_move(isize::MAX / 2);
            GalleryAction::Handled
        }
        _ => GalleryAction::Ignored,
    }
}

fn handle_list_key(state: &mut State, byte: u8) -> GalleryAction {
    match byte {
        b'j' | b'J' => {
            state.gallery_mut().list_move(1);
            GalleryAction::Handled
        }
        b'k' | b'K' => {
            state.gallery_mut().list_move(-1);
            GalleryAction::Handled
        }
        b'\r' | b'\n' | b'l' | b'L' => {
            state.gallery_mut().open_selected_piece();
            GalleryAction::Handled
        }
        b'v' | b'V' => {
            state.gallery_mut().applaud_selected();
            GalleryAction::Handled
        }
        0x1B | b'h' | b'H' => {
            state.gallery_mut().focus_rail();
            GalleryAction::Handled
        }
        _ => GalleryAction::Ignored,
    }
}

fn handle_piece_key(state: &mut State, byte: u8) -> GalleryAction {
    match byte {
        b'j' | b'J' => {
            state.gallery_mut().list_move(1);
            GalleryAction::Handled
        }
        b'k' | b'K' => {
            state.gallery_mut().list_move(-1);
            GalleryAction::Handled
        }
        b'v' | b'V' => {
            state.gallery_mut().applaud_selected();
            GalleryAction::Handled
        }
        0x1B | b'q' | b'Q' | b'\r' | b'\n' | b'h' | b'H' => {
            state.gallery_mut().close_piece();
            GalleryAction::Handled
        }
        _ => GalleryAction::Ignored,
    }
}

// ----- framing -----

fn handle_framing_key(state: &mut State, screen_size: (u16, u16), byte: u8) -> GalleryAction {
    match byte {
        0x1B => {
            state.cancel_framing();
            GalleryAction::Handled
        }
        b'\r' | b'\n' => {
            state.frame_selection_for_hang();
            GalleryAction::Handled
        }
        // Ctrl+P: the page's help, framing or not.
        0x10 => GalleryAction::Ignored,
        _ => {
            let _ = screen_size;
            GalleryAction::Handled
        }
    }
}

fn handle_framing_event(
    state: &mut State,
    screen_size: (u16, u16),
    event: &ParsedInput,
) -> GalleryAction {
    match event {
        ParsedInput::ShiftArrow(key) => framing_arrow(state, screen_size, *key, true),
        ParsedInput::Mouse(mouse) => {
            let left = matches!(mouse.button, Some(MouseButton::Left));
            let framing_gesture = left
                && matches!(
                    mouse.kind,
                    MouseEventKind::Down | MouseEventKind::Drag | MouseEventKind::Up
                );
            if framing_gesture {
                state.set_viewport_for_screen(screen_size);
                if let Some(pointer) = app_pointer_event_from_mouse(mouse) {
                    state.handle_pointer_event(pointer);
                }
            }
            GalleryAction::Handled
        }
        _ => GalleryAction::Ignored,
    }
}

fn framing_arrow(
    state: &mut State,
    screen_size: (u16, u16),
    key: u8,
    shift: bool,
) -> GalleryAction {
    let code = match key {
        b'A' => AppKeyCode::Up,
        b'B' => AppKeyCode::Down,
        b'C' => AppKeyCode::Right,
        b'D' => AppKeyCode::Left,
        _ => return GalleryAction::Ignored,
    };
    state.set_viewport_for_screen(screen_size);
    state.handle_app_key(AppKey {
        code,
        modifiers: AppModifiers {
            shift,
            ..Default::default()
        },
    });
    GalleryAction::Handled
}

fn handle_confirm_key(state: &mut State, byte: u8) -> GalleryAction {
    match byte {
        0x1B => {
            state.cancel_framing();
            GalleryAction::Handled
        }
        b'\r' | b'\n' => {
            state.gallery_mut().submit_hang();
            GalleryAction::Handled
        }
        0x7F | 0x08 => {
            state.gallery_mut().title_pop();
            GalleryAction::Handled
        }
        byte if byte.is_ascii_graphic() || byte == b' ' => {
            state.gallery_mut().title_push(byte as char);
            GalleryAction::Handled
        }
        0x10 => GalleryAction::Ignored,
        _ => GalleryAction::Handled,
    }
}
