use crate::input_filter::*;

/// Feed `input` through a fresh filter split at boundary `k`, returning the
/// concatenated output. Splitting is how a malicious client evades a stateless
/// filter, so the invariant under test is that the split point never changes
/// the result.
fn through_split(input: &[u8], k: usize) -> Vec<u8> {
    let mut f = InputFilter::new();
    let mut out = f.push(&input[..k]);
    out.extend(f.push(&input[k..]));
    out
}

/// Assert that `input` produces `expected` no matter where it is split, and
/// also when fed whole. Catches any chunk boundary that leaks a protected key.
fn assert_every_split(input: &[u8], expected: &[u8]) {
    let whole = InputFilter::new().push(input);
    assert_eq!(whole, expected, "whole: {input:02x?}");
    for k in 0..=input.len() {
        assert_eq!(
            through_split(input, k),
            expected,
            "split at {k}: {input:02x?}"
        );
    }
}

// The load-bearing sysop keys DDPlus binds in local mode: F2 chat, F7/F8 time
// credit, F10 terminate. Each must vanish at every split.
const PROTECTED: &[&[u8]] = &[
    b"\x1bOP",   // F1 (SS3)
    b"\x1bOQ",   // F2 (SS3)
    b"\x1bOR",   // F3 (SS3)
    b"\x1bOS",   // F4 (SS3)
    b"\x1b[11~", // F1 (CSI)
    b"\x1b[12~", // F2
    b"\x1b[15~", // F5
    b"\x1b[17~", // F6
    b"\x1b[18~", // F7
    b"\x1b[19~", // F8
    b"\x1b[20~", // F9
    b"\x1b[21~", // F10 -> HosedMessage; halt
    b"\x1b[23~", // F11
    b"\x1b[24~", // F12
    b"\x1b[[A",  // F1 (linux console)
    b"\x1b[[E",  // F5 (linux console)
];

#[test]
fn every_protected_key_is_dropped_at_every_split() {
    for seq in PROTECTED {
        assert_every_split(seq, b"");
    }
}

#[test]
fn f10_split_across_chunks_cannot_reach_the_child() {
    // The exact reviewer scenario: ESC alone, then the rest.
    let mut f = InputFilter::new();
    assert_eq!(f.push(b"\x1b"), b"");
    assert_eq!(f.push(b"[21~"), b"");
    // And byte-by-byte.
    let mut f = InputFilter::new();
    for b in b"\x1b[21~" {
        assert!(f.push(&[*b]).is_empty());
    }
}

#[test]
fn protected_keys_dropped_amid_real_typing() {
    // F10 wedged between real keystrokes: the keystrokes survive, F10 does not.
    assert_every_split(b"a\x1b[21~b", b"ab");
    assert_every_split(b"look\x1bOQmore", b"lookmore");
}

#[test]
fn mouse_and_paste_reports_are_dropped() {
    assert_every_split(b"\x1b[<35;10;5M", b""); // SGR mouse press
    assert_every_split(b"\x1b[<35;10;5m", b""); // SGR mouse release
    assert_every_split(b"\x1b[M\x20\x21\x22", b""); // X10 mouse
    assert_every_split(b"\x1b[200~hi\x1b[201~", b"hi"); // bracketed paste
}

#[test]
fn navigation_keys_pass_through_at_every_split() {
    // Arrows and the numeric nav keys the game needs must survive intact.
    assert_every_split(b"\x1b[A", b"\x1b[A"); // up
    assert_every_split(b"\x1b[B", b"\x1b[B"); // down
    assert_every_split(b"\x1b[C\x1b[D", b"\x1b[C\x1b[D"); // right, left
    assert_every_split(b"\x1b[H", b"\x1b[H"); // Home
    assert_every_split(b"\x1b[3~", b"\x1b[3~"); // Delete
    assert_every_split(b"\x1b[5~\x1b[6~", b"\x1b[5~\x1b[6~"); // PgUp, PgDn
    assert_every_split(b"\x1b[1;2A", b"\x1b[1;2A"); // shift-up (modified)
}

#[test]
fn ordinary_keys_and_control_chars_pass_through() {
    assert_every_split(b"hjkl", b"hjkl");
    assert_every_split(b"1\r2\n", b"1\r2\n");
    // Ctrl-S must reach the game (it binds it); it is not flow control here.
    assert_every_split(b"\x13", b"\x13");
}

#[test]
fn lone_escape_passes_through_once_resolved() {
    // A bare Esc followed by an ordinary key (the game's menu-back): the Esc is
    // held at a chunk end but released as soon as the next byte disambiguates.
    assert_every_split(b"\x1bx", b"\x1bx");
    // Two escapes then a key (mashing Esc): all three survive once resolved.
    assert_every_split(b"\x1b\x1bx", b"\x1b\x1bx");
}

#[test]
fn a_lone_trailing_escape_is_held_not_forwarded() {
    // At a hard chunk end we cannot yet tell a real Esc from the start of a
    // protected sequence, so it is retained rather than flushed. This is the
    // documented ESC-ambiguity tradeoff; it is released on the next push.
    let mut f = InputFilter::new();
    assert_eq!(f.push(b"\x1b"), b"");
    assert_eq!(f.push(b"x"), b"\x1bx");
}

#[test]
fn high_bytes_are_dropped_ascii_gate() {
    // The child reads CP437/ASCII; stray high bytes from a UTF-8 client would
    // be misread as glyph codes.
    let mut f = InputFilter::new();
    assert_eq!(f.push(&[0xc3, 0xa9, b'a']), b"a"); // 'e-acute' UTF-8 -> dropped
}

#[test]
fn unterminated_mouse_prefix_is_bounded_not_buffered_forever() {
    // A client that opens an SGR-mouse sequence and never terminates it must
    // not grow pending without bound; past the cap the junk is flushed (it is
    // not a protected key) rather than retained.
    let mut f = InputFilter::new();
    let junk: Vec<u8> = std::iter::once(0x1b)
        .chain(b"[<".iter().copied())
        .chain(std::iter::repeat_n(b'9', 64))
        .collect();
    let out = f.push(&junk);
    // Something is released (the cap fired); nothing is still pending-looped.
    assert!(!out.is_empty());
    // A protected key afterwards is still caught.
    assert_eq!(f.push(b"\x1b[21~"), b"");
}
