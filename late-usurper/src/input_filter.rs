//! Authoritative, stream-stateful input filter for the Usurper PTY.
//!
//! This is the security boundary for client input, not the door's client-side
//! strip. In DOOR32 local mode the player's keyboard IS the game's sysop
//! console: DDPlus reads the function keys as local sysop commands, F2 (forced
//! chat), F7/F8 (`time_credit += 5` / `-= 5`), and F10 (`HosedMessage; halt`,
//! terminate the node). Letting any of those reach the child hands every player
//! the sysop panel and a self-terminate. So F1-F12 in every encoding the child
//! could reconstruct must be dropped here, at the host, where the bytes are
//! about to be written to the child's tty. The door also strips them, but that
//! is best-effort noise reduction: a raw SSH client to this host (or a client
//! that splits a sequence across chunks) would bypass a client-only filter.
//!
//! Statefulness is the whole point. A malicious client can split `F10`
//! (`ESC [ 21 ~`) across two SSH data chunks; a stateless per-chunk filter
//! forwards each fragment, and the child's reader (which pulls one byte at a
//! time) reconstructs the key. This filter instead retains an incomplete
//! escape-sequence prefix at a chunk boundary and resolves it when the rest
//! arrives.
//!
//! It also folds in the byte-clean gate the child needs: bytes >= 0x80 are
//! dropped (the game reads CP437/ASCII; stray high bytes from a UTF-8 client
//! would be misread as glyph codes).
//!
//! Tradeoff: a lone `Esc` (a real key the game uses for menu-back / quit-chat)
//! that lands at the very end of a chunk is held until the next byte arrives,
//! since we cannot yet tell it from the start of `ESC [ ...`. This is the
//! standard terminal "ESC ambiguity"; in practice a real Esc arrives as its own
//! write and is released the instant the next key is pressed.

/// Upper bound on a held incomplete prefix. Every strippable sequence we care
/// about (all F-key encodings, paste markers) is <= 6 bytes, so a real
/// protected key always resolves well under this. The cap only bites on
/// genuinely unbounded junk (e.g. an SGR-mouse prefix whose terminator never
/// comes); at that point the held bytes are flushed as passthrough, which is
/// harmless because such a prefix is not a protected key.
const MAX_PENDING: usize = 32;

/// One outcome of classifying the bytes starting at an `ESC`.
enum Match {
    /// Bytes `[0, n)` are a complete sequence to drop.
    Strip(usize),
    /// Bytes `[0, n)` are a complete sequence to forward verbatim.
    Pass(usize),
    /// Not enough bytes yet to decide; retain everything and wait.
    Incomplete,
}

/// Per-connection filter state: the tail of the previous chunk when it ended
/// mid escape-sequence.
#[derive(Default)]
pub(crate) struct InputFilter {
    pending: Vec<u8>,
}

impl InputFilter {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feed one inbound chunk; return the bytes safe to write to the child.
    pub(crate) fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        // Work over the retained prefix followed by the new chunk.
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(chunk);

        let mut out = Vec::with_capacity(buf.len());
        let mut i = 0;
        while i < buf.len() {
            let b = buf[i];
            if b != 0x1b {
                // ASCII gate: drop high bytes, forward the rest.
                if b < 0x80 {
                    out.push(b);
                }
                i += 1;
                continue;
            }
            match classify(&buf[i..]) {
                Match::Strip(n) => i += n,
                Match::Pass(n) => {
                    // Escape sequences are ASCII; forward as-is.
                    out.extend_from_slice(&buf[i..i + n]);
                    i += n;
                }
                Match::Incomplete => {
                    let tail = &buf[i..];
                    if tail.len() > MAX_PENDING {
                        // Unresolvable junk: it is not a protected key (those
                        // resolve well under the cap), so flush it verbatim
                        // through the ASCII gate rather than buffer forever.
                        out.extend(tail.iter().copied().filter(|c| *c < 0x80));
                    } else {
                        self.pending = tail.to_vec();
                    }
                    break;
                }
            }
        }
        out
    }
}

