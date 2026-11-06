use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

use super::thumbnail::{THUMBNAIL_MAX_BYTES, shrink, thumbnail_url};

const MIN_DURATION_MS: i32 = 30_000;
const THUMBNAIL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const OEMBED_ENDPOINT: &str = "https://www.youtube.com/oembed";
const OEMBED_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// An oEmbed answer is under a kilobyte; anything near this is not one.
const OEMBED_MAX_BYTES: usize = 16 * 1024;

#[derive(Clone)]
pub struct YoutubeClient {
    http: reqwest::Client,
    api_key: Option<String>,
}

#[derive(Debug, Clone)]
pub struct YoutubeVideo {
    pub video_id: String,
    pub title: Option<String>,
    pub channel: Option<String>,
    pub duration_ms: Option<i32>,
    pub is_stream: bool,
}

/// Why a link is turned down: the link itself, or what YouTube says about
/// the video behind it. The submitter's problem to see; `message` is the
/// banner. Anything else that stops a lookup (no API key, the Data API
/// down) is an error, and ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeRefusal {
    UnsupportedUrl,
    InvalidVideoId,
    NotFound,
    NotPublic,
    NotEmbeddable,
    UpcomingStream,
    DurationUnavailable,
    TooShort,
}

impl YoutubeRefusal {
    pub fn message(self) -> &'static str {
        match self {
            YoutubeRefusal::UnsupportedUrl
            | YoutubeRefusal::InvalidVideoId
            | YoutubeRefusal::NotFound => "Invalid YouTube URL",
            YoutubeRefusal::NotPublic => "Video is not public",
            YoutubeRefusal::NotEmbeddable => "Video is not embeddable",
            YoutubeRefusal::UpcomingStream => "Upcoming streams are not supported",
            YoutubeRefusal::DurationUnavailable => "Video duration is unavailable",
            YoutubeRefusal::TooShort => "Video must be at least 30 seconds",
        }
    }
}

/// What `validate_url` found behind a link.
#[derive(Debug, Clone)]
pub enum YoutubeLookup {
    Video(YoutubeVideo),
    Refused(YoutubeRefusal),
}

impl YoutubeClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
        }
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key
            .as_deref()
            .map(|key| !key.trim().is_empty())
            .unwrap_or(false)
    }

    /// The video's thumbnail, shrunk for the live strip
    /// (`thumbnail::shrink`). Needs no API key: the image host is public.
    pub async fn fetch_thumbnail(&self, video_id: &str) -> Result<image::RgbaImage> {
        let mut response = self
            .http
            .get(thumbnail_url(video_id))
            .timeout(THUMBNAIL_TIMEOUT)
            .send()
            .await
            .context("failed to call the YouTube thumbnail host")?
            .error_for_status()
            .context("the YouTube thumbnail host refused the request")?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .context("failed to read the YouTube thumbnail")?
        {
            if bytes.len() + chunk.len() > THUMBNAIL_MAX_BYTES {
                anyhow::bail!("YouTube thumbnail is past {THUMBNAIL_MAX_BYTES} bytes");
            }
            bytes.extend_from_slice(&chunk);
        }
        shrink(&bytes)
    }

    /// The video behind a booth link, validated through the Data API, or
    /// why it is turned down.
    pub async fn validate_url(&self, url: &str) -> Result<YoutubeLookup> {
        let video_id = match extract_video_id(url) {
            Ok(video_id) => video_id,
            Err(refusal) => return Ok(YoutubeLookup::Refused(refusal)),
        };
        self.validate_video_id(video_id).await
    }

    async fn validate_video_id(&self, video_id: String) -> Result<YoutubeLookup> {
        let api_key = self
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
            .context("LATE_YOUTUBE_API_KEY is not configured")?;

        let mut api_url = Url::parse("https://www.googleapis.com/youtube/v3/videos")
            .context("invalid YouTube Data API URL")?;
        api_url
            .query_pairs_mut()
            .append_pair("part", "snippet,contentDetails,status")
            .append_pair("id", video_id.as_str())
            .append_pair("key", api_key);

        let response = self
            .http
            .get(api_url)
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .context("failed to call YouTube Data API")?
            .error_for_status()
            .map_err(reqwest::Error::without_url)
            .context("YouTube Data API rejected the validation request")?
            .json::<VideosListResponse>()
            .await
            .map_err(reqwest::Error::without_url)
            .context("failed to decode YouTube Data API response")?;

        let Some(item) = response.items.into_iter().next() else {
            return Ok(YoutubeLookup::Refused(YoutubeRefusal::NotFound));
        };

        if item.status.privacy_status.as_deref() != Some("public") {
            return Ok(YoutubeLookup::Refused(YoutubeRefusal::NotPublic));
        }
        if item.status.embeddable != Some(true) {
            return Ok(YoutubeLookup::Refused(YoutubeRefusal::NotEmbeddable));
        }

        let live_state = item
            .snippet
            .live_broadcast_content
            .as_deref()
            .unwrap_or("none");
        let is_stream = live_state == "live";
        if live_state == "upcoming" {
            return Ok(YoutubeLookup::Refused(YoutubeRefusal::UpcomingStream));
        }

        let duration_ms = item
            .content_details
            .duration
            .as_deref()
            .and_then(parse_youtube_duration_ms);

        if !is_stream {
            let Some(duration_ms) = duration_ms else {
                return Ok(YoutubeLookup::Refused(YoutubeRefusal::DurationUnavailable));
            };
            if duration_ms < MIN_DURATION_MS {
                return Ok(YoutubeLookup::Refused(YoutubeRefusal::TooShort));
            }
        }

        Ok(YoutubeLookup::Video(YoutubeVideo {
            video_id,
            title: item.snippet.title.filter(|title| !title.trim().is_empty()),
            channel: item
                .snippet
                .channel_title
                .filter(|channel| !channel.trim().is_empty()),
            duration_ms,
            is_stream,
        }))
    }
}

