use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct Track {
    pub title: String,
    pub artist: Option<String>,
    pub duration_seconds: Option<u64>,
}

impl fmt::Display for Track {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.artist {
            Some(artist) => write!(f, "{} - {}", artist, self.title),
            None => write!(f, "{}", self.title),
        }
    }
}

/// Now playing info with track start time for remaining calculation
#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub track: Track,
    pub started_at: std::time::Instant,
}

impl NowPlaying {
    pub fn new(track: Track) -> Self {
        Self {
            track,
            started_at: std::time::Instant::now(),
        }
    }

    /// Calculate remaining seconds, or None if duration unknown
    pub fn remaining_seconds(&self) -> Option<u64> {
        let duration = self.track.duration_seconds?;
        let elapsed = self.started_at.elapsed().as_secs();
        Some(duration.saturating_sub(elapsed))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NowPlayingResponse {
    pub current_track: Track,
    pub listeners_count: usize,
    pub started_at_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub online: bool,
    pub message: String,
    pub version: String,
}
