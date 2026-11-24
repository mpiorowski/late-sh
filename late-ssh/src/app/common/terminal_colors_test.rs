use crate::app::common::terminal_colors::*;

#[test]
fn strips_color_replies_and_reads_them() {
    // Ghostty, xterm and kitty answer with four digits a channel, ST.
    let (input, reports) =
        strip_reports(b"\x1b]10;rgb:1010/0f0f/0f0f\x1b\\\x1b]11;rgb:ffff/fcfc/f0f0\x1b\\");
    assert!(input.is_empty());
    assert_eq!(reports.fg, Some((0x10, 0x0F, 0x0F)));
    assert_eq!(reports.bg, Some((0xFF, 0xFC, 0xF0)));
    assert!(!reports.palette_changed);

    // Two digits, BEL; rxvt-unicode's rgba with an alpha that is ignored.
    let (_, reports) =
        strip_reports(b"\x1b]10;rgb:10/0f/0f\x07\x1b]11;rgba:0000/8080/ffff/c000\x07");
    assert_eq!(reports.fg, Some((0x10, 0x0F, 0x0F)));
    assert_eq!(reports.bg, Some((0, 0x80, 0xFF)));
}

#[test]
fn keys_around_a_report_keep_their_order() {
    let (input, reports) = strip_reports(b"j\x1b[?997;1nk\x1b]11;rgb:00/00/00\x1b\\\x1b[A");
    assert_eq!(&*input, b"jk\x1b[A");
    assert!(reports.palette_changed);
    assert_eq!(reports.bg, Some((0, 0, 0)));
}

#[test]
fn input_without_reports_is_borrowed_untouched() {
    for data in [
        &b"hello"[..],
        b"\x1b[A\x1b[?62;4c",
        b"\x1b]12;rgb:00/00/00\x1b\\",
    ] {
        let (input, reports) = strip_reports(data);
        assert!(matches!(input, std::borrow::Cow::Borrowed(_)), "{data:?}");
        assert_eq!(&*input, data);
        assert_eq!(reports, Reports::default());
    }
}

#[test]
fn a_reply_with_an_unreadable_color_is_dropped_without_a_report() {
    for data in [
        &b"\x1b]11;rgb:zz/00/00\x1b\\"[..],
        b"\x1b]11;rgb:00/00\x1b\\",
        b"\x1b]11;rgb:00000/00/00\x1b\\",
        b"\x1b]11;#ffffff\x1b\\",
    ] {
        let (input, reports) = strip_reports(data);
        assert!(input.is_empty(), "{data:?}");
        assert_eq!(reports, Reports::default(), "{data:?}");
    }
}

#[test]
fn an_incomplete_report_passes_through_as_input() {
    // Cut by a read boundary, or never closed: not provably the terminal's.
    for data in [
        &b"\x1b]11;rgb:ff/ff"[..],
        b"\x1b]10;",
        b"\x1b]1",
        b"\x1b[?997;",
        b"\x1b[?997;1",
        b"\x1b[?997n",
        b"\x1b[?997;1234n",
    ] {
        let (input, reports) = strip_reports(data);
        assert_eq!(&*input, data);
        assert_eq!(reports, Reports::default());
    }
    // An opened reply never closed costs a bounded look, then reads as keys.
    let mut long = b"\x1b]10;".to_vec();
    long.extend(std::iter::repeat_n(b'a', 4096));
    long.extend_from_slice(b"\x07");
    let (input, _) = strip_reports(&long);
    assert_eq!(&*input, &long[..]);
}

#[test]
fn a_flood_of_notices_queues_one_query() {
    let notices = b"\x1b[?997;2n".repeat(1000);
    let (input, reports) = strip_reports(&notices);
    assert!(input.is_empty());
    assert!(reports.palette_changed);

    let mut pending = vec![b"\x1b]111\x1b\\".to_vec()];
    queue_query(&mut pending);
    queue_query(&mut pending);
    assert_eq!(pending, vec![b"\x1b]111\x1b\\".to_vec(), QUERY.to_vec()]);
}
