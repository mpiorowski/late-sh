// Orchestration for the live-game rosters: one connect-with-retry task per
// door whose host serves watch sessions (spawned from main.rs), following the
// host's `list` stream and publishing each block as a snapshot every session
// reads. The roster drives the hub's watch list, the `s` key's target, and
// the watcher count a player sees in their own game's chrome.
//
// While the stream is down the published roster is empty, never stale: a
// list of games nobody can open is worse than no list.

use std::sync::Arc;
use std::time::Duration;

use late_core::shutdown::CancellationToken;
use tokio::sync::{mpsc, watch};

use super::proxy::{LiveGame, WatchTarget, run_roster_stream};
use super::state::SpectateGame;

/// Backoff between roster-stream attempts (host restarts, rollouts, network
/// blips). Short: the hub shows nobody playing until it reconnects.
const RETRY_DELAY: Duration = Duration::from_secs(10);

type Roster = Arc<Vec<LiveGame>>;

/// The published rosters plus this holder's read position. Cloned into every
/// session, so each clone tracks its own "seen" for [`Self::tick`].
#[derive(Clone)]
pub struct LiveGamesService {
    dcss_tx: Arc<watch::Sender<Roster>>,
    dcss: watch::Receiver<Roster>,
}

impl LiveGamesService {
    pub fn new() -> Self {
        let (dcss_tx, dcss) = watch::channel(Arc::new(Vec::new()));
        Self {
            dcss_tx: Arc::new(dcss_tx),
            dcss,
        }
    }

    /// The live games of `game`, oldest first.
    pub fn roster(&self, game: SpectateGame) -> Roster {
        match game {
            SpectateGame::Dcss => self.dcss.borrow().clone(),
        }
    }

    /// Watchers on `playname`'s game, `None` when it is not listed.
    pub fn watchers_of(&self, game: SpectateGame, playname: &str) -> Option<usize> {
        self.roster(game)
            .iter()
            .find(|g| g.playname == playname)
            .map(|g| g.watchers)
    }

    /// Whether a roster changed since this holder last looked. Marks it seen.
    pub fn tick(&mut self) -> bool {
        match self.dcss.has_changed() {
            Ok(true) => {
                self.dcss.borrow_and_update();
                true
            }
            Ok(false) | Err(_) => false,
        }
    }

    /// Spawn `game`'s roster loop: connect, follow, publish; on any end
    /// (host rollout, network drop, broken stream) publish an empty roster
    /// and reconnect after [`RETRY_DELAY`].
    pub fn start_task(
        &self,
        game: SpectateGame,
        target: WatchTarget,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        let publish = match game {
            SpectateGame::Dcss => self.dcss_tx.clone(),
        };
        tokio::spawn(async move {
            loop {
                let (tx, mut rx) = mpsc::channel::<Vec<LiveGame>>(8);
                let follow = async {
                    while let Some(roster) = rx.recv().await {
                        publish.send_replace(Arc::new(roster));
                    }
                };
                let result = tokio::select! {
                    (result, ()) = async { tokio::join!(run_roster_stream(target.clone(), tx), follow) } => result,
                    _ = shutdown.cancelled() => return,
                };
                publish.send_replace(Arc::new(Vec::new()));
                match result {
                    Ok(()) => tracing::info!(
                        game = game.door_game().key(),
                        "door roster stream ended; reconnecting"
                    ),
                    Err(e) => {
                        tracing::warn!(
                            error = ?e,
                            game = game.door_game().key(),
                            "door roster stream failed; retrying"
                        );
                        crate::metrics::record_door_watch_roster_failure(game.door_game());
                    }
                }
                tokio::select! {
                    _ = tokio::time::sleep(RETRY_DELAY) => {}
                    _ = shutdown.cancelled() => return,
                }
            }
        })
    }
}

impl Default for LiveGamesService {
    fn default() -> Self {
        Self::new()
    }
}
