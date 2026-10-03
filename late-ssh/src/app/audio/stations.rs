//! Stream resolution and now-playing lookup for the radio catalogue
//! (`late_core::radio`). The catalogue itself lives in late-core so the
//! settings parser can validate station keys; this module is where the
//! server turns a station into a URL a client can open and a track line
//! the rail can show.

use std::collections::HashMap;

use late_core::models::user::{AudioSource, RadioStation};
use late_core::radio::Provider;

use super::radio_meta::svc::ArtistTitle;
use late_core::api_types::NowPlaying;

pub struct StreamSelection {
    pub url: String,
    pub station: &'static str,
}

/// What a paired client should open for `source`. `house_base_url` is the
/// Icecast base house stations resolve against; third-party stations carry
/// their own absolute URL and are never proxied through late.sh.
pub fn resolve_stream_selection(
    house_base_url: &str,
    source: AudioSource,
    station: RadioStation,
) -> Option<StreamSelection> {
    match source {
        AudioSource::Radio => Some(StreamSelection {
            url: station.stream_url(house_base_url),
            station: station.as_str(),
        }),
        AudioSource::Youtube => None,
    }
}

/// Public stream URL for a station key, or `None` for a key late.sh does
/// not offer. The Nightride `/meta` feed carries more stations than the
/// catalogue (darksynth, horrorsynth, ebsm), so this must stay strict:
/// `RadioStation::from_settings_str` defaults unknown input to Chillsynth,
/// which here would hand out the wrong stream.
pub fn station_stream_url_by_key(public_house_base_url: &str, key: &str) -> Option<String> {
    RadioStation::from_key(key).map(|station| station.stream_url(public_house_base_url))
}

/// `Artist - Title` for `station` from whichever feed its provider has:
/// the Nightride SSE map or the house Icecast now-playing map. `None`
/// while that feed has nothing for it, so the caller shows the label.
pub fn station_now_playing(
    station: RadioStation,
    radio_meta: &HashMap<String, ArtistTitle>,
    house: &HashMap<String, NowPlaying>,
) -> Option<String> {
    match station.provider() {
        Provider::House => house.get(station.as_str()).map(house_track_text),
        Provider::Nightride | Provider::Plaza | Provider::CodeRadio => radio_meta
            .get(station.as_str())
            .map(|meta| format!("{} - {}", meta.artist, meta.title)),
    }
}

/// Combined `Artist - Title` row for a house now-playing track; bare title
/// when the tag has no artist.
pub fn house_track_text(now: &NowPlaying) -> String {
    match now.track.artist.as_deref() {
        Some(artist) if !artist.trim().is_empty() => {
            format!("{} - {}", artist.trim(), now.track.title)
        }
        _ => now.track.title.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_stream_url_by_key_is_strict_about_unknown_stations() {
        assert_eq!(
            station_stream_url_by_key("https://late.sh/stream", "chillsynth").as_deref(),
            Some("https://stream.nightride.fm/chillsynth.mp3")
        );
        // The `ambient` label persists and keys as `rektify`.
        assert_eq!(
            station_stream_url_by_key("https://late.sh/stream", "rektify").as_deref(),
            Some("https://stream.nightride.fm/rektify.mp3")
        );
        // House mounts resolve against the public base.
        assert_eq!(
            station_stream_url_by_key("https://late.sh/stream", "classical").as_deref(),
            Some("https://late.sh/stream/classical")
        );
        // Stations the /meta feed carries but late.sh does not offer must
        // drop out, not silently resolve to the Chillsynth default.
        assert_eq!(
            station_stream_url_by_key("https://late.sh/stream", "darksynth"),
            None
        );
        assert_eq!(
            station_stream_url_by_key("https://late.sh/stream", "ambient"),
            None
        );
        assert_eq!(
            station_stream_url_by_key("https://late.sh/stream", ""),
            None
        );
    }

    #[test]
    fn station_now_playing_reads_the_provider_feed() {
        let mut radio_meta = HashMap::new();
        radio_meta.insert(
            "datawave".to_string(),
            ArtistTitle {
                artist: "Com Truise".to_string(),
                title: "Flightwave".to_string(),
            },
        );
        let mut house = HashMap::new();
        house.insert(
            "classical".to_string(),
            NowPlaying::new(late_core::api_types::Track {
                artist: Some("Kimiko Ishizaka".to_string()),
                title: "Prelude No. 1".to_string(),
                duration_seconds: None,
            }),
        );
        let datawave = RadioStation::from_key("datawave").unwrap();
        let classical = RadioStation::from_key("classical").unwrap();
        let chill = RadioStation::from_key("chill").unwrap();
        assert_eq!(
            station_now_playing(datawave, &radio_meta, &house).as_deref(),
            Some("Com Truise - Flightwave")
        );
        assert_eq!(
            station_now_playing(classical, &radio_meta, &house).as_deref(),
            Some("Kimiko Ishizaka - Prelude No. 1")
        );
        // A house station never reads the Nightride map, and vice versa.
        assert_eq!(station_now_playing(chill, &radio_meta, &house), None);
    }
}
