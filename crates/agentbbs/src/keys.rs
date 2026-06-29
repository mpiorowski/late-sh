//! Translate a stream of raw terminal/SSH input bytes into crossterm
//! [`KeyEvent`]s that the AgentBBS [`agentbbs_tui::App`] understands.
//!
//! SSH (and a raw PTY) delivers keystrokes as bytes, sometimes split across
//! reads. [`KeyDecoder`] buffers partial UTF-8 and partial escape sequences so
//! arrow keys and multibyte characters survive fragmentation.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A small, allocation-light decoder from raw bytes to crossterm key events.
///
/// Feed bytes with [`KeyDecoder::feed`]; it returns the events that became
/// complete, retaining any partial sequence for the next call.
#[derive(Default)]
pub struct KeyDecoder {
    /// Bytes carried over from a previous `feed` (partial UTF-8 or escape seq).
    pending: Vec<u8>,
}

impl KeyDecoder {
    /// A fresh decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode `bytes`, returning every [`KeyEvent`] that completed.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<KeyEvent> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut i = 0;
        let buf = std::mem::take(&mut self.pending);

        while i < buf.len() {
            let b = buf[i];
            match b {
                0x1b => {
                    // Escape, possibly the start of a CSI sequence.
                    match decode_escape(&buf[i..]) {
                        EscResult::Key(ev, consumed) => {
                            out.push(ev);
                            i += consumed;
                        }
                        EscResult::Incomplete => {
                            // Hold the rest until more bytes arrive.
                            self.pending = buf[i..].to_vec();
                            return out;
                        }
                    }
                }
                b'\r' | b'\n' => {
                    out.push(plain(KeyCode::Enter));
                    i += 1;
                }
                0x7f | 0x08 => {
                    out.push(plain(KeyCode::Backspace));
                    i += 1;
                }
                b'\t' => {
                    out.push(plain(KeyCode::Tab));
                    i += 1;
                }
                0x03 => {
                    out.push(ctrl('c'));
                    i += 1;
                }
                0x13 => {
                    out.push(ctrl('s'));
                    i += 1;
                }
                // Other C0 controls (Ctrl-A..Ctrl-Z) map to Ctrl+<letter>,
                // skipping the ones already handled above.
                0x01..=0x1a => {
                    let letter = (b + b'a' - 1) as char;
                    out.push(ctrl(letter));
                    i += 1;
                }
                _ => {
                    // A printable byte: decode one UTF-8 scalar, buffering if
                    // the sequence is truncated.
                    match decode_utf8(&buf[i..]) {
                        Some((ch, len)) => {
                            out.push(plain(KeyCode::Char(ch)));
                            i += len;
                        }
                        None => {
                            // Partial multibyte char; wait for more bytes.
                            self.pending = buf[i..].to_vec();
                            return out;
                        }
                    }
                }
            }
        }
        out
    }
}

