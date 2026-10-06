// Orchestration for the live-game rosters: one connect-with-retry task per
// door whose host serves watch sessions (spawned from main.rs), following the
// host's `list` stream and publishing each block as a snapshot every session
// reads. The roster drives the live rows on the hub's rail, the `s` key's
// target, and the watcher count a player sees in their own game's chrome.
//
// While the stream is down the published roster is empty, never stale: a
// list of games nobody can open is worse than no list.
//
// It also resolves the watch-chat rooms: one permanent chat room per player,
// per door (`ChatRoom::get_or_create_watch_room`), created the first time
// anyone needs it and cached for the life of the process, since a room never
// changes once it exists.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use late_core::db::Db;
use late_core::models::chat_room::ChatRoom;
use late_core::shutdown::CancellationToken;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::proxy::{LiveGame, WatchTarget, run_roster_stream};
use super::state::{LiveRow, SpectateGame};
use crate::render_signal::RenderSignal;

/// Backoff between roster-stream attempts (host restarts, rollouts, network
/// blips). Short: the hub shows nobody playing until it reconnects.
const RETRY_DELAY: Duration = Duration::from_secs(10);

type Roster = Arc<Vec<LiveGame>>;

/// Where a player's watch-chat room stands in the process-wide cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RoomSlot {
    /// A lookup is in flight; nobody else needs to start one.
    Resolving,
    Ready(Uuid),
}

type ChatRooms = Arc<Mutex<HashMap<(SpectateGame, String), RoomSlot>>>;

/// The published rosters plus this holder's read position, and the shared
/// watch-chat room cache. Cloned into every session, so each clone tracks its
/// own "seen" for [`Self::tick`].
#[derive(Clone)]
pub struct LiveGamesService {
    db: Db,
    dcss_tx: Arc<watch::Sender<Roster>>,
    dcss: watch::Receiver<Roster>,
    chat_rooms: ChatRooms,
}

impl LiveGamesService {
    pub fn new(db: Db) -> Self {
        let (dcss_tx, dcss) = watch::channel(Arc::new(Vec::new()));
        Self {
            db,
            dcss_tx: Arc::new(dcss_tx),
            dcss,
            chat_rooms: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The watch-chat room for `playname`'s runs of `game`, once resolved.
    pub fn chat_room_id(&self, game: SpectateGame, playname: &str) -> Option<Uuid> {
        let rooms = self.chat_rooms.lock().expect("watch chat rooms mutex");
        match rooms.get(&(game, playname.to_string())) {
            Some(RoomSlot::Ready(room_id)) => Some(*room_id),
            Some(RoomSlot::Resolving) | None => None,
        }
    }

    /// Resolve (creating on first use) the watch-chat room for `playname`'s
    /// runs of `game`. A no-op when it is already resolved or in flight. A
    /// failure clears the slot, so the next watch or launch tries again;
    /// callers ask once per watch, never every tick.
    pub fn resolve_chat_room_task(
        &self,
        game: SpectateGame,
        playname: String,
        repaint: Option<Arc<RenderSignal>>,
    ) {
        let key = (game, playname);
        {
            let mut rooms = self.chat_rooms.lock().expect("watch chat rooms mutex");
            if rooms.contains_key(&key) {
                return;
            }
            rooms.insert(key.clone(), RoomSlot::Resolving);
        }
        let db = self.db.clone();
        let chat_rooms = self.chat_rooms.clone();
        tokio::spawn(async move {
            let resolved = watch_room_id(&db, game, &key.1).await;
            let mut rooms = chat_rooms.lock().expect("watch chat rooms mutex");
            match resolved {
                Ok(room_id) => {
                    rooms.insert(key, RoomSlot::Ready(room_id));
                }
                Err(e) => {
                    tracing::warn!(
                        error = ?e,
                        game = game.door_game().key(),
                        playname = %key.1,
                        "failed to resolve watch chat room"
                    );
                    crate::metrics::record_door_watch_chat_room_failure(game.door_game());
                    rooms.remove(&key);
                }
            }
            drop(rooms);
            if let Some(sig) = &repaint {
                sig.wake();
            }
        });
    }

    /// The live games of `game`, oldest first.
    pub fn roster(&self, game: SpectateGame) -> Roster {
        match game {
            SpectateGame::Dcss => self.dcss.borrow().clone(),
        }
    }

    /// Every live game across the watchable doors, in the order the Games
    /// hub's rail lists them: door by door, oldest game first.
    pub fn live_rows(&self) -> Vec<LiveRow> {
        SpectateGame::ALL
            .into_iter()
            .flat_map(|game| {
                let roster = self.roster(game);
                (0..roster.len()).map(move |index| LiveRow {
                    game,
                    entry: roster[index].clone(),
                })
            })
            .collect()
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

/// The id of the watch-chat room for `playname`'s runs of `game`, creating
/// the room when this is the first time anyone needs it.
async fn watch_room_id(db: &Db, game: SpectateGame, playname: &str) -> Result<Uuid> {
    let client = db.get().await.context("getting db client")?;
    let room = ChatRoom::get_or_create_watch_room(&client, game.door_game(), playname)
        .await
        .context("ensuring watch chat room")?;
    Ok(room.id)
}
