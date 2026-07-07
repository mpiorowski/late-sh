//! Process-global World Cup data service.
//!
//! One poll loop for the whole server, mirroring the `radio_meta` pattern: it
//! owns a `watch` snapshot, hands out a receiver to each session, and fetches
//! from FotMob. The twist is **demand gating** — the loop only hits the
//! network while at least one session is actually on the World Cup screen.
//! When no one is looking it parks on a `Notify` and makes zero requests; the
//! first viewer to arrive wakes it for an immediate fetch.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use chrono::Utc;
use late_core::shutdown::CancellationToken;
use tokio::sync::{Notify, watch};

use super::fotmob;
use super::model::WorldCupSnapshot;

/// Cadence between polls while the screen is being viewed.
const POLL_INTERVAL: Duration = Duration::from_secs(60);
const BACKOFF_INITIAL: Duration = Duration::from_secs(5);
const BACKOFF_MAX: Duration = Duration::from_secs(120);
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const ACCEPT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

/// Shared viewer counter + wake signal for the poll loop.
struct Gate {
    count: AtomicUsize,
    notify: Notify,
}

/// RAII handle proving a session is currently viewing the World Cup screen.
/// Incrementing on creation and decrementing on `Drop` means a dropped
/// session (disconnect) automatically releases its slot.
pub struct WorldCupViewer {
    gate: Arc<Gate>,
}

impl Drop for WorldCupViewer {
    fn drop(&mut self) {
        self.gate.count.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
pub struct WorldCupService {
    tx: watch::Sender<Arc<WorldCupSnapshot>>,
    rx: watch::Receiver<Arc<WorldCupSnapshot>>,
    gate: Arc<Gate>,
}

impl Default for WorldCupService {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldCupService {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(Arc::new(WorldCupSnapshot::default()));
        Self {
            tx,
            rx,
            gate: Arc::new(Gate {
                count: AtomicUsize::new(0),
                notify: Notify::new(),
            }),
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<Arc<WorldCupSnapshot>> {
        self.rx.clone()
    }

    /// Registers a viewer and returns a guard that releases on drop. Waking
    /// the loop on the 0→1 transition gives the first viewer fresh data
    /// without waiting out the poll interval.
    pub fn viewer(&self) -> WorldCupViewer {
        if self.gate.count.fetch_add(1, Ordering::SeqCst) == 0 {
            self.gate.notify.notify_one();
        }
        WorldCupViewer {
            gate: self.gate.clone(),
        }
    }

    pub fn start_task(&self, shutdown: CancellationToken) -> tokio::task::JoinHandle<()> {
        let tx = self.tx.clone();
        let gate = self.gate.clone();
        tokio::spawn(run_poll_loop(tx, gate, shutdown))
    }

    #[cfg(test)]
    fn viewer_count(&self) -> usize {
        self.gate.count.load(Ordering::SeqCst)
    }
}

async fn run_poll_loop(
    tx: watch::Sender<Arc<WorldCupSnapshot>>,
    gate: Arc<Gate>,
    shutdown: CancellationToken,
) {
    let client = match reqwest::Client::builder().timeout(HTTP_TIMEOUT).build() {
        Ok(client) => client,
        Err(err) => {
            tracing::error!(error = ?err, "failed to build world cup http client");
            return;
        }
    };

    let mut backoff = BACKOFF_INITIAL;
    loop {
        if shutdown.is_cancelled() {
            break;
        }

        // Park with zero network traffic until a viewer shows up.
        if gate.count.load(Ordering::SeqCst) == 0 {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = gate.notify.notified() => continue,
            }
        }

        let (wait, next_backoff) = match fetch_snapshot(&client).await {
            Ok(mut snap) => {
                snap.fetched_at = Some(Utc::now());
                snap.stale = false;
                let _ = tx.send_replace(Arc::new(snap));
                (POLL_INTERVAL, BACKOFF_INITIAL)
            }
            Err(err) => {
                tracing::warn!(error = ?err, "world cup fetch failed; serving last good data");
                mark_stale(&tx);
                (backoff, (backoff * 2).min(BACKOFF_MAX))
            }
        };
        backoff = next_backoff;

        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = tokio::time::sleep(wait) => {}
        }
    }
    tracing::info!("world cup fetcher shutting down");
}

async fn fetch_snapshot(client: &reqwest::Client) -> anyhow::Result<WorldCupSnapshot> {
    let html = client
        .get(fotmob::WORLD_CUP_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, ACCEPT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    fotmob::parse_page(&html)
        .ok_or_else(|| anyhow::anyhow!("world cup page had no parseable __NEXT_DATA__"))
}

/// Flags the currently published snapshot as stale without discarding it.
fn mark_stale(tx: &watch::Sender<Arc<WorldCupSnapshot>>) {
    let current = tx.borrow().clone();
    if current.stale {
        return;
    }
    let mut updated = (*current).clone();
    updated.stale = true;
    let _ = tx.send_replace(Arc::new(updated));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewer_guard_tracks_count() {
        let svc = WorldCupService::new();
        assert_eq!(svc.viewer_count(), 0);

        let a = svc.viewer();
        assert_eq!(svc.viewer_count(), 1);
        let b = svc.viewer();
        assert_eq!(svc.viewer_count(), 2);

        drop(a);
        assert_eq!(svc.viewer_count(), 1);
        drop(b);
        assert_eq!(svc.viewer_count(), 0);
    }
}
