//! Bridge between our `vte`-driven byte stream and `ratatui-textarea`'s
//! built-in emacs/readline keymap.
//!
//! We don't pull in `ratatui-textarea`'s `crossterm` feature, and our input
//! pipeline produces raw C0 bytes instead of `crossterm::KeyEvent`s. The
//! helper here rebuilds the `Input` value that `TextArea::input()` would have
//! received so composers can forward unclaimed control bytes and inherit the
//! stock `^A/^E/^K/^F/^B/...` behavior without a hand-rolled match arm per
//! chord.
//!
//! Callers decide which bytes to intercept (ESC, CR, ^U, etc.) *before*
//! calling this; anything returned here is meant to be handed straight to
//! `TextArea::input(...)`.

use ratatui_textarea::{Input, Key};

/// Rebuild the `Input` for a `Ctrl+<letter>` chord from its raw C0 byte.
///
/// Only accepts `0x01..=0x1A` — the subset of control bytes that are
/// unambiguously `Ctrl+a`..`Ctrl+z`. Returns `None` for every other byte
/// (including 0x00 NUL, 0x1B ESC, 0x1C..=0x1F, 0x7F DEL) so callers can
/// keep their own routing for those.
pub fn ctrl_byte_to_input(byte: u8) -> Option<Input> {
    if !(0x01..=0x1A).contains(&byte) {
        return None;
    }
    let ch = (byte + b'a' - 1) as char;
    Some(Input {
        key: Key::Char(ch),
        ctrl: true,
        alt: false,
        shift: false,
    })
}


