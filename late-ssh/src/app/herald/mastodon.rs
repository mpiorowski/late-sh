//! The Mastodon leg of a delivery: the media upload and the status call,
//! against any server speaking the Mastodon API. Returns the status's URL
//! or an error; `svc.rs` logs and counts.

use std::time::Duration;

use anyhow::{Context, bail};
use late_core::telemetry::TracedExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::{HeraldVisibility, MastodonConfig};

/// An upload the instance answered 202 to is polled this often, this
/// many times, before the delivery fails. Images are processed in
/// seconds.
const MEDIA_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MEDIA_POLL_TRIES: u32 = 15;

/// The file name the upload carries; the instance keeps its own.
const UPLOAD_FILE_NAME: &str = "late-sh.png";

/// How much of an error body an error message keeps.
const BODY_EXCERPT: usize = 200;

/// What goes out with the picture.
pub(crate) struct Post<'a> {
    /// The day's key: the `Idempotency-Key` on the status call, so a retry
    /// after a lost answer gets the status the instance already made.
    pub(crate) key: Uuid,
    pub(crate) png: &'a [u8],
    pub(crate) text: &'a str,
    pub(crate) alt: &'a str,
    pub(crate) visibility: HeraldVisibility,
}

/// `POST /api/v1/statuses` as the instance reads it.
#[derive(Serialize)]
struct StatusBody<'a> {
    status: &'a str,
    media_ids: Vec<&'a str>,
    visibility: &'a str,
    language: &'a str,
}

#[derive(Deserialize)]
struct MediaResponse {
    id: String,
}

#[derive(Deserialize)]
struct StatusResponse {
    id: String,
    url: Option<String>,
}

/// Upload the picture, then post the status carrying it. The status's
/// public URL comes back.
pub(crate) async fn deliver(
    http: &reqwest::Client,
    config: &MastodonConfig,
    post: Post<'_>,
) -> anyhow::Result<String> {
    let media_id = upload_media(http, config, post.png.to_vec(), post.alt).await?;
    post_status(http, config, &post, &media_id).await
}

/// `POST /api/v2/media`: the picture with its alt text. The instance
/// answers 200 with the attachment ready, or 202 while it processes one,
/// polled until it is.
async fn upload_media(
    http: &reqwest::Client,
    config: &MastodonConfig,
    png: Vec<u8>,
    alt: &str,
) -> anyhow::Result<String> {
    let part = reqwest::multipart::Part::bytes(png)
        .file_name(UPLOAD_FILE_NAME)
        .mime_str("image/png")
        .context("building the png part")?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("description", alt.to_string());
    let url = format!("{}/api/v2/media", config.instance);
    let response = http
        .post(&url)
        .bearer_auth(&config.token)
        .multipart(form)
        .send_traced()
        .await
        .with_context(|| format!("posting {url}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("reading {url}"))?;
    match status.as_u16() {
        200 => Ok(parse_media(&body)?.id),
        202 => {
            let id = parse_media(&body)?.id;
            await_media(http, config, &id).await?;
            Ok(id)
        }
        _ => bail!("{url} answered {status}: {}", excerpt(&body)),
    }
}

/// `GET /api/v1/media/{id}` until the instance answers 200 (206 while it
/// is still processing).
async fn await_media(
    http: &reqwest::Client,
    config: &MastodonConfig,
    id: &str,
) -> anyhow::Result<()> {
    let url = format!("{}/api/v1/media/{id}", config.instance);
    for _ in 0..MEDIA_POLL_TRIES {
        tokio::time::sleep(MEDIA_POLL_INTERVAL).await;
        let response = http
            .get(&url)
            .bearer_auth(&config.token)
            .send_traced()
            .await
            .with_context(|| format!("getting {url}"))?;
        let status = response.status();
        match status.as_u16() {
            200 => return Ok(()),
            206 => continue,
            _ => {
                let body = response.text().await.unwrap_or_default();
                bail!("{url} answered {status}: {}", excerpt(&body));
            }
        }
    }
    bail!("media {id} was still processing after {MEDIA_POLL_TRIES} polls")
}

/// `POST /api/v1/statuses` with the day's key as `Idempotency-Key`.
async fn post_status(
    http: &reqwest::Client,
    config: &MastodonConfig,
    post: &Post<'_>,
    media_id: &str,
) -> anyhow::Result<String> {
    let url = format!("{}/api/v1/statuses", config.instance);
    let body = StatusBody {
        status: post.text,
        media_ids: vec![media_id],
        visibility: post.visibility.as_str(),
        language: "en",
    };
    let response = http
        .post(&url)
        .bearer_auth(&config.token)
        .header("Idempotency-Key", post.key.to_string())
        .json(&body)
        .send_traced()
        .await
        .with_context(|| format!("posting {url}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .with_context(|| format!("reading {url}"))?;
    if !status.is_success() {
        bail!("{url} answered {status}: {}", excerpt(&body));
    }
    let posted: StatusResponse =
        serde_json::from_str(&body).context("decoding the status answer")?;
    match posted.url {
        Some(url) => Ok(url),
        None => bail!("status {} came back without a url", posted.id),
    }
}

fn parse_media(body: &str) -> anyhow::Result<MediaResponse> {
    serde_json::from_str(body).context("decoding the media answer")
}

fn excerpt(body: &str) -> String {
    body.chars().take(BODY_EXCERPT).collect()
}
