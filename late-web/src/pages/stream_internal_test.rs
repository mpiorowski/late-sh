use super::*;

#[test]
fn normalize_mount_falls_back_to_chill_for_unknown_mounts() {
    assert_eq!(normalize_mount("classical"), "classical");
    assert_eq!(normalize_mount("chill"), "chill");
    assert_eq!(normalize_mount("dubstep"), "chill");
    assert_eq!(normalize_mount(""), "chill");
}

#[test]
fn upstream_stream_url_appends_suffix_once() {
    assert_eq!(
        upstream_stream_url("http://icecast:8000", "chill"),
        "http://icecast:8000/chill"
    );
    assert_eq!(
        upstream_stream_url("http://icecast:8000/classical", "classical"),
        "http://icecast:8000/classical"
    );
}

#[test]
fn silence_chunk_cycles_without_empty_output() {
    let mut offset = SILENCE_MP3.len().saturating_sub(16);
    let first = next_silence_chunk(&mut offset);
    let second = next_silence_chunk(&mut offset);

    assert!(!first.is_empty());
    assert!(!second.is_empty());
}
