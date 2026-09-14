//! Nightcap input: number keys 1-6 sit in or stand from a seat, `d` orders
//! a drink. No walking, no mouse handling. Esc is handled centrally by
//! `app/input.rs::dispatch_escape` (returns to the Clubhouse), same as
//! every other contextual screen — it never reaches this handler.

use crate::app::input::ParsedInput;
use crate::app::state::App;

use super::lobby::SEAT_COUNT;

pub fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    let Some(byte) = event_byte(event) else {
        return false;
    };
    match byte {
        b'1'..=b'6' => {
            let seat = (byte - b'1') as usize;
            if seat < SEAT_COUNT {
                app.nightcap.toggle_seat(seat);
            }
            true
        }
        b'd' | b'D' => {
            app.nightcap.order_drink();
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
