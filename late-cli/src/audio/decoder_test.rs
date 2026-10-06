use super::*;

#[test]
fn resolve_stream_url_appends_icecast_mount() {
    assert_eq!(
        resolve_stream_url("http://audio.late.sh"),
        "http://audio.late.sh/stream"
    );
    assert_eq!(
        resolve_stream_url("http://audio.late.sh/"),
        "http://audio.late.sh/stream"
    );
}

#[test]
fn resolve_stream_url_preserves_mount_or_direct_url() {
    assert_eq!(
        resolve_stream_url("http://audio.late.sh/stream"),
        "http://audio.late.sh/stream"
    );
    assert_eq!(
        resolve_stream_url("https://late.sh/stream/chill"),
        "https://late.sh/stream/chill"
    );
    assert_eq!(
        resolve_stream_url("https://stream.nightride.fm/chillsynth.m4a"),
        "https://stream.nightride.fm/chillsynth.m4a"
    );
}

#[test]
fn find_mp3_sync_offset_finds_frame_after_garbage() {
    let mut bytes = vec![0x12, 0x34, 0x56, 0x78];
    bytes.extend_from_slice(&[0xFF, 0xFB, 0x90, 0x64, 0x00, 0x00]);
    assert_eq!(find_mp3_sync_offset(&bytes), Some(4));
}

#[test]
fn find_mp3_sync_offset_accepts_id3_header() {
    assert_eq!(find_mp3_sync_offset(b"ID3\x04\x00\x00"), Some(0));
}

#[test]
fn find_mp3_sync_offset_checks_last_possible_offset() {
    let bytes = [0x00, 0xFF, 0xFB, 0x90];
    assert_eq!(find_mp3_sync_offset(&bytes), Some(1));
}
