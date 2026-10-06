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
#[path = "icecast_test.rs"]
mod icecast_test;
