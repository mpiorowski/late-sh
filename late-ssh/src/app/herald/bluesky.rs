//! The Bluesky leg of a delivery: the AT Protocol calls (a session from
//! the app password, the blob upload, the post record), the picture
//! fitted under the blob cap, the record key derived from the day's id so
//! a retry overwrites instead of duplicating, and the facets that make the
//! link and the tags live. Returns the post's URL or an error; `svc.rs`
//! logs and counts.

use std::io::Cursor;

use anyhow::{Context, bail};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, ImageReader};
use late_core::telemetry::TracedExt;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::BlueskyConfig;

/// The PDS refuses a blob over a megabyte; stay under with room for the
/// container's own bytes.
pub(crate) const BLOB_MAX_BYTES: usize = 950_000;

/// Each pass shrinks the picture's sides by this much until it fits.
const SHRINK_STEP: f64 = 0.8;

/// A picture narrower than this is not worth posting at all.
const MIN_WIDTH: u32 = 320;

/// The record key alphabet: base32-sortable, as the AT Protocol's TIDs
/// spell it.
const TID_ALPHABET: &[u8; 32] = b"234567abcdefghijklmnopqrstuvwxyz";

/// How much of an error body an error message keeps.
const BODY_EXCERPT: usize = 200;

/// What goes out with the picture.
pub(crate) struct Post<'a> {
    /// The day's key: the record key is derived from it, so a retry puts
    /// the same record again instead of a second post.
    pub(crate) key: Uuid,
    pub(crate) png: &'a [u8],
    pub(crate) text: &'a str,
    pub(crate) alt: &'a str,
    /// When the post says it was made.
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct Session {
    #[serde(rename = "accessJwt")]
    access_jwt: String,
    did: String,
    handle: String,
}

#[derive(Deserialize)]
struct Uploaded {
    blob: Value,
}

#[derive(Deserialize)]
struct PutRecord {
    uri: String,
}

/// The picture as it goes up: under the cap, with its size for the
/// embed's aspect ratio.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fitted {
    pub(crate) png: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Session, blob, record. The post's URL on bsky.app comes back.
pub(crate) async fn deliver(
    http: &reqwest::Client,
    config: &BlueskyConfig,
    post: Post<'_>,
) -> anyhow::Result<String> {
    let session = create_session(http, config).await?;
    let fitted = fit_under(post.png, BLOB_MAX_BYTES)?;
    let blob = upload_blob(http, config, &session, fitted.png).await?;
    let rkey = tid_for(post.key);
    let record = json!({
        "$type": "app.bsky.feed.post",
        "text": post.text,
        "createdAt": post.created_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "langs": ["en"],
        "facets": facets(post.text),
        "embed": {
            "$type": "app.bsky.embed.images",
            "images": [{
                "alt": post.alt,
                "image": blob,
                "aspectRatio": { "width": fitted.width, "height": fitted.height },
            }],
        },
    });
    put_record(http, config, &session, &rkey, record).await?;
    Ok(format!(
        "https://bsky.app/profile/{}/post/{rkey}",
        session.handle
    ))
}

/// `com.atproto.server.createSession` with the handle and the app
/// password.
async fn create_session(http: &reqwest::Client, config: &BlueskyConfig) -> anyhow::Result<Session> {
    let url = format!("{}/xrpc/com.atproto.server.createSession", config.pds);
    let response = http
        .post(&url)
        .json(&json!({
            "identifier": config.identifier,
            "password": config.app_password,
        }))
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
    serde_json::from_str(&body).context("decoding the session answer")
}

/// `com.atproto.repo.uploadBlob`: the PNG bytes, the blob reference back.
async fn upload_blob(
    http: &reqwest::Client,
    config: &BlueskyConfig,
    session: &Session,
    png: Vec<u8>,
) -> anyhow::Result<Value> {
    let url = format!("{}/xrpc/com.atproto.repo.uploadBlob", config.pds);
    let response = http
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .header("Content-Type", "image/png")
        .body(png)
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
    let uploaded: Uploaded = serde_json::from_str(&body).context("decoding the blob answer")?;
    Ok(uploaded.blob)
}

