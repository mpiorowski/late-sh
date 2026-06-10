use std::{convert::Infallible, time::Duration};

use async_stream::stream;
use axum::{
    Router,
    body::Body,
    extract::{Path, State},
    http::{
        HeaderValue, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    response::Response,
    routing::get,
};
use bytes::Bytes;
use late_core::telemetry::TracedExt;

use crate::AppState;

const SILENCE_MP3: &[u8] = include_bytes!("../../assets/silence.mp3");
const SILENCE_CHUNK_BYTES: usize = 8 * 1024;
const RETRY_INTERVAL: Duration = Duration::from_millis(500);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/stream", get(stream_handler))
        .route("/stream/{mount}", get(stream_mount_handler))
}

async fn stream_handler(State(state): State<AppState>) -> Response {
    stream_response(state, "chill")
}

async fn stream_mount_handler(
    State(state): State<AppState>,
    Path(mount): Path<String>,
) -> Response {
    stream_response(state, normalize_mount(&mount))
}

fn stream_response(state: AppState, mount: &'static str) -> Response {
    let body = Body::from_stream(proxy_stream(state, mount));

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("audio/mpeg"));
    response.headers_mut().insert(
        CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    response
}

fn proxy_stream(
    state: AppState,
    mount: &'static str,
) -> impl futures_util::Stream<Item = Result<Bytes, Infallible>> + Send + 'static {
    stream! {
        let upstream_url = upstream_stream_url(&state.config.audio_base_url, mount);
        let mut silence_offset = 0usize;

        loop {
            match state.http_client.get(&upstream_url).send_traced().await {
                Ok(mut response) if response.status().is_success() => {
                    loop {
                        match response.chunk().await {
                            Ok(Some(bytes)) if !bytes.is_empty() => {
                                yield Ok(bytes);
                            }
                            Ok(Some(_)) => {}
                            Ok(None) => break,
                            Err(err) => {
                                tracing::warn!(error = ?err, url = %upstream_url, "upstream stream chunk failed");
                                break;
                            }
                        }
                    }
                    tracing::warn!(url = %upstream_url, "upstream stream ended; injecting silence until reconnect");
                }
                Ok(response) => {
                    tracing::warn!(
                        status = %response.status(),
                        url = %upstream_url,
                        "upstream stream request failed; injecting silence until reconnect"
                    );
                }
                Err(err) => {
                    tracing::warn!(error = ?err, url = %upstream_url, "upstream stream unavailable; injecting silence until reconnect");
                }
            }

            yield Ok(next_silence_chunk(&mut silence_offset));
            tokio::time::sleep(RETRY_INTERVAL).await;
        }
    }
}

fn normalize_mount(mount: &str) -> &'static str {
    match mount {
        "classical" => "classical",
        _ => "chill",
    }
}

fn upstream_stream_url(base_url: &str, mount: &str) -> String {
    if base_url.ends_with(&format!("/{mount}")) {
        base_url.to_string()
    } else {
        format!("{}/{}", base_url.trim_end_matches('/'), mount)
    }
}

fn next_silence_chunk(offset: &mut usize) -> Bytes {
    let start = (*offset).min(SILENCE_MP3.len());
    let end = (start + SILENCE_CHUNK_BYTES).min(SILENCE_MP3.len());
    let chunk = Bytes::copy_from_slice(&SILENCE_MP3[start..end]);
    *offset = if end >= SILENCE_MP3.len() { 0 } else { end };
    chunk
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_stream_url_appends_suffix_once() {
        assert_eq!(
            upstream_stream_url("http://icecast:8000", "chill"),
            "http://icecast:8000/chill"
        );
        assert_eq!(
            upstream_stream_url("http://icecast:8000/classical", "classical"),
            "http://icecast:8000/classical"
        );
    }

    #[test]
    fn silence_chunk_cycles_without_empty_output() {
        let mut offset = SILENCE_MP3.len().saturating_sub(16);
        let first = next_silence_chunk(&mut offset);
        let second = next_silence_chunk(&mut offset);

        assert!(!first.is_empty());
        assert!(!second.is_empty());
    }
}
