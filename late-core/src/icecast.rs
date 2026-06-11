use crate::api_types::Track;
use anyhow::{Context, Result};
use std::collections::HashMap;

#[derive(serde::Deserialize)]
struct Source {
    title: Option<String>,
    listenurl: Option<String>,
}

// Icecast's /status-json.xsl renders `source` as a single object with one
// mount and as an array with two or more.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum SourceField {
    One(Source),
    Many(Vec<Source>),
}

#[derive(serde::Deserialize)]
struct IceStats {
    source: Option<SourceField>,
}

#[derive(serde::Deserialize)]
struct StatusRoot {
    icestats: IceStats,
}

/// Fetch the current track for every mount, keyed by mount name (the last
/// path segment of the source's `listenurl`, e.g. `chill`, `classical`).
pub fn fetch_tracks(url: &str) -> Result<HashMap<String, Track>> {
    let status_url = url.to_string() + "/status-json.xsl";
    let body = reqwest::blocking::get(status_url)
        .context("fetching icecast status")?
        .text()
        .context("reading icecast status body")?;

    parse_tracks(&body)
}

fn parse_tracks(body: &str) -> Result<HashMap<String, Track>> {
    let parsed: StatusRoot = serde_json::from_str(body).context("parsing icecast status json")?;

    let sources = match parsed.icestats.source {
        Some(SourceField::One(source)) => vec![source],
        Some(SourceField::Many(sources)) => sources,
        None => Vec::new(),
    };

    let mut tracks = HashMap::new();
    for source in sources {
        let Some(mount) = source.listenurl.as_deref().and_then(mount_name) else {
            continue;
        };
        tracks.insert(mount.to_string(), parse_track_title(source.title));
    }
    Ok(tracks)
}

fn mount_name(listenurl: &str) -> Option<&str> {
    let segment = listenurl.trim_end_matches('/').rsplit('/').next()?;
    (!segment.is_empty() && !segment.contains(':')).then_some(segment)
}

fn parse_track_title(title: Option<String>) -> Track {
    let full_title = title.unwrap_or_else(|| "Unknown - Unknown Track".to_string());

    // Format: "Artist - Title | Duration"

    // 1. Extract Duration if present
    let (metadata, duration_seconds) = if let Some((rest, dur_str)) = full_title.rsplit_once(" | ")
    {
        let dur = dur_str.parse::<u64>().ok();
        (rest, dur)
    } else {
        (full_title.as_str(), None)
    };

    // 2. Extract Artist and Title
    // We split once by " - ". If not found, assume entire string is Title and Artist is Unknown.
    let (artist, title) = if let Some((a, t)) = metadata.split_once(" - ") {
        (Some(a.trim().to_string()), t.trim().to_string())
    } else {
        (None, metadata.trim().to_string())
    };

    Track {
        title,
        artist,
        duration_seconds,
    }
}

#[cfg(test)]
mod tests {
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
        assert!(tracks.get("jazz").is_none());
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
}
