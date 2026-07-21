use super::*;

#[test]
fn parse_tracks_single_object_source() {
    let json = r#"{
        "icestats": {
            "source": {
                "listenurl": "http://localhost:8000/chill",
                "title": "My Artist - My Song | 180"
            }
        }
    }"#;

    let tracks = parse_tracks(json).unwrap();
    assert_eq!(tracks.len(), 1);
    let track = &tracks["chill"];
    assert_eq!(track.artist.as_deref(), Some("My Artist"));
    assert_eq!(track.title, "My Song");
    assert_eq!(track.duration_seconds, Some(180));
}

#[test]
fn parse_tracks_two_element_array() {
    let json = r#"{
        "icestats": {
            "source": [
                {
                    "listenurl": "http://localhost:8000/chill",
                    "title": "Lofi Artist - Lofi Song | 120"
                },
                {
                    "listenurl": "http://localhost:8000/classical",
                    "title": "Composer - Sonata"
                }
            ]
        }
    }"#;

    let tracks = parse_tracks(json).unwrap();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks["chill"].title, "Lofi Song");
    assert_eq!(tracks["chill"].duration_seconds, Some(120));
    assert_eq!(tracks["classical"].artist.as_deref(), Some("Composer"));
    assert_eq!(tracks["classical"].title, "Sonata");
    assert!(tracks["classical"].duration_seconds.is_none());
}

#[test]
fn parse_tracks_skips_source_without_listenurl() {
    let json = r#"{
        "icestats": {
            "source": [
                { "title": "Orphan - Track" },
                {
                    "listenurl": "http://localhost:8000/classical",
                    "title": "Composer - Sonata"
                }
            ]
        }
    }"#;

    let tracks = parse_tracks(json).unwrap();
    assert_eq!(tracks.len(), 1);
    assert!(tracks.contains_key("classical"));
}

#[test]
fn parse_tracks_unknown_mount_lookup_is_none() {
    let json = r#"{
        "icestats": {
            "source": {
                "listenurl": "http://localhost:8000/chill",
                "title": "My Artist - My Song"
            }
        }
    }"#;

    let tracks = parse_tracks(json).unwrap();
    assert!(!tracks.contains_key("jazz"));
}

#[test]
fn parse_tracks_missing_title_falls_back() {
    let json = r#"{
        "icestats": {
            "source": { "listenurl": "http://localhost:8000/chill" }
        }
    }"#;

    let tracks = parse_tracks(json).unwrap();
    let track = &tracks["chill"];
    assert_eq!(track.title, "Unknown Track");
    assert_eq!(track.artist.as_deref(), Some("Unknown"));
}

#[test]
fn parse_tracks_no_source() {
    let json = r#"{
        "icestats": {
            "admin": "admin@localhost",
            "dummy": null
        }
    }"#;

    let tracks = parse_tracks(json).unwrap();
    assert!(tracks.is_empty());
}

#[test]
fn parse_tracks_invalid_json() {
    assert!(parse_tracks("not json").is_err());
}

#[test]
fn parse_track_title_multiple_dashes() {
    let track = parse_track_title(Some("A - B - C | 60".to_string()));
    // split_once on " - " gives artist="A", title="B - C"
    assert_eq!(track.artist.as_deref(), Some("A"));
    assert_eq!(track.title, "B - C");
    assert_eq!(track.duration_seconds, Some(60));
}

#[test]
fn parse_track_title_non_numeric_duration() {
    let track = parse_track_title(Some("Artist - Title | abc".to_string()));
    assert_eq!(track.artist.as_deref(), Some("Artist"));
    assert_eq!(track.title, "Title");
    assert!(track.duration_seconds.is_none());
}

#[test]
fn mount_name_extracts_last_segment() {
    assert_eq!(mount_name("http://localhost:8000/chill"), Some("chill"));
    assert_eq!(
        mount_name("http://localhost:8000/classical/"),
        Some("classical")
    );
    assert_eq!(mount_name("http://localhost:8000"), None);
}
