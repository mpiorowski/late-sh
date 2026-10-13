// The watch sessions: read-only spectating of live Zork games. late-ssh
// connects with the reserved `late_watch` username (outside the `late_<hex>`
// account labels, so no player can ever be it) and names what it wants in one
// env request before the shell:
//
// - `LATE_DOOR_WATCH=list` streams the roster of live games: one block per
//   change, `game\t<playname>\t<started_unix>\t<watchers>\t<status>\n` per
//   game, closed by `end\n`. The first block lands on connect, so a fresh
//   client never waits for a change to learn who is playing. `status` is the
//   edition and the room the player stands in (`hud_status`, read off Frotz's
//   status bar), the edition alone until the bar is drawn; the roster is
//   re-read every few seconds so it keeps up.
// - `LATE_DOOR_WATCH=game:<playname>` streams that player's screen. Frames
//   are `[tag u8][len u32 BE][payload]`: `R` (reset) carries `cols u16 BE`,
//   `rows u16 BE`, then a full redraw; `D` (diff) carries the bytes that turn
//   the last frame into the current screen. The channel closes when the game
//   ends (or was never live).
//
// A game is listed under its player's arcade handle, which late-ssh sends with
// every `play` as a display label only: accounts, leases and saves stay keyed
// by the account label.
//
// Watchers never see the child's raw output. Every live game keeps a host-side
// `vt100::Parser` fed with the same bytes the player gets, and each watcher is
// sent screen diffs computed from it (`contents_formatted` /
// `contents_diff`). Diffs are absolute cell writes, so a watcher who joins
// mid-game is exactly as right as the host's parser.
//
// There is no path from a watch session to a game's input: the session holds
// the game's `LiveGame` (screen + pulse), never its `PtyHost`.
//
// This module is the fourth copy of the roguelike hosts' `watch.rs` (the
// hosts share no library). The username, env var, and both wire shapes are
// duplicated in late-ssh's spectate client
// (`late-ssh/src/app/door/spectate/proxy.rs`); keep the copies in sync.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::ChannelId;
use russh::server::Handle;
use tokio::sync::{mpsc, watch};

use crate::protocol::{Edition, parse_playname};

/// The reserved SSH username that opens a watch session instead of a game.
pub(crate) const WATCH_USERNAME: &str = "late_watch";

/// Env request naming what the watch session streams.
pub(crate) const WATCH_ENV_VAR: &str = "LATE_DOOR_WATCH";

/// Reset frame: `cols u16 BE`, `rows u16 BE`, then a full redraw.
pub(crate) const FRAME_RESET: u8 = b'R';
/// Diff frame: bytes that turn the previous frame into the current screen.
pub(crate) const FRAME_DIFF: u8 = b'D';

/// Coalescing window between screen frames. Frotz writes a turn as many small
/// chunks; one diff per window keeps a watcher at most ~20 frames a second no
/// matter how chatty the game is.
const FRAME_INTERVAL: Duration = Duration::from_millis(50);

/// Coalescing window between roster blocks, so a burst of joins and leaves
/// lands as one block.
const LIST_INTERVAL: Duration = Duration::from_millis(500);

/// How often a roster session re-reads the games' statuses with nothing else
/// changing. A block goes out only when it differs from the last one sent.
const STATUS_INTERVAL: Duration = Duration::from_secs(5);

/// Longest room name the roster carries; anything longer is not a room.
const PLACE_MAX_CHARS: usize = 32;

/// What follows the room name on Frotz's V3 status bar: the score in its full
/// and narrow forms, or the clock of a timed story.
const STATUS_MARKERS: [&str; 3] = [" Score: ", " S: ", " Time: "];

/// What a watch session asked for, parsed from [`WATCH_ENV_VAR`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum WatchRequest {
    List,
    Game(String),
}

/// Parse the env value. `None` for anything else, including a game name
/// outside the handle shape: the registry is keyed by handles, so nothing
/// else could match.
pub(crate) fn parse_request(value: &str) -> Option<WatchRequest> {
    if value == "list" {
        return Some(WatchRequest::List);
    }
    let name = value.strip_prefix("game:")?;
    parse_playname(name).map(WatchRequest::Game)
}

