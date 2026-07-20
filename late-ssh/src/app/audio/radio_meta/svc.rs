use std::collections::HashMap;
use std::time::Duration;

use anyhow::Context;
use late_core::shutdown::CancellationToken;
use tokio::sync::watch;

// Metadata fetch only. Nightride audio is never proxied/restreamed through
// late.sh; clients connect directly to the official station stream URLs.
const NIGHTRIDE_META_URL: &str = "https://nightride.fm/meta";
const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_secs(1);
const RECONNECT_BACKOFF_MAX: Duration = Duration::from_secs(60);
// The feed sends keep-alive comments between track changes; a connection
// quiet for this long is dead. Without it a half-open connection would
// show stale artist/title forever and never reconnect.
const SSE_IDLE_READ_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArtistTitle {
    pub artist: String,
    pub title: String,
}

#[derive(serde::Deserialize)]
struct StationRecord {
    station: String,
    #[serde(default)]
    artist: String,
    #[serde(default)]
    title: String,
}

/// Nightride FM live metadata: one SSE connection to `/meta`, published as
/// a `station name -> ArtistTitle` watch. Consumers fall back to the
/// station display name for any station missing from the map (startup,
/// disconnect, gap, parse failure).
#[derive(Clone)]
pub struct RadioMetaService {
    tx: watch::Sender<HashMap<String, ArtistTitle>>,
    rx: watch::Receiver<HashMap<String, ArtistTitle>>,
}

impl Default for RadioMetaService {
    fn default() -> Self {
        Self::new()
    }
}

impl RadioMetaService {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(HashMap::new());
        Self { tx, rx }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<HashMap<String, ArtistTitle>> {
        self.rx.clone()
    }

    pub fn start_task(&self, shutdown: CancellationToken) -> tokio::task::JoinHandle<()> {
        let tx = self.tx.clone();
        tokio::spawn(run_sse_loop(tx, shutdown))
    }
}

async fn run_sse_loop(
    tx: watch::Sender<HashMap<String, ArtistTitle>>,
    shutdown: CancellationToken,
) {
    let client = reqwest::Client::builder()
        .read_timeout(SSE_IDLE_READ_TIMEOUT)
        .build()
        .expect("building nightride meta http client");
    let mut backoff = RECONNECT_BACKOFF_INITIAL;
    loop {
        if shutdown.is_cancelled() {
            break;
        }

        match stream_events(&client, &tx, &shutdown).await {
            Ok(received_any) => {
                if received_any {
                    backoff = RECONNECT_BACKOFF_INITIAL;
                }
            }
            Err(err) => {
                tracing::warn!(error = ?err, "nightride meta stream failed");
            }
        }
        if shutdown.is_cancelled() {
            break;
        }

        // While disconnected the data is a gap; clear so consumers fall
        // back to station display names instead of stale artist/title.
        tx.send_replace(HashMap::new());

        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(RECONNECT_BACKOFF_MAX);
    }
    tracing::info!("nightride meta fetcher shutting down");
}

/// Reads one SSE connection until it ends or shutdown fires. Returns
/// whether any metadata event was successfully applied (used to reset the
/// reconnect backoff).
async fn stream_events(
    client: &reqwest::Client,
    tx: &watch::Sender<HashMap<String, ArtistTitle>>,
    shutdown: &CancellationToken,
) -> anyhow::Result<bool> {
    let mut response = client
        .get(NIGHTRIDE_META_URL)
        .header("accept", "text/event-stream")
        .send()
        .await
        .context("connecting to nightride meta sse")?
        .error_for_status()
        .context("nightride meta sse status")?;

    let mut buffer = String::new();
    let mut received_any = false;
    loop {
        let chunk = tokio::select! {
            _ = shutdown.cancelled() => return Ok(received_any),
            chunk = response.chunk() => chunk.context("reading nightride meta sse chunk")?,
        };
        let Some(chunk) = chunk else {
            tracing::debug!("nightride meta sse stream ended");
            return Ok(received_any);
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(newline) = buffer.find('\n') {
            let line = buffer[..newline].trim_end_matches('\r').to_string();
            buffer.drain(..=newline);
            if let Some(stations) = parse_meta_line(&line) {
                received_any = true;
                // Merge rather than replace: an event carrying a subset of
                // stations must not blank the others.
                tx.send_modify(|map| map.extend(stations));
            }
        }
    }
}

/// One SSE line. Metadata events are a single `data:` line carrying a JSON
/// array of station records. Anything else (comments, empty keep-alives,
/// unparsable payloads, records missing artist/title) yields `None`.
fn parse_meta_line(line: &str) -> Option<HashMap<String, ArtistTitle>> {
    let payload = line.strip_prefix("data:")?.trim();
    if payload.is_empty() {
        return None;
    }
    let records: Vec<StationRecord> = match serde_json::from_str(payload) {
        Ok(records) => records,
        Err(err) => {
            tracing::debug!(error = ?err, "failed to parse nightride meta event");
            return None;
        }
    };

    let mut stations = HashMap::new();
    for record in records {
        let artist = record.artist.trim();
        let title = record.title.trim();
        if record.station.is_empty() || artist.is_empty() || title.is_empty() {
            continue;
        }
        stations.insert(
            record.station,
            ArtistTitle {
                artist: artist.to_string(),
                title: title.to_string(),
            },
        );
    }
    (!stations.is_empty()).then_some(stations)
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;