fn plain(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

enum EscResult {
    Key(KeyEvent, usize),
    Incomplete,
}

/// Decode an escape sequence starting at `buf[0] == 0x1b`.
fn decode_escape(buf: &[u8]) -> EscResult {
    debug_assert_eq!(buf[0], 0x1b);
    // Bare ESC with nothing after it: could be a lone Esc or the start of a
    // CSI. We treat a buffer of length 1 as incomplete so a following '[' can
    // arrive; a length-1 buffer that never grows is flushed by the caller's
    // pending logic on the next feed (rare in practice over SSH).
    if buf.len() == 1 {
        return EscResult::Incomplete;
    }
    match buf[1] {
        b'[' | b'O' => {
            // CSI / SS3. Expect a final byte indicating the key.
            if buf.len() < 3 {
                return EscResult::Incomplete;
            }
            let final_byte = buf[2];
            let code = match final_byte {
                b'A' => Some(KeyCode::Up),
                b'B' => Some(KeyCode::Down),
                b'C' => Some(KeyCode::Right),
                b'D' => Some(KeyCode::Left),
                b'H' => Some(KeyCode::Home),
                b'F' => Some(KeyCode::End),
                _ => None,
            };
            match code {
                Some(c) => EscResult::Key(plain(c), 3),
                // Unknown CSI: drop ESC and '[' so we don't loop forever.
                None => EscResult::Key(plain(KeyCode::Esc), 2),
            }
        }
        // ESC followed by anything else: treat the ESC as a standalone Esc and
        // let the following byte be processed on its own.
        _ => EscResult::Key(plain(KeyCode::Esc), 1),
    }
}

/// Decode a single UTF-8 scalar from the front of `buf`.
///
/// Returns `(char, bytes_consumed)` or `None` if `buf` holds only a prefix of
/// a multibyte sequence.
fn decode_utf8(buf: &[u8]) -> Option<(char, usize)> {
    let b0 = buf[0];
    let len = if b0 < 0x80 {
        1
    } else if b0 >> 5 == 0b110 {
        2
    } else if b0 >> 4 == 0b1110 {
        3
    } else if b0 >> 3 == 0b11110 {
        4
    } else {
        // Invalid lead byte; consume it as the replacement char.
        return Some(('\u{fffd}', 1));
    };
    if buf.len() < len {
        return None; // Truncated multibyte sequence.
    }
    match std::str::from_utf8(&buf[..len]) {
        Ok(s) => s.chars().next().map(|c| (c, len)),
        Err(_) => Some(('\u{fffd}', 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(bytes: &[u8]) -> Vec<KeyEvent> {
        KeyDecoder::new().feed(bytes)
    }

    #[test]
    fn printable_ascii() {
        let evs = one(b"hi");
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0], plain(KeyCode::Char('h')));
        assert_eq!(evs[1], plain(KeyCode::Char('i')));
    }

    #[test]
    fn enter_backspace_tab() {
        assert_eq!(one(b"\r"), vec![plain(KeyCode::Enter)]);
        assert_eq!(one(b"\n"), vec![plain(KeyCode::Enter)]);
        assert_eq!(one(&[0x7f]), vec![plain(KeyCode::Backspace)]);
        assert_eq!(one(&[0x08]), vec![plain(KeyCode::Backspace)]);
        assert_eq!(one(b"\t"), vec![plain(KeyCode::Tab)]);
    }

    #[test]
    fn ctrl_c_and_ctrl_s() {
        assert_eq!(one(&[0x03]), vec![ctrl('c')]);
        assert_eq!(one(&[0x13]), vec![ctrl('s')]);
    }

    #[test]
    fn csi_arrow_keys() {
        assert_eq!(one(b"\x1b[A"), vec![plain(KeyCode::Up)]);
        assert_eq!(one(b"\x1b[B"), vec![plain(KeyCode::Down)]);
        assert_eq!(one(b"\x1b[C"), vec![plain(KeyCode::Right)]);
        assert_eq!(one(b"\x1b[D"), vec![plain(KeyCode::Left)]);
    }

    #[test]
    fn split_escape_sequence_is_buffered() {
        let mut dec = KeyDecoder::new();
        assert!(dec.feed(b"\x1b").is_empty());
        assert!(dec.feed(b"[").is_empty());
        assert_eq!(dec.feed(b"A"), vec![plain(KeyCode::Up)]);
    }

    #[test]
    fn split_utf8_is_buffered() {
        // 'é' is 0xC3 0xA9 in UTF-8.
        let mut dec = KeyDecoder::new();
        assert!(dec.feed(&[0xc3]).is_empty());
        let evs = dec.feed(&[0xa9]);
        assert_eq!(evs, vec![plain(KeyCode::Char('é'))]);
    }

    #[test]
    fn multibyte_in_one_feed() {
        let evs = one("café".as_bytes());
        let chars: Vec<char> = evs
            .iter()
            .filter_map(|e| match e.code {
                KeyCode::Char(c) => Some(c),
                _ => None,
            })
            .collect();
        assert_eq!(chars, vec!['c', 'a', 'f', 'é']);
    }
}
