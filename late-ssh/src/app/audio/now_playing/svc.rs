use late_core::{api_types::NowPlaying, icecast, shutdown::CancellationToken};
use tokio::sync::watch;

#[derive(Clone)]
pub struct NowPlayingService {
    icecast_url: String,
    tx: watch::Sender<Option<NowPlaying>>,
    rx: watch::Receiver<Option<NowPlaying>>,
}

impl NowPlayingService {
    pub fn new(icecast_url: String) -> Self {
        let (tx, rx) = watch::channel(None);
        Self {
            icecast_url,
            tx,
            rx,
        }
    }

    pub fn subscribe_state(&self) -> watch::Receiver<Option<NowPlaying>> {
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
    now_playing_tx: watch::Sender<Option<NowPlaying>>,
    shutdown: CancellationToken,
) {
    let mut last_title: Option<String> = None;
    loop {
        if shutdown.is_cancelled() {
            tracing::info!("now playing fetcher shutting down");
            break;
        }

        let result = icecast::fetch_track(&icecast_url);
        match result {
            Ok(track) => {
                tracing::debug!(track = %track, "fetched now playing");
                let current_title = track.to_string();
                if last_title.as_ref() != Some(&current_title) {
                    tracing::info!(track = %track, "now playing changed");
                    last_title = Some(current_title);
                    let now_playing = NowPlaying::new(track);
                    if let Err(err) = now_playing_tx.send(Some(now_playing)) {
                        tracing::error!(error = ?err, "failed to publish now playing update");
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = ?e, "failed to fetch now playing, retrying in 5s");
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
