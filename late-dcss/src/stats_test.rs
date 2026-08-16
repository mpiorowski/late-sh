use super::{append_cursors, frame_lines, parse_cursors};

#[test]
fn append_cursors_merges_split_requests() {
    let merged = append_cursors(None, "logfile:123");
    let merged = append_cursors(Some(merged), "milestones:456");
    let cursors = parse_cursors(&merged);
    assert_eq!(cursors.get("logfile"), Some(&123));
    assert_eq!(cursors.get("milestones"), Some(&456));
}

#[test]
fn parse_cursors_reads_both_files() {
    let cursors = parse_cursors("logfile:123,milestones:456");
    assert_eq!(cursors.get("logfile"), Some(&123));
    assert_eq!(cursors.get("milestones"), Some(&456));
}

#[test]
fn parse_cursors_skips_malformed_entries() {
    let cursors = parse_cursors("logfile:abc,milestones:7,junk,:9");
    assert_eq!(cursors.get("logfile"), None);
    assert_eq!(cursors.get("milestones"), Some(&7));
    assert_eq!(cursors.len(), 2); // "" from ":9" plus milestones
}

#[test]
fn parse_cursors_empty_value_is_empty() {
    assert!(parse_cursors("").is_empty());
}

#[test]
fn frame_lines_frames_complete_lines_with_next_cursor() {
    let (frames, consumed) = frame_lines("logfile", 100, b"one\ntwo\n");
    assert_eq!(consumed, 8);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "logfile\t104\tone\nlogfile\t108\ttwo\n"
    );
}

#[test]
fn frame_lines_leaves_trailing_partial_unconsumed() {
    let (frames, consumed) = frame_lines("milestones", 0, b"done\npart");
    assert_eq!(consumed, 5);
    assert_eq!(String::from_utf8(frames).unwrap(), "milestones\t5\tdone\n");
}

#[test]
fn frame_lines_all_partial_consumes_nothing() {
    let (frames, consumed) = frame_lines("logfile", 42, b"no newline yet");
    assert_eq!(consumed, 0);
    assert!(frames.is_empty());
}