/// One live game as the roster lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListedGame {
    pub(crate) playname: String,
    pub(crate) started_unix: u64,
    pub(crate) watchers: usize,
    /// The edition and where the player is, `Zork I · West of House`; the
    /// edition alone until the status bar was seen.
    pub(crate) status: String,
}

/// One roster block: every game line, then the `end` terminator.
pub(crate) fn encode_list(games: &[ListedGame]) -> Vec<u8> {
    let mut out = Vec::new();
    for game in games {
        out.extend_from_slice(
            format!(
                "game\t{}\t{}\t{}\t{}\n",
                game.playname, game.started_unix, game.watchers, game.status
            )
            .as_bytes(),
        );
    }
    out.extend_from_slice(b"end\n");
    out
}

/// The room a player stands in, read off Frotz's status bar on the mirrored
/// screen's top row: `West of House` from
/// ` West of House          Score: 0        Moves: 1`. `None` when the top row
/// is not the status bar (the story has not drawn it yet), so the caller
/// keeps what it last read.
pub(crate) fn hud_status(top_row: &str) -> Option<String> {
    let cut = STATUS_MARKERS
        .iter()
        .filter_map(|marker| top_row.find(marker))
        .min()?;
    let place = top_row[..cut].trim();
    let plausible = !place.is_empty()
        && place.chars().count() <= PLACE_MAX_CHARS
        && !place.chars().any(char::is_control);
    plausible.then(|| place.to_string())
}

/// The roster's status for a game of `edition`: the edition, then the room
/// once one was read.
pub(crate) fn status_line(edition: Edition, place: Option<&str>) -> String {
    match place {
        Some(place) => format!("{} \u{b7} {place}", edition.title()),
        None => edition.title().to_string(),
    }
}

