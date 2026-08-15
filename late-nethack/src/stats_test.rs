use super::{append_cursors, frame_lines, parse_cursors};

#[test]
fn append_cursors_merges_split_requests() {
    let merged = append_cursors(None, "xlogfile:123");
    let merged = append_cursors(Some(merged), "livelog:456");
    let cursors = parse_cursors(&merged);
    assert_eq!(cursors.get("xlogfile"), Some(&123));
    assert_eq!(cursors.get("livelog"), Some(&456));
}

#[test]
fn parse_cursors_reads_both_files() {
    let cursors = parse_cursors("xlogfile:123,livelog:456");
    assert_eq!(cursors.get("xlogfile"), Some(&123));
    assert_eq!(cursors.get("livelog"), Some(&456));
}

#[test]
fn parse_cursors_skips_malformed_entries() {
    let cursors = parse_cursors("xlogfile:abc,livelog:7,junk,:9");
    assert_eq!(cursors.get("xlogfile"), None);
    assert_eq!(cursors.get("livelog"), Some(&7));
    assert_eq!(cursors.len(), 2); // "" from ":9" plus livelog
}

#[test]
fn parse_cursors_empty_value_is_empty() {
    assert!(parse_cursors("").is_empty());
}

#[test]
fn frame_lines_frames_complete_lines_with_next_cursor() {
    let (frames, consumed) = frame_lines("xlogfile", 100, b"one\ntwo\n");
    assert_eq!(consumed, 8);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "xlogfile\t104\tone\nxlogfile\t108\ttwo\n"
    );
}

#[test]
fn frame_lines_keeps_embedded_tabs_in_the_line() {
    // NetHack's own xlog fields are tab-separated; the frame must carry them
    // untouched (the client splits with `splitn(3, '\t')`).
    let (frames, consumed) = frame_lines("xlogfile", 0, b"version=5.0.0\tpoints=12\n");
    assert_eq!(consumed, 24);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "xlogfile\t24\tversion=5.0.0\tpoints=12\n"
    );
}

#[test]
fn frame_lines_leaves_trailing_partial_unconsumed() {
    let (frames, consumed) = frame_lines("livelog", 0, b"done\npart");
    assert_eq!(consumed, 5);
    assert_eq!(String::from_utf8(frames).unwrap(), "livelog\t5\tdone\n");
}

#[test]
fn frame_lines_all_partial_consumes_nothing() {
    let (frames, consumed) = frame_lines("xlogfile", 42, b"no newline yet");
    assert_eq!(consumed, 0);
    assert!(frames.is_empty());
}
