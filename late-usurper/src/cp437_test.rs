use crate::cp437::*;

#[test]
fn ascii_and_escape_sequences_pass_through() {
    let ansi = b"\x1b[1;31mHello\x1b[0m\r\n";
    assert_eq!(to_utf8(ansi), ansi);
}

#[test]
fn box_drawing_and_shades_map() {
    // 0xC9 0xCD 0xBB is the classic double-line box top; 0xB0 light shade.
    assert_eq!(to_utf8(&[0xC9, 0xCD, 0xBB]), "╔═╗".as_bytes());
    assert_eq!(to_utf8(&[0xB0, 0xDB]), "░█".as_bytes());
}

#[test]
fn mixed_stream_keeps_ascii_positions() {
    let mixed = [b'a', 0xFB, b'b'];
    assert_eq!(to_utf8(&mixed), "a√b".as_bytes());
}

#[test]
fn every_high_byte_maps_to_multibyte_utf8() {
    for b in 0x80..=0xFFu8 {
        let out = to_utf8(&[b]);
        assert!(out.len() >= 2, "byte {b:#x} produced {out:?}");
        assert!(std::str::from_utf8(&out).is_ok());
    }
}
