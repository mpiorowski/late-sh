//! Keys for a piece's #lounge card: Enter or a click on the card opens the
//! piece, and while it is open over the chat every key is the modal's, so
//! `v` applauds here and is never the page's `v` underneath. Enter on a
//! vote call's line opens the gallery's Vote listing the same way in.

use uuid::Uuid;

use crate::app::artboard::gallery::svc::GalleryPiece;
use crate::app::common::primitives::Screen;
use crate::app::input::ParsedInput;
use crate::app::state::App;

/// What a #lounge line opens on the Artboard page, held by `App` until the
/// page may show (`sync_dartboard_connection`): at once, or after the
/// content disclaimer is answered.
pub(crate) enum LoungeDoor {
    /// One piece full frame, from its card.
    Piece(GalleryPiece),
    /// The Vote listing full frame, from a vote call's line.
    Vote,
}

/// Enter or a click on the selected message in `room_id` when it is a
/// piece's card: the piece over the chat. With the Artboard's content
/// disclaimer still to answer, the Artboard page instead, which asks first
/// and then shows the piece: the disclaimer has one door. Enter on a vote
/// call's line: the Artboard page on its Vote listing. False for any other
/// message.
pub(crate) fn open_selected_card(app: &mut App, room_id: Uuid) -> bool {
    let Some(message_id) = app.chat.selected_message_id_in_room(room_id) else {
        return false;
    };
    if app.gallery_cards.is_vote_call(message_id) {
        app.open_gallery_door(LoungeDoor::Vote);
        return true;
    }
    let Some(piece) = app.gallery_cards.card(message_id) else {
        return false;
    };
    match app.artboard_content_gated() {
        true => {
            let piece = piece.clone();
            app.open_gallery_door(LoungeDoor::Piece(piece));
        }
        false => {
            app.gallery_cards.open(message_id);
        }
    }
    true
}

/// A key or the mouse while a piece is open over the chat. Whatever is not
/// the modal's is swallowed, so nothing reaches the page under it.
pub(crate) fn handle_input(app: &mut App, event: &ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.gallery_cards.close();
        }
        ParsedInput::Byte(b'v' | b'V') | ParsedInput::Char('v' | 'V') => {
            app.gallery_cards.applaud();
        }
        ParsedInput::Byte(b'o' | b'O') | ParsedInput::Char('o' | 'O') => {
            if let Some(piece) = app.gallery_cards.close() {
                app.open_gallery_door(LoungeDoor::Piece(piece));
            }
        }
        _ => {}
    }
}

impl App {
    /// Go to the Artboard page with `door` open full-frame in its gallery.
    /// It waits in `pending_gallery_door` until the page may show
    /// (`sync_dartboard_connection`): at once, or after the content
    /// disclaimer is answered.
    pub(crate) fn open_gallery_door(&mut self, door: LoungeDoor) {
        self.set_screen(Screen::Artboard);
        self.pending_gallery_door = Some(door);
        self.sync_dartboard_connection();
    }
}