/// Canonical watch link for a video id, the same form the CLI's MPRIS
/// metadata publishes.
pub fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

/// YouTube's public oEmbed endpoint for a video id: its title, no API key.
fn oembed_url(video_id: &str) -> String {
    let mut url = Url::parse(OEMBED_ENDPOINT).expect("the oEmbed endpoint is a valid URL");
    url.query_pairs_mut()
        .append_pair("url", &watch_url(video_id))
        .append_pair("format", "json");
    url.to_string()
}

/// The video's title from YouTube's oEmbed endpoint, fetched through the
/// shared public-only downloader. Errs on a private, deleted or
/// unembeddable video (oEmbed answers 401 or 404) and on an empty title.
pub async fn fetch_oembed_title(video_id: &str) -> Result<String> {
    let bytes = crate::app::files::image_upload::download_url_bytes(
        &oembed_url(video_id),
        OEMBED_TIMEOUT,
        OEMBED_MAX_BYTES,
    )
    .await
    .context("fetching youtube oembed")?;
    oembed_title(&bytes)
}

/// The trimmed title out of an oEmbed JSON body.
fn oembed_title(body: &[u8]) -> Result<String> {
    let payload: OEmbedResponse =
        serde_json::from_slice(body).context("decoding youtube oembed")?;
    let title = payload.title.trim();
    if title.is_empty() {
        anyhow::bail!("youtube oembed title is empty");
    }
    Ok(title.to_string())
}

/// The video id in a YouTube watch, short, embed (no-cookie included),
/// shorts or live link: the shapes the booth takes. A channel, a playlist
/// or another site has none.
pub fn video_id(raw: &str) -> Option<String> {
    extract_video_id(raw).ok()
}

fn extract_video_id(raw: &str) -> Result<String, YoutubeRefusal> {
    let Ok(url) = Url::parse(raw.trim()) else {
        return Err(YoutubeRefusal::UnsupportedUrl);
    };
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let id = if host == "youtu.be" {
        url.path_segments()
            .and_then(|mut segments| segments.next())
            .map(str::to_string)
    } else if host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtube-nocookie.com"
        || host.ends_with(".youtube-nocookie.com")
    {
        if url.path() == "/watch" {
            url.query_pairs()
                .find(|(key, _)| key == "v")
                .map(|(_, value)| value.into_owned())
        } else {
            let mut segments = url.path_segments().into_iter().flatten();
            match segments.next() {
                Some("embed" | "shorts" | "live") => segments.next().map(str::to_string),
                _ => None,
            }
        }
    } else {
        None
    };

    let Some(id) = id else {
        return Err(YoutubeRefusal::UnsupportedUrl);
    };
    let id = id.trim();
    if id.len() != 11
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(YoutubeRefusal::InvalidVideoId);
    }
    Ok(id.to_string())
}

fn parse_youtube_duration_ms(duration: &str) -> Option<i32> {
    let bytes = duration.as_bytes();
    if !duration.starts_with('P') {
        return None;
    }

    let mut in_time = false;
    let mut value: i64 = 0;
    let mut total_ms: i64 = 0;
    let mut saw_component = false;

    for &byte in &bytes[1..] {
        match byte {
            b'0'..=b'9' => {
                value = value
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as i64);
            }
            b'T' => {
                if in_time || value != 0 {
                    return None;
                }
                in_time = true;
            }
            b'H' if in_time => {
                total_ms = total_ms.saturating_add(value.saturating_mul(3_600_000));
                value = 0;
                saw_component = true;
            }
            b'M' if in_time => {
                total_ms = total_ms.saturating_add(value.saturating_mul(60_000));
                value = 0;
                saw_component = true;
            }
            b'S' if in_time => {
                total_ms = total_ms.saturating_add(value.saturating_mul(1_000));
                value = 0;
                saw_component = true;
            }
            _ => return None,
        }
    }

    if value != 0 || !saw_component || total_ms > i32::MAX as i64 {
        return None;
    }
    Some(total_ms as i32)
}

#[derive(Debug, Deserialize)]
struct VideosListResponse {
    items: Vec<VideoItem>,
}

#[derive(Debug, Deserialize)]
struct VideoItem {
    snippet: VideoSnippet,
    #[serde(rename = "contentDetails")]
    content_details: VideoContentDetails,
    status: VideoStatus,
}

#[derive(Debug, Deserialize)]
struct VideoSnippet {
    title: Option<String>,
    #[serde(rename = "channelTitle")]
    channel_title: Option<String>,
    #[serde(rename = "liveBroadcastContent")]
    live_broadcast_content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VideoContentDetails {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VideoStatus {
    embeddable: Option<bool>,
    #[serde(rename = "privacyStatus")]
    privacy_status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OEmbedResponse {
    title: String,
}

#[cfg(test)]
#[path = "youtube_test.rs"]
mod youtube_test;