/// Classify the bytes starting at `s[0] == ESC`.
fn classify(s: &[u8]) -> Match {
    debug_assert_eq!(s[0], 0x1b);
    if s.len() < 2 {
        return Match::Incomplete; // could be ESC O.. / ESC [.. / lone Esc
    }
    match s[1] {
        // SS3: ESC O x. F1-F4 are P/Q/R/S (strip); anything else (app-mode
        // nav like ESC O H) passes as a 3-byte sequence.
        b'O' => {
            if s.len() < 3 {
                Match::Incomplete
            } else if matches!(s[2], b'P' | b'Q' | b'R' | b'S') {
                Match::Strip(3)
            } else {
                Match::Pass(3)
            }
        }
        b'[' => classify_csi(&s[2..]),
        // ESC + ordinary byte (meta/Alt combos): forward just the ESC and let
        // the next byte be reprocessed normally.
        _ => Match::Pass(1),
    }
}

/// Classify bytes after a `ESC [` prefix. `rest` is everything past the `[`.
fn classify_csi(rest: &[u8]) -> Match {
    if rest.is_empty() {
        return Match::Incomplete;
    }
    // Legacy X10 mouse: ESC [ M <button> <x> <y> (three raw bytes after M,
    // which break the standard CSI grammar, so handle it first).
    if rest[0] == b'M' {
        return if rest.len() >= 4 {
            Match::Strip(2 + 4)
        } else {
            Match::Incomplete
        };
    }
    // Linux console F1-F5: ESC [ [ A..E.
    if rest[0] == b'[' {
        if rest.len() < 2 {
            return Match::Incomplete;
        }
        return if matches!(rest[1], b'A'..=b'E') {
            Match::Strip(4)
        } else {
            // ESC [ [ X (X not A-E): forward the 3 bytes, reprocess X.
            Match::Pass(3)
        };
    }
    // Standard CSI: parameter bytes (0x30-0x3f) then intermediates (0x20-0x2f)
    // then a final byte (0x40-0x7e).
    let mut j = 0;
    while j < rest.len() && (0x30..=0x3f).contains(&rest[j]) {
        j += 1;
    }
    let param_end = j;
    while j < rest.len() && (0x20..=0x2f).contains(&rest[j]) {
        j += 1;
    }
    if j >= rest.len() {
        return Match::Incomplete; // no final byte yet
    }
    let final_b = rest[j];
    if !(0x40..=0x7e).contains(&final_b) {
        // Malformed (a control byte inside the CSI): forward ESC [ and let the
        // stray byte be reprocessed.
        return Match::Pass(2);
    }
    let total = 2 + j + 1; // ESC [ + params/intermediates + final
    let params = &rest[..param_end];

    // SGR mouse: ESC [ < ... M/m.
    if params.first() == Some(&b'<') && (final_b == b'M' || final_b == b'm') {
        return Match::Strip(total);
    }
    // Tilde-terminated: F-keys, bracketed paste, and numeric nav keys share
    // this shape (ESC [ <num> ~). Strip the F-key codes (11-15, 17-21, 23, 24)
    // and the paste markers (200/201); pass the nav codes (1-8: Home/Ins/Del/
    // End/PgUp/PgDn).
    if final_b == b'~' {
        if let Some(code) = parse_code(params)
            && matches!(code, 11..=15 | 17..=21 | 23 | 24 | 200 | 201)
        {
            return Match::Strip(total);
        }
        return Match::Pass(total);
    }
    // Everything else (arrows ESC [ A, modified arrows ESC [ 1;2 A, etc.).
    Match::Pass(total)
}

/// Parse a pure-decimal CSI parameter (e.g. `21`). Returns `None` for anything
/// with separators or non-digits (`1;2`), which is never a protected key.
fn parse_code(params: &[u8]) -> Option<u16> {
    if params.is_empty() || !params.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut code: u32 = 0;
    for &d in params {
        code = code.checked_mul(10)?.checked_add((d - b'0') as u32)?;
        if code > u16::MAX as u32 {
            return None;
        }
    }
    Some(code as u16)
}
