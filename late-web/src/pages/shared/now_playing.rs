use anyhow::Context;
use late_core::telemetry::TracedExt;
use serde::Deserialize;

use crate::{AppState, metrics};

// late-web only surfaces the listener count; per-source track display lives
// on the connect page, fed over the pair WS.
#[derive(Clone, Debug, Default)]
pub struct NowPlayingStatus {
    pub listeners_count: Option<usize>,
}

#[derive(Deserialize)]
struct NowPlayingResponse {
    listeners_count: Option<usize>,
}

pub async fn fetch(state: &AppState) -> anyhow::Result<NowPlayingStatus> {
    let url = format!("{}/api/now-playing", state.config.ssh_internal_url);

    let response = state
        .http_client
        .get(&url)
        .send_traced()
        .await
        .map_err(|err| {
            metrics::record_now_playing_fetch("error");
            late_core::error_span!("now_playing_fetch_failed", error = ?err, url = %url, "failed to fetch now playing");
            err
        })
        .context("failed to fetch now playing")?;

    let np: NowPlayingResponse = response
        .json()
        .await
        .map_err(|err| {
            metrics::record_now_playing_fetch("error");
            late_core::error_span!("now_playing_parse_failed", error = ?err, "failed to parse now playing response");
            err
        })
        .context("failed to parse now playing response")?;

    metrics::record_now_playing_fetch("success");

    Ok(NowPlayingStatus {
        listeners_count: np.listeners_count,
    })
}