/// `com.atproto.repo.putRecord` under the day's key: the same call twice
/// is one post.
async fn put_record(
    http: &reqwest::Client,
    config: &BlueskyConfig,
    session: &Session,
    rkey: &str,
    record: Value,
) -> anyhow::Result<String> {
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", config.pds);
    let response = http
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&json!({
            "repo": session.did,
            "collection": "app.bsky.feed.post",
            "rkey": rkey,
            "record": record,
        }))
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
    let put: PutRecord = serde_json::from_str(&body).context("decoding the record answer")?;
    Ok(put.uri)
}

/// The day's record key: a TID (the AT Protocol's timestamp identifier,
/// 13 characters of base32-sortable) built from the UUID v7's millisecond
/// timestamp and ten bits of its randomness. Deterministic per key, so a
/// retried day puts the same record.
pub(crate) fn tid_for(key: Uuid) -> String {
    let bytes = key.as_bytes();
    let millis = u64::from_be_bytes([
        0, 0, bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5],
    ]);
    let clock = u64::from(u16::from_be_bytes([bytes[6], bytes[7]]) & 0x3ff);
    let value = (millis * 1000) << 10 | clock;
    let mut out = String::with_capacity(13);
    for index in 0..13 {
        let shift = 5 * (12 - index);
        let digit = ((value >> shift) & 31) as usize;
        out.push(TID_ALPHABET[digit] as char);
    }
    out
}

/// The picture under `max_bytes`: as it came when it already is, else
/// shrunk by `SHRINK_STEP` a side per pass and re-encoded until it fits.
pub(crate) fn fit_under(png: &[u8], max_bytes: usize) -> anyhow::Result<Fitted> {
    let decoded = ImageReader::with_format(Cursor::new(png), ImageFormat::Png)
        .decode()
        .context("decoding the picture")?;
    if png.len() <= max_bytes {
        return Ok(Fitted {
            png: png.to_vec(),
            width: decoded.width(),
            height: decoded.height(),
        });
    }
    let mut image = decoded;
    loop {
        let width = (f64::from(image.width()) * SHRINK_STEP).round() as u32;
        let height = (f64::from(image.height()) * SHRINK_STEP).round() as u32;
        if width < MIN_WIDTH {
            bail!("the picture does not fit under {max_bytes} bytes above {MIN_WIDTH} pixels wide");
        }
        image = DynamicImage::ImageRgba8(
            image
                .resize_exact(width, height, FilterType::Triangle)
                .to_rgba8(),
        );
        let mut encoded = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png)
            .context("encoding the shrunk picture")?;
        if encoded.len() <= max_bytes {
            return Ok(Fitted {
                png: encoded,
                width,
                height,
            });
        }
    }
}

/// The rich-text facets: every `http(s)://` word a link, every `#word` a
/// tag, by UTF-8 byte range as the protocol counts them. Our own text, so
/// a word is whatever sits between whitespace.
pub(crate) fn facets(text: &str) -> Vec<Value> {
    let mut facets = Vec::new();
    let mut start = 0;
    for (index, ch) in text
        .char_indices()
        .chain(std::iter::once((text.len(), ' ')))
    {
        if !ch.is_whitespace() {
            continue;
        }
        let word = &text[start..index];
        if word.starts_with("https://") || word.starts_with("http://") {
            facets.push(json!({
                "index": { "byteStart": start, "byteEnd": index },
                "features": [{ "$type": "app.bsky.richtext.facet#link", "uri": word }],
            }));
        } else if let Some(tag) = word.strip_prefix('#') {
            if !tag.is_empty() {
                facets.push(json!({
                    "index": { "byteStart": start, "byteEnd": index },
                    "features": [{ "$type": "app.bsky.richtext.facet#tag", "tag": tag }],
                }));
            }
        }
        start = index + ch.len_utf8();
    }
    facets
}

fn excerpt(body: &str) -> String {
    body.chars().take(BODY_EXCERPT).collect()
}
