use super::*;

fn frame(tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

fn reset(cols: u16, rows: u16, contents: &[u8]) -> Vec<u8> {
    let mut payload = cols.to_be_bytes().to_vec();
    payload.extend_from_slice(&rows.to_be_bytes());
    payload.extend_from_slice(contents);
    frame(b'R', &payload)
}

#[test]
fn frames_split_across_chunks_decode_whole() {
    let mut wire = reset(100, 30, b"@..g");
    wire.extend(frame(b'D', b"\x1b[1;5H>"));
    let mut decoder = FrameDecoder::default();
    let mut frames = Vec::new();
    for chunk in wire.chunks(3) {
        frames.extend(decoder.push(chunk).unwrap());
    }
    assert_eq!(
        frames,
        vec![
            WatchFrame::Reset {
                cols: 100,
                rows: 30,
                contents: b"@..g".to_vec(),
            },
            WatchFrame::Diff(b"\x1b[1;5H>".to_vec()),
        ]
    );
}

/// A reset sizes the parser to the player's terminal, not this session's,
/// and a diff then lands on top of it.
#[test]
fn applied_frames_hold_the_players_screen_at_the_players_size() {
    let mut parser = vt100::Parser::new(24, 80, 0);
    let mut decoder = FrameDecoder::default();
    let mut wire = reset(100, 30, b"@..g");
    wire.extend(frame(b'D', b"\x1b[1;5H>"));
    for frame in decoder.push(&wire).unwrap() {
        frame.apply(&mut parser);
    }
    assert_eq!(parser.screen().size(), (30, 100));
    assert_eq!(parser.screen().contents().lines().next(), Some("@..g>"));
}

#[test]
fn a_broken_frame_stream_is_an_error() {
    assert_eq!(
        FrameDecoder::default().push(&frame(b'X', b"")),
        Err(WireError::UnknownFrameTag(b'X'))
    );
    assert_eq!(
        FrameDecoder::default().push(&frame(b'R', b"\x00")),
        Err(WireError::ShortResetFrame)
    );
    let mut oversized = vec![b'D'];
    oversized.extend_from_slice(&u32::MAX.to_be_bytes());
    assert_eq!(
        FrameDecoder::default().push(&oversized),
        Err(WireError::OversizedFrame(u32::MAX as usize))
    );
}

#[test]
fn roster_blocks_decode_across_chunks() {
    let wire = b"game\talice\t100\t2\ngame\tbob\t200\t0\nend\nend\n";
    let mut decoder = RosterDecoder::default();
    let mut rosters = Vec::new();
    for chunk in wire.chunks(5) {
        rosters.extend(decoder.push(chunk).unwrap());
    }
    assert_eq!(
        rosters,
        vec![
            vec![
                LiveGame {
                    playname: "alice".to_string(),
                    started_unix: 100,
                    watchers: 2,
                },
                LiveGame {
                    playname: "bob".to_string(),
                    started_unix: 200,
                    watchers: 0,
                },
            ],
            // The last game ended: an empty block clears the roster.
            vec![],
        ]
    );
}

#[test]
fn a_malformed_roster_line_is_an_error() {
    assert!(matches!(
        RosterDecoder::default().push(b"game\talice\tsoon\t2\n"),
        Err(WireError::MalformedRosterLine(_))
    ));
    assert!(matches!(
        RosterDecoder::default().push(b"player\talice\n"),
        Err(WireError::MalformedRosterLine(_))
    ));
}
