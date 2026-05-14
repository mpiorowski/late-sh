use anyhow::{Context, Result};
use reqwest::Url;
use serde::Deserialize;

const MIN_DURATION_MS: i32 = 30_000;

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

impl YoutubeClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            api_key,
        }
    }

    pub async fn validate_url(&self, url: &str) -> Result<YoutubeVideo> {
        let video_id = extract_video_id(url)?;
        self.validate_video_id(video_id).await
    }

    async fn validate_video_id(&self, video_id: String) -> Result<YoutubeVideo> {
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
            .context("failed to call YouTube Data API")?
            .error_for_status()
            .context("YouTube Data API rejected the validation request")?
            .json::<VideosListResponse>()
            .await
            .context("failed to decode YouTube Data API response")?;

        let item = response
            .items
            .into_iter()
            .next()
            .context("YouTube video was not found")?;

        if item.status.privacy_status.as_deref() != Some("public") {
            anyhow::bail!("YouTube video is not public");
        }
        if item.status.embeddable != Some(true) {
            anyhow::bail!("YouTube video is not embeddable");
        }

        let live_state = item
            .snippet
            .live_broadcast_content
            .as_deref()
            .unwrap_or("none");
        let is_stream = live_state == "live";
        if live_state == "upcoming" {
            anyhow::bail!("YouTube video is an upcoming stream");
        }

        let duration_ms = item
            .content_details
            .duration
            .as_deref()
            .and_then(parse_youtube_duration_ms);

        if !is_stream {
            let duration_ms = duration_ms.context("YouTube video duration is unavailable")?;
            if duration_ms < MIN_DURATION_MS {
                anyhow::bail!("YouTube video must be at least 30 seconds long");
            }
        }

        Ok(YoutubeVideo {
            video_id,
            title: item.snippet.title.filter(|title| !title.trim().is_empty()),
            channel: item
                .snippet
                .channel_title
                .filter(|channel| !channel.trim().is_empty()),
            duration_ms,
            is_stream,
        })
    }
}

pub fn trusted_video_from_url(url: &str) -> Result<YoutubeVideo> {
    Ok(YoutubeVideo {
        video_id: extract_video_id(url)?,
        title: None,
        channel: None,
        duration_ms: None,
        is_stream: false,
    })
}

pub fn extract_video_id(raw: &str) -> Result<String> {
    let url = Url::parse(raw.trim()).context("invalid URL")?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    let id = if host == "youtu.be" {
        url.path_segments()
            .and_then(|mut segments| segments.next())
            .map(str::to_string)
    } else if host == "youtube.com" || host.ends_with(".youtube.com") {
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
        anyhow::bail!("unsupported YouTube URL");
    };
    let id = id.trim();
    if id.len() != 11
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        anyhow::bail!("invalid YouTube video id");
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