fn encode_frame(tag: u8, payload: &[u8]) -> Vec<u8> {
    let len = u32::try_from(payload.len()).expect("a terminal frame fits in u32");
    let mut out = Vec::with_capacity(5 + payload.len());
    out.push(tag);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// The frame that brings a watcher from `prev` (the screen it last got) to
/// `screen`: a reset on the first frame or a size change, a diff otherwise,
/// and `None` when nothing visible changed.
pub(crate) fn screen_frame(
    prev: Option<&vt100::Screen>,
    screen: &vt100::Screen,
) -> Option<Vec<u8>> {
    match prev {
        Some(prev) if prev.size() == screen.size() => {
            let diff = screen.contents_diff(prev);
            if diff.is_empty() {
                None
            } else {
                Some(encode_frame(FRAME_DIFF, &diff))
            }
        }
        Some(_) | None => {
            let (rows, cols) = screen.size();
            let mut payload = Vec::new();
            payload.extend_from_slice(&cols.to_be_bytes());
            payload.extend_from_slice(&rows.to_be_bytes());
            payload.extend_from_slice(&screen.contents_formatted());
            Some(encode_frame(FRAME_RESET, &payload))
        }
    }
}

/// One live Zork game: the host-side mirror of the player's screen, plus the
/// pulse watchers wait on.
pub(crate) struct LiveGame {
    started_unix: u64,
    edition: Edition,
    parser: Mutex<vt100::Parser>,
    /// Pulsed (`send_modify`) on every screen change; flipped to `true` when
    /// the game ends, which is what closes its watch sessions.
    ended: watch::Sender<bool>,
    watchers: AtomicUsize,
    /// The last room read off the status bar (`hud_status`), kept while the
    /// bar is not on screen. Refreshed whenever the roster is listed.
    place: Mutex<Option<String>>,
}

impl LiveGame {
    fn snapshot(&self) -> vt100::Screen {
        self.parser
            .lock()
            .expect("live parser mutex")
            .screen()
            .clone()
    }

    /// Re-read the room off the status bar, keeping the last one while the
    /// bar is not on screen, and return the status that now stands.
    fn refresh_status(&self) -> String {
        let top_row = {
            let parser = self.parser.lock().expect("live parser mutex");
            let (_, cols) = parser.screen().size();
            parser.screen().contents_between(0, 0, 0, cols)
        };
        let mut place = self.place.lock().expect("live place mutex");
        if let Some(read) = hud_status(&top_row) {
            *place = Some(read);
        }
        status_line(self.edition, place.as_deref())
    }
}

/// Every live game on this host, by playname. Shared by the bridges (which
/// register their game and feed it) and the watch sessions (which read it).
pub(crate) struct LiveRegistry {
    games: Mutex<HashMap<String, Arc<LiveGame>>>,
    /// Pulsed whenever the roster changes: a game starts or ends, a watcher
    /// joins or leaves.
    roster: watch::Sender<()>,
}

impl LiveRegistry {
    pub(crate) fn new() -> Arc<Self> {
        let (roster, _) = watch::channel(());
        Arc::new(Self {
            games: Mutex::new(HashMap::new()),
            roster,
        })
    }

    /// Register a game that just spawned. The returned handle feeds the mirror
    /// and unregisters on drop. A second game under the same playname (an
    /// edition switch racing the old child's teardown) replaces the first;
    /// the first's drop then leaves the newer entry alone.
    pub(crate) fn register(
        self: &Arc<Self>,
        playname: &str,
        edition: Edition,
        cols: u16,
        rows: u16,
    ) -> LiveHandle {
        let game = Arc::new(LiveGame {
            started_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("wall clock after the unix epoch")
                .as_secs(),
            edition,
            parser: Mutex::new(vt100::Parser::new(rows.max(1), cols.max(1), 0)),
            ended: watch::channel(false).0,
            watchers: AtomicUsize::new(0),
            place: Mutex::new(None),
        });
        self.games
            .lock()
            .expect("live registry mutex")
            .insert(playname.to_string(), game.clone());
        self.roster.send_modify(|_| {});
        LiveHandle {
            registry: self.clone(),
            playname: playname.to_string(),
            game,
        }
    }

    fn get(&self, playname: &str) -> Option<Arc<LiveGame>> {
        self.games
            .lock()
            .expect("live registry mutex")
            .get(playname)
            .cloned()
    }

    /// The roster, sorted by start time (oldest first) so the client's order
    /// is stable across blocks. Reads each game's status off its screen on
    /// the way.
    pub(crate) fn list(&self) -> Vec<ListedGame> {
        let mut games: Vec<ListedGame> = self
            .games
            .lock()
            .expect("live registry mutex")
            .iter()
            .map(|(playname, game)| ListedGame {
                playname: playname.clone(),
                started_unix: game.started_unix,
                watchers: game.watchers.load(Ordering::Relaxed),
                status: game.refresh_status(),
            })
            .collect();
        games.sort_by(|a, b| {
            a.started_unix
                .cmp(&b.started_unix)
                .then_with(|| a.playname.cmp(&b.playname))
        });
        games
    }

    fn subscribe_roster(&self) -> watch::Receiver<()> {
        self.roster.subscribe()
    }
}

/// The bridge's handle on its registered game.
pub(crate) struct LiveHandle {
    registry: Arc<LiveRegistry>,
    playname: String,
    game: Arc<LiveGame>,
}

impl LiveHandle {
    /// Mirror a chunk of the child's output.
    pub(crate) fn feed(&self, bytes: &[u8]) {
        self.game
            .parser
            .lock()
            .expect("live parser mutex")
            .process(bytes);
        self.game.ended.send_modify(|_| {});
    }

    /// Mirror a window change. Watchers get a reset frame at the new size.
    pub(crate) fn resize(&self, cols: u16, rows: u16) {
        self.game
            .parser
            .lock()
            .expect("live parser mutex")
            .screen_mut()
            .set_size(rows.max(1), cols.max(1));
        self.game.ended.send_modify(|_| {});
    }
}

impl Drop for LiveHandle {
    fn drop(&mut self) {
        {
            let mut games = self.registry.games.lock().expect("live registry mutex");
            if games
                .get(&self.playname)
                .is_some_and(|current| Arc::ptr_eq(current, &self.game))
            {
                games.remove(&self.playname);
            }
        }
        self.game.ended.send_replace(true);
        self.registry.roster.send_modify(|_| {});
    }
}

/// Counts one watcher on a game for as long as it lives.
struct WatcherGuard {
    registry: Arc<LiveRegistry>,
    game: Arc<LiveGame>,
}

impl WatcherGuard {
    fn new(registry: Arc<LiveRegistry>, game: Arc<LiveGame>) -> Self {
        game.watchers.fetch_add(1, Ordering::Relaxed);
        registry.roster.send_modify(|_| {});
        Self { registry, game }
    }
}

impl Drop for WatcherGuard {
    fn drop(&mut self) {
        self.game.watchers.fetch_sub(1, Ordering::Relaxed);
        self.registry.roster.send_modify(|_| {});
    }
}

/// Per-session host for one watch stream. Owns a detached background task;
/// dropping the host (client EOF/close) drops `_stop_tx`, which the task
/// observes and exits on.
pub(crate) struct WatchHost {
    _stop_tx: mpsc::Sender<()>,
}

impl WatchHost {
    pub(crate) fn spawn(
        registry: Arc<LiveRegistry>,
        request: WatchRequest,
        handle: Handle,
        channel: ChannelId,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
        tokio::spawn(async move {
            match request {
                WatchRequest::List => {
                    run_list(registry, &handle, channel, stop_rx, shutdown_rx).await;
                }
                WatchRequest::Game(playname) => {
                    run_game(registry, &playname, &handle, channel, stop_rx, shutdown_rx).await;
                }
            }
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });
        Self { _stop_tx: stop_tx }
    }
}

/// Why a watch loop's wait ended.
enum Wake {
    Changed,
    Stop,
}

/// Wait for `changed`, the session's end, or host shutdown.
async fn wait_for<T>(
    changed: &mut watch::Receiver<T>,
    stop_rx: &mut mpsc::Receiver<()>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Wake {
    tokio::select! {
        res = changed.changed() => match res {
            Ok(()) => Wake::Changed,
            Err(_) => Wake::Stop,
        },
        _ = stop_rx.recv() => Wake::Stop,
        res = shutdown_rx.changed() => match res {
            Ok(()) if !*shutdown_rx.borrow() => Wake::Changed,
            Ok(()) | Err(_) => Wake::Stop,
        },
    }
}

async fn run_list(
    registry: Arc<LiveRegistry>,
    handle: &Handle,
    channel: ChannelId,
    mut stop_rx: mpsc::Receiver<()>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let mut roster = registry.subscribe_roster();
    let mut sent: Option<Vec<u8>> = None;
    loop {
        roster.borrow_and_update();
        let block = encode_list(&registry.list());
        if sent.as_ref() != Some(&block) {
            if handle.data(channel, block.clone()).await.is_err() {
                return;
            }
            sent = Some(block);
        }
        // A roster change wakes this at once; with none, the timeout brings
        // it round to re-read the statuses.
        let wake = tokio::time::timeout(
            STATUS_INTERVAL,
            wait_for(&mut roster, &mut stop_rx, &mut shutdown_rx),
        )
        .await;
        match wake {
            Ok(Wake::Changed) => tokio::time::sleep(LIST_INTERVAL).await,
            Ok(Wake::Stop) => return,
            Err(_elapsed) => {}
        }
    }
}

async fn run_game(
    registry: Arc<LiveRegistry>,
    playname: &str,
    handle: &Handle,
    channel: ChannelId,
    mut stop_rx: mpsc::Receiver<()>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let Some(game) = registry.get(playname) else {
        tracing::info!(playname, "watch requested for a game that is not live");
        return;
    };
    let _watcher = WatcherGuard::new(registry, game.clone());
    tracing::info!(playname, "watch session streaming");

    let mut pulse = game.ended.subscribe();
    let mut prev: Option<vt100::Screen> = None;
    loop {
        let ended = *pulse.borrow_and_update();
        let screen = game.snapshot();
        if let Some(frame) = screen_frame(prev.as_ref(), &screen)
            && handle.data(channel, frame).await.is_err()
        {
            return;
        }
        prev = Some(screen);
        if ended {
            return;
        }
        match wait_for(&mut pulse, &mut stop_rx, &mut shutdown_rx).await {
            Wake::Changed => tokio::time::sleep(FRAME_INTERVAL).await,
            Wake::Stop => return,
        }
    }
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod watch_test;
