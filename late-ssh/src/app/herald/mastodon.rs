//! The Mastodon leg of a delivery: the media upload and the status call
//! for a picture, the status call alone for a note behind a warning
//! (`state::mastodon_post`), against any server speaking the Mastodon
//! API. Returns the status's URL or an error; `svc.rs` logs and counts.

use std::time::Duration;

use anyhow::{Context, bail};
use late_core::telemetry::TracedExt;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::MastodonConfig;

/// An upload the instance answered 202 to is polled this often, this
/// many times, before the delivery fails. Images are processed in
/// seconds.
const MEDIA_POLL_INTERVAL: Duration = Duration::from_secs(2);
const MEDIA_POLL_TRIES: u32 = 15;

/// The file name the upload carries; the instance keeps its own.
const UPLOAD_FILE_NAME: &str = "late-sh.png";

/// What goes out.
pub(crate) struct Post<'a> {
    /// The day's key: the `Idempotency-Key` on the status call, so a retry
    /// after a lost answer gets the status the instance already made.
    pub(crate) key: Uuid,
    pub(crate) text: &'a str,
    pub(crate) body: Body<'a>,
}

/// A clean picture, or a note in its place.
pub(crate) enum Body<'a> {
    Picture {
        png: &'a [u8],
        alt: &'a str,
    },
    /// No media: the status sensitive, `warning` its content warning.
    Note {
        warning: &'a str,
    },
}

/// `POST /api/v1/statuses` as the instance reads it.
#[derive(Serialize)]
struct StatusBody<'a> {
    status: &'a str,
    media_ids: Vec<&'a str>,
    visibility: &'a str,
    language: &'a str,
    sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    spoiler_text: Option<&'a str>,
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

/// Upload the picture, then post the status carrying it; a note is the
/// status alone. The status's public URL comes back.
pub(crate) async fn deliver(
    http: &reqwest::Client,
    config: &MastodonConfig,
    post: Post<'_>,
) -> anyhow::Result<String> {
    match post.body {
        Body::Picture { png, alt } => {
            let media_id = upload_media(http, config, png.to_vec(), alt).await?;
            post_status(http, config, post.key, post.text, Carries::Media(&media_id)).await
        }
        Body::Note { warning } => {
            post_status(http, config, post.key, post.text, Carries::Warning(warning)).await
        }
    }
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
    match status.as_u16() {
        200 => Ok(parse_media(response).await?.id),
        202 => {
            let id = parse_media(response).await?.id;
            await_media(http, config, &id).await?;
            Ok(id)
        }
        _ => bail!("{url} answered {status}"),
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
            _ => bail!("{url} answered {status}"),
        }
    }
    bail!("media {id} was still processing after {MEDIA_POLL_TRIES} polls")
}

/// What a status carries besides its words.
enum Carries<'a> {
    /// The uploaded picture's id.
    Media(&'a str),
    /// A note's content warning.
    Warning(&'a str),
}

/// `POST /api/v1/statuses` with the day's key as `Idempotency-Key`: the
/// picture's media, or the note's warning as the status's content
/// warning, the status marked sensitive.
async fn post_status(
    http: &reqwest::Client,
    config: &MastodonConfig,
    key: Uuid,
    text: &str,
    carries: Carries<'_>,
) -> anyhow::Result<String> {
    let url = format!("{}/api/v1/statuses", config.instance);
    let (media_ids, spoiler_text) = match carries {
        Carries::Media(id) => (vec![id], None),
        Carries::Warning(warning) => (Vec::new(), Some(warning)),
    };
    let body = StatusBody {
        status: text,
        media_ids,
        visibility: config.visibility.as_str(),
        language: "en",
        sensitive: spoiler_text.is_some(),
        spoiler_text,
    };
    let posted: StatusResponse = http
        .post(&url)
        .bearer_auth(&config.token)
        .header("Idempotency-Key", key.to_string())
        .json(&body)
        .send_traced()
        .await
        .with_context(|| format!("posting {url}"))?
        .json()
        .await
        .context("decoding the status answer")?;
    match posted.url {
        Some(url) => Ok(url),
        None => bail!("status {} came back without a url", posted.id),
    }
}

async fn parse_media(response: reqwest::Response) -> anyhow::Result<MediaResponse> {
    response.json().await.context("decoding the media answer")
}
