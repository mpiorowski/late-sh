use std::collections::HashMap;

use late_core::{api_types::NowPlaying, icecast, shutdown::CancellationToken};
use tokio::sync::watch;

#[derive(Clone)]
pub struct NowPlayingService {
    icecast_url: String,
    tx: watch::Sender<HashMap<String, NowPlaying>>,
    rx: watch::Receiver<HashMap<String, NowPlaying>>,
}

impl NowPlayingService {
    pub fn new(icecast_url: String) -> Self {
        let (tx, rx) = watch::channel(HashMap::new());
        Self {
            icecast_url,
            tx,
            rx,
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<HashMap<String, NowPlaying>> {
        self.rx.clone()
    }

    pub fn start_poll_task(&self, shutdown: CancellationToken) -> tokio::task::JoinHandle<()> {
        let icecast_url = self.icecast_url.clone();
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || poll_now_playing(icecast_url, tx, shutdown))
    }
}

fn poll_now_playing(
    icecast_url: String,
    now_playing_tx: watch::Sender<HashMap<String, NowPlaying>>,
    shutdown: CancellationToken,
) {
    // Per-mount state: `started_at` must only reset when that mount's title
    // changes, so entries for unchanged mounts are carried over verbatim.
    let mut current: HashMap<String, NowPlaying> = HashMap::new();
    loop {
        if shutdown.is_cancelled() {
            tracing::info!("now playing fetcher shutting down");
            break;
        }

        match icecast::fetch_tracks(&icecast_url) {
            Ok(tracks) => {
                let mut changed = false;
                current.retain(|mount, _| {
                    let keep = tracks.contains_key(mount);
                    if !keep {
                        tracing::info!(mount = %mount, "mount disappeared from icecast status");
                        changed = true;
                    }
                    keep
                });
                for (mount, track) in tracks {
                    let title = track.to_string();
                    let title_changed = current
                        .get(&mount)
                        .is_none_or(|np| np.track.to_string() != title);
                    if title_changed {
                        tracing::info!(mount = %mount, track = %track, "now playing changed");
                        current.insert(mount, NowPlaying::new(track));
                        changed = true;
                    }
                }
                if changed && now_playing_tx.send(current.clone()).is_err() {
                    tracing::error!("failed to publish now playing update");
                    break;
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "failed to fetch now playing, retrying in 10s");
            }
        }

        for _ in 0..10 {
            if shutdown.is_cancelled() {
                tracing::info!("now playing fetcher shutting down");
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
