//! Providers whose now-playing is one JSON document fetched on a timer
//! (Nightride pushes over SSE instead; see `svc.rs`). Each feed serves a
//! single catalogue station and knows how to read its own payload.

use anyhow::Context;

use super::svc::ArtistTitle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolledFeed {
    /// Nightwave Plaza's public status API.
    Plaza,
    /// freeCodeCamp Code Radio's AzuraCast now-playing API.
    CodeRadio,
    /// Radio Paradise's now-playing API, one variant per channel.
    ParadiseMain,
    ParadiseMellow,
    ParadiseRock,
    ParadiseGlobe,
    ParadiseBeyond,
    ParadiseKfat,
    /// Radio France's live metadata for the FIP Jazz webradio. This is the
    /// endpoint their own web player polls, not a documented API.
    FipJazz,
    /// Radio Swiss Jazz's current-track API (the one its site polls).
    SwissJazz,
    /// Radio Swiss Classic's current-track API. `artist` is the composer.
    SwissClassic,
}

#[derive(serde::Deserialize)]
struct Song {
    #[serde(default)]
    artist: String,
    #[serde(default)]
    title: String,
}

#[derive(serde::Deserialize)]
struct PlazaStatus {
    song: Song,
}

#[derive(serde::Deserialize)]
struct AzuraCastNowPlaying {
    now_playing: AzuraCastCurrent,
}

#[derive(serde::Deserialize)]
struct AzuraCastCurrent {
    song: Song,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RadioFranceLive {
    now: RadioFranceNow,
}

/// Between songs `now` describes the programme instead: the lines hold a
/// show name and blurb, and `songUuid` is null.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RadioFranceNow {
    #[serde(default)]
    first_line: String,
    #[serde(default)]
    second_line: String,
    song_uuid: Option<String>,
}

#[derive(serde::Deserialize)]
struct RadioSwissCurrent {
    channel: RadioSwissChannel,
}

#[derive(serde::Deserialize)]
struct RadioSwissChannel {
    playingnow: RadioSwissPlayingNow,
}

#[derive(serde::Deserialize)]
struct RadioSwissPlayingNow {
    current: RadioSwissEntry,
}

#[derive(serde::Deserialize)]
struct RadioSwissEntry {
    metadata: Song,
}

impl PolledFeed {
    pub const ALL: [Self; 11] = [
        Self::Plaza,
        Self::CodeRadio,
        Self::ParadiseMain,
        Self::ParadiseMellow,
        Self::ParadiseRock,
        Self::ParadiseGlobe,
        Self::ParadiseBeyond,
        Self::ParadiseKfat,
        Self::FipJazz,
        Self::SwissJazz,
        Self::SwissClassic,
    ];

    /// The catalogue key (`late_core::radio`) this feed's track is
    /// published under in the metadata map.
    pub fn station_key(self) -> &'static str {
        match self {
            Self::Plaza => "plaza",
            Self::CodeRadio => "coderadio",
            Self::ParadiseMain => "paradise",
            Self::ParadiseMellow => "mellow",
            Self::ParadiseRock => "rock",
            Self::ParadiseGlobe => "global",
            Self::ParadiseBeyond => "beyond",
            Self::ParadiseKfat => "kfat",
            Self::FipJazz => "fipjazz",
            Self::SwissJazz => "swissjazz",
            Self::SwissClassic => "swissclassic",
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Self::Plaza => "https://api.plaza.one/status",
            Self::CodeRadio => {
                "https://coderadio-admin-v2.freecodecamp.org/api/nowplaying/coderadio"
            }
            // Channel ids from `api.radioparadise.com/api/list_chan`.
            Self::ParadiseMain => "https://api.radioparadise.com/api/now_playing?chan=0",
            Self::ParadiseMellow => "https://api.radioparadise.com/api/now_playing?chan=1",
            Self::ParadiseRock => "https://api.radioparadise.com/api/now_playing?chan=2",
            Self::ParadiseGlobe => "https://api.radioparadise.com/api/now_playing?chan=3",
            Self::ParadiseBeyond => "https://api.radioparadise.com/api/now_playing?chan=5",
            Self::ParadiseKfat => "https://api.radioparadise.com/api/now_playing?chan=945",
            // 65 is FIP Jazz's station id.
            Self::FipJazz => "https://api.radiofrance.fr/livemeta/live/65/webrf_webradio_player",
            Self::SwissJazz => "https://api.radioswissjazz.ch/api/v1/rsj/en/current",
            Self::SwissClassic => "https://api.radioswissclassic.ch/api/v1/rsc/en/current",
        }
    }

    /// The current track out of one response body, or `None` while the
    /// provider reports that no track is on air (FIP between songs). A
    /// payload without a title is an error. Either way the caller shows the
    /// station label. The artist may be empty: some tracks are tagged with
    /// a title only.
    pub fn parse(self, body: &str) -> anyhow::Result<Option<ArtistTitle>> {
        let song = match self {
            Self::Plaza => {
                serde_json::from_str::<PlazaStatus>(body)
                    .context("parsing plaza status")?
                    .song
            }
            Self::CodeRadio => {
                serde_json::from_str::<AzuraCastNowPlaying>(body)
                    .context("parsing code radio now-playing")?
                    .now_playing
                    .song
            }
            Self::ParadiseMain
            | Self::ParadiseMellow
            | Self::ParadiseRock
            | Self::ParadiseGlobe
            | Self::ParadiseBeyond
            | Self::ParadiseKfat => {
                serde_json::from_str::<Song>(body).context("parsing radio paradise now-playing")?
            }
            Self::FipJazz => {
                let now = serde_json::from_str::<RadioFranceLive>(body)
                    .context("parsing fip live metadata")?
                    .now;
                match now.song_uuid {
                    Some(_) => Song {
                        artist: now.second_line,
                        title: now.first_line,
                    },
                    None => return Ok(None),
                }
            }
            Self::SwissJazz | Self::SwissClassic => {
                serde_json::from_str::<RadioSwissCurrent>(body)
                    .context("parsing radio swiss current track")?
                    .channel
                    .playingnow
                    .current
                    .metadata
            }
        };
        let artist = song.artist.trim();
        let title = song.title.trim();
        if title.is_empty() {
            anyhow::bail!("now-playing payload has no title");
        }
        Ok(Some(ArtistTitle {
            artist: artist.to_string(),
            title: title.to_string(),
        }))
    }
}

#[cfg(test)]
#[path = "polled_test.rs"]
mod polled_test;
