//! Nightcap input: number keys 1-6 sit in or stand from a seat, `i` (or
//! Enter) opens the room's composer for a seated patron, `d` opens the house
//! menu (where `1`-`4` order a pour and `r` buys the other stools a round),
//! and `c` takes the knife out to carve a line into your stool. While the
//! knife is out every key goes to that one-line field until Enter carves
//! or Esc drops it. No walking, no mouse handling. Esc is handled centrally
//! by `app/input.rs::dispatch_escape` (drops the knife, closes the menu,
//! else returns to the Clubhouse), same as every other contextual screen:
//! it never reaches this handler. Composing input never reaches it either:
//! the shared composer gate in `app/input.rs` intercepts first
//! (`screen_composes_chat`).
//!
//! Unlike the Clubhouse and the Undercity, this screen does claim keys the
//! global handler wants (`1`-`6` are the page digits, and `v` arms a music
//! chord whose suffixes they are). Anything consumed here therefore has to
//! disarm a half-typed chord on the way past, or the chord survives the
//! keypress that fed it and swallows whatever is pressed next.

use uuid::Uuid;

use late_core::models::nightcap_carving::CARVING_MAX_CHARS;

use crate::app::common::textarea_input::{EditOutcome, handle_single_line_edit};
use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::lobby::SEAT_COUNT;
use super::state::{Drink, Order};

/// The room a seated patron composes into: `None` off a stool, or before
/// the room snapshot carries the nightcap room. The seat is the whole
/// gate; the room itself is hidden from every other surface.
pub fn compose_room(app: &App) -> Option<Uuid> {
    if !app.nightcap.compose_allowed() {
        return None;
    }
    app.chat.nightcap_room_id()
}

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    // The knife is out: the field owns every key. Digits are text here,
    // not stools, and `?` is a character, not the help guide.
    if let Some(field) = app.nightcap.carving_mut() {
        app.music_prefix_armed = false;
        match handle_single_line_edit(field, event, CARVING_MAX_CHARS) {
            EditOutcome::Submit => {
                if let Some((stool, body)) = app.nightcap.take_carving() {
                    app.nightcap_carve(stool, body);
                }
            }
            EditOutcome::Cancel => {
                app.nightcap.cancel_carving();
            }
            EditOutcome::Handled | EditOutcome::Ignored => {}
        }
        return true;
    }

    let Some(byte) = event_byte(event) else {
        return false;
    };
    match byte {
        b'i' | b'I' | b'\r' | b'\n' => {
            app.music_prefix_armed = false;
            match compose_room(app) {
                Some(room_id) => app.chat.start_composing_in_room(room_id),
                None if app.nightcap.compose_allowed() => app.nightcap.note_room_not_loaded(),
                None => app.nightcap.note_compose_needs_seat(),
            }
            true
        }
        b'd' | b'D' => {
            app.music_prefix_armed = false;
            // Opening the menu is the one moment the banked-drinks count is
            // read: it is the only place that count is printed.
            if app.nightcap.toggle_menu() {
                app.nightcap_check_credits();
            }
            true
        }
        b'c' | b'C' => {
            app.music_prefix_armed = false;
            app.nightcap.start_carving();
            true
        }
        b'1'..=b'6' if app.nightcap.menu_open() => {
            app.music_prefix_armed = false;
            let pick = (byte - b'1') as usize;
            if let Some(drink) = Drink::MENU.get(pick) {
                app.nightcap_order(Order::Drink(*drink));
            }
            true
        }
        b'1'..=b'6' => {
            app.music_prefix_armed = false;
            let seat = (byte - b'1') as usize;
            if seat < SEAT_COUNT {
                app.nightcap.toggle_seat(seat);
            }
            true
        }
        b'r' | b'R' if app.nightcap.menu_open() => {
            app.music_prefix_armed = false;
            app.nightcap_order(Order::Round);
            true
        }
        _ => false,
    }
}

fn event_byte(event: &ParsedInput) -> Option<u8> {
    match event {
        ParsedInput::Byte(byte) => Some(*byte),
        ParsedInput::Char(ch) if ch.is_ascii() => Some(*ch as u8),
        _ => None,
    }
}
