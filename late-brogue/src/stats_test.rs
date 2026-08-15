use super::{append_cursors, frame_lines, parse_cursors, valid_playname_dir};

#[test]
fn append_cursors_merges_split_requests() {
    let merged = append_cursors(None, "players/mat/BrogueRunHistory.txt:123");
    let merged = append_cursors(Some(merged), "players/zed_2/BrogueRunHistory.txt:456");
    let cursors = parse_cursors(&merged);
    assert_eq!(cursors.get("players/mat/BrogueRunHistory.txt"), Some(&123));
    assert_eq!(
        cursors.get("players/zed_2/BrogueRunHistory.txt"),
        Some(&456)
    );
}

#[test]
fn parse_cursors_reads_path_shaped_ids() {
    let cursors = parse_cursors(
        "players/mat/BrogueRunHistory.txt:123,players/zed_2/BrogueRunHistory.txt:456",
    );
    assert_eq!(cursors.get("players/mat/BrogueRunHistory.txt"), Some(&123));
    assert_eq!(
        cursors.get("players/zed_2/BrogueRunHistory.txt"),
        Some(&456)
    );
}

#[test]
fn parse_cursors_skips_malformed_entries() {
    let cursors =
        parse_cursors("players/a/BrogueRunHistory.txt:abc,players/b/BrogueRunHistory.txt:7,junk");
    assert_eq!(cursors.get("players/a/BrogueRunHistory.txt"), None);
    assert_eq!(cursors.get("players/b/BrogueRunHistory.txt"), Some(&7));
    assert_eq!(cursors.len(), 1);
}

#[test]
fn parse_cursors_empty_value_is_empty() {
    assert!(parse_cursors("").is_empty());
}

#[test]
fn valid_playname_dir_accepts_sanitize_output() {
    assert!(valid_playname_dir("mat"));
    assert!(valid_playname_dir("Zed_2"));
    assert!(valid_playname_dir("late_stats"));
}

#[test]
fn valid_playname_dir_rejects_frame_breaking_names() {
    // These bytes would corrupt the frame (`\t`) or cursor (`:`/`,`)
    // encodings, or escape the players dir (`/`, `.`); sanitize can never
    // produce them, so a dir carrying one was not created by this host.
    assert!(!valid_playname_dir(""));
    assert!(!valid_playname_dir("a:b"));
    assert!(!valid_playname_dir("a,b"));
    assert!(!valid_playname_dir("a\tb"));
    assert!(!valid_playname_dir("a/b"));
    assert!(!valid_playname_dir(".."));
    assert!(!valid_playname_dir("a-b"));
}

#[test]
fn frame_lines_frames_complete_lines_with_next_cursor() {
    let (frames, consumed) = frame_lines("players/mat/BrogueRunHistory.txt", 100, b"one\ntwo\n");
    assert_eq!(consumed, 8);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "players/mat/BrogueRunHistory.txt\t104\tone\nplayers/mat/BrogueRunHistory.txt\t108\ttwo\n"
    );
}

#[test]
fn frame_lines_keeps_embedded_tabs_in_the_line() {
    // Brogue's own run-history fields are tab-separated; the frame must carry
    // them untouched (the client splits with `splitn(3, '\t')`).
    let (frames, consumed) =
        frame_lines("players/mat/BrogueRunHistory.txt", 0, b"123\t456\tDied\n");
    assert_eq!(consumed, 13);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "players/mat/BrogueRunHistory.txt\t13\t123\t456\tDied\n"
    );
}

#[test]
fn frame_lines_leaves_trailing_partial_unconsumed() {
    let (frames, consumed) = frame_lines("players/a/BrogueRunHistory.txt", 0, b"done\npart");
    assert_eq!(consumed, 5);
    assert_eq!(
        String::from_utf8(frames).unwrap(),
        "players/a/BrogueRunHistory.txt\t5\tdone\n"
    );
}

#[test]
fn frame_lines_all_partial_consumes_nothing() {
    let (frames, consumed) = frame_lines("players/a/BrogueRunHistory.txt", 42, b"no newline yet");
    assert_eq!(consumed, 0);
    assert!(frames.is_empty());
}
