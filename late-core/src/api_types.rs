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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_json_roundtrip() {
        let track = Track {
            title: "Lo-Fi Beats".to_string(),
            artist: Some("Classic Artist".to_string()),
            duration_seconds: Some(180),
        };

        let json = serde_json::to_string(&track).unwrap();
        let parsed: Track = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.title, track.title);
        assert_eq!(parsed.artist, track.artist);
        assert_eq!(parsed.duration_seconds, track.duration_seconds);
    }

    #[test]
    fn track_with_none_fields() {
        let track = Track {
            title: "Unknown".to_string(),
            artist: None,
            duration_seconds: None,
        };

        let json = serde_json::to_string(&track).unwrap();
        let parsed: Track = serde_json::from_str(&json).unwrap();

        assert!(parsed.artist.is_none());
        assert!(parsed.duration_seconds.is_none());
    }

    #[test]
    fn now_playing_response_roundtrip() {
        let response = NowPlayingResponse {
            current_track: Track {
                title: "Test Track".to_string(),
                artist: None,
                duration_seconds: None,
            },
            listeners_count: 42,
            started_at_ts: 1700000000,
        };

        let json = serde_json::to_string(&response).unwrap();
        let parsed: NowPlayingResponse = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.listeners_count, 42);
        assert_eq!(parsed.started_at_ts, 1700000000);
    }

    #[test]
    fn status_response_roundtrip() {
        let status = StatusResponse {
            online: true,
            message: "All systems go".to_string(),
            version: "1.0.0".to_string(),
        };

        let json = serde_json::to_string(&status).unwrap();
        let parsed: StatusResponse = serde_json::from_str(&json).unwrap();

        assert!(parsed.online);
        assert_eq!(parsed.message, "All systems go");
        assert_eq!(parsed.version, "1.0.0");
    }

    #[test]
    fn track_display_with_artist() {
        let track = Track {
            title: "My Song".to_string(),
            artist: Some("My Artist".to_string()),
            duration_seconds: None,
        };
        assert_eq!(track.to_string(), "My Artist - My Song");
    }

    #[test]
    fn track_display_without_artist() {
        let track = Track {
            title: "Solo Title".to_string(),
            artist: None,
            duration_seconds: None,
        };
        assert_eq!(track.to_string(), "Solo Title");
    }

    #[test]
    fn track_default_is_empty() {
        let track = Track::default();
        assert_eq!(track.title, "");
        assert!(track.artist.is_none());
        assert!(track.duration_seconds.is_none());
    }

    #[test]
    fn now_playing_remaining_without_duration() {
        let np = NowPlaying::new(Track::default());
        assert!(np.remaining_seconds().is_none());
    }

    #[test]
    fn now_playing_remaining_with_duration() {
        let track = Track {
            title: "Test".to_string(),
            artist: None,
            duration_seconds: Some(300),
        };
        let np = NowPlaying::new(track);
        // Just created, remaining should be close to full duration
        let remaining = np.remaining_seconds().unwrap();
        assert!(remaining <= 300);
        assert!(remaining >= 298);
    }
}
