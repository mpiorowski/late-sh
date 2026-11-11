use super::{append_cursors, frame_lines, parse_cursors};

#[test]
fn cursors_round_trip_through_split_requests() {
    let merged = append_cursors(None, "runs:123");
    let merged = append_cursors(Some(merged), "other:456");
    let cursors = parse_cursors(&merged);
    assert_eq!(cursors.get("runs"), Some(&123));
    assert_eq!(cursors.get("other"), Some(&456));
}

#[test]
fn parse_cursors_skips_malformed_entries() {
    let cursors = parse_cursors("runs:abc,junk,:9");
    assert_eq!(cursors.get("runs"), None);
    assert_eq!(cursors.len(), 1); // only the "" id from ":9"
}

#[test]
fn frame_lines_frames_complete_lines_with_next_cursor() {
    let (frames, consumed) = frame_lines("runs", 100, b"one\ntwo\n");
    assert_eq!(consumed, 8);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "runs\t104\tone\nruns\t108\ttwo\n"
    );
}

#[test]
fn frame_lines_keeps_embedded_tabs_in_the_line() {
    // The run log's own fields are tab-separated; the frame must carry them
    // untouched (the client splits with `splitn(3, '\t')`).
    let (frames, consumed) = frame_lines("runs", 0, b"Alice\t1234\t0\t99\t01-02-2026\n");
    assert_eq!(consumed, 27);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "runs\t27\tAlice\t1234\t0\t99\t01-02-2026\n"
    );
}

#[test]
fn frame_lines_leaves_trailing_partial_unconsumed() {
    let (frames, consumed) = frame_lines("runs", 0, b"done\npart");
    assert_eq!(consumed, 5);
    assert_eq!(String::from_utf8(frames).unwrap(), "runs\t5\tdone\n");
}

#[test]
fn frame_lines_all_partial_consumes_nothing() {
    let (frames, consumed) = frame_lines("runs", 42, b"no newline yet");
    assert_eq!(consumed, 0);
    assert!(frames.is_empty());
}
