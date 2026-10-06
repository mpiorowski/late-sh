// The watch-session SSH client: connects to a door host as the reserved
// `late_watch` username and either follows the roster of live games
// (`list`) or one player's screen (`game:<playname>`). The username, env var,
// and both wire shapes mirror the host side (`late-dcss/src/watch.rs`), the
// same cross-crate contract style as the stats session; keep the copies in
// sync.
//
// Roster blocks are text:
// `game\t<playname>\t<started_unix>\t<watchers>\t<status>\n` per game,
// closed by `end\n`; `status` is the host's one line on where the player is
// and may be empty. Screen frames are binary,
// `[tag u8][len u32 BE][payload]`: `R` resets the watcher's parser to
// `cols u16 BE`, `rows u16 BE` and redraws it from the rest of the payload,
// `D` applies a diff. The parser therefore always holds the player's
// screen at the player's size; fitting it into this session's viewport is
// the renderer's job.
//
// There is no input path: the client never writes to the channel.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use late_core::models::leaderboard::DoorGame;
use russh::client::{self, Config, Handler};
use russh::keys::PublicKey;
use russh::{ChannelMsg, Disconnect};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

use crate::metrics::DoorWatchOutcome;
use crate::render_signal::RenderSignal;

/// The reserved SSH username that opens a watch session instead of a game.
pub const WATCH_USERNAME: &str = "late_watch";

/// Env request naming what the watch session streams.
pub const WATCH_ENV_VAR: &str = "LATE_DOOR_WATCH";

const FRAME_RESET: u8 = b'R';
const FRAME_DIFF: u8 = b'D';

/// Largest frame the client accepts. A full redraw of the biggest terminal
/// crawl draws into is well under this; anything larger is a broken stream.
const MAX_FRAME_LEN: usize = 4 * 1024 * 1024;

const SETUP_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a door host's watch sessions are served.
#[derive(Clone)]
pub struct WatchTarget {
    pub host: String,
    pub port: u16,
    /// The shared-secret-derived door client key (each door has its own
    /// blake3 domain; the caller derives it with that door's `identity`).
    pub key: russh::keys::PrivateKey,
}

/// One live game as the host's roster lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveGame {
    pub playname: String,
    pub started_unix: u64,
    pub watchers: usize,
    /// Where the player is, as the host read it off the game (`XL3 Lair:2`);
    /// empty until it has.
    pub status: String,
}

/// Why a stream from the host could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum WireError {
    UnknownFrameTag(u8),
    OversizedFrame(usize),
    ShortResetFrame,
    MalformedRosterLine(String),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownFrameTag(tag) => write!(f, "unknown watch frame tag {tag}"),
            Self::OversizedFrame(len) => write!(f, "watch frame of {len} bytes exceeds the cap"),
            Self::ShortResetFrame => write!(f, "watch reset frame without a size"),
            Self::MalformedRosterLine(line) => write!(f, "malformed roster line {line:?}"),
        }
    }
}

impl std::error::Error for WireError {}

/// One decoded screen frame.
#[derive(Debug, PartialEq, Eq)]
pub enum WatchFrame {
    Reset {
        cols: u16,
        rows: u16,
        contents: Vec<u8>,
    },
    Diff(Vec<u8>),
}

impl WatchFrame {
    pub fn apply(self, parser: &mut vt100::Parser) {
        match self {
            Self::Reset {
                cols,
                rows,
                contents,
            } => {
                *parser = vt100::Parser::new(rows.max(1), cols.max(1), 0);
                parser.process(&contents);
            }
            Self::Diff(bytes) => parser.process(&bytes),
        }
    }
}

/// Reassembles screen frames across SSH chunk boundaries.
#[derive(Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<WatchFrame>, WireError> {
        self.buf.extend_from_slice(bytes);
        let mut frames = Vec::new();
        loop {
            if self.buf.len() < 5 {
                return Ok(frames);
            }
            let tag = self.buf[0];
            let len =
                u32::from_be_bytes([self.buf[1], self.buf[2], self.buf[3], self.buf[4]]) as usize;
            if len > MAX_FRAME_LEN {
                return Err(WireError::OversizedFrame(len));
            }
            if self.buf.len() < 5 + len {
                return Ok(frames);
            }
            let payload: Vec<u8> = self.buf.drain(..5 + len).skip(5).collect();
            let frame = match tag {
                FRAME_RESET => {
                    if payload.len() < 4 {
                        return Err(WireError::ShortResetFrame);
                    }
                    WatchFrame::Reset {
                        cols: u16::from_be_bytes([payload[0], payload[1]]),
                        rows: u16::from_be_bytes([payload[2], payload[3]]),
                        contents: payload[4..].to_vec(),
                    }
                }
                FRAME_DIFF => WatchFrame::Diff(payload),
                other => return Err(WireError::UnknownFrameTag(other)),
            };
            frames.push(frame);
        }
    }
}

/// Reassembles roster blocks across SSH chunk boundaries.
#[derive(Default)]
pub struct RosterDecoder {
    buf: Vec<u8>,
    pending: Vec<LiveGame>,
}

impl RosterDecoder {
    /// Every roster block completed by `bytes`, oldest first.
    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<LiveGame>>, WireError> {
        self.buf.extend_from_slice(bytes);
        let mut rosters = Vec::new();
        while let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(&self.buf[..nl]).into_owned();
            self.buf.drain(..=nl);
            if line == "end" {
                rosters.push(std::mem::take(&mut self.pending));
                continue;
            }
            self.pending.push(parse_roster_line(&line)?);
        }
        Ok(rosters)
    }
}

fn parse_roster_line(line: &str) -> Result<LiveGame, WireError> {
    let malformed = || WireError::MalformedRosterLine(line.to_string());
    let mut parts = line.split('\t');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("game"), Some(playname), Some(started), Some(watchers), Some(status), None)
            if !playname.is_empty() =>
        {
            match (started.parse(), watchers.parse()) {
                (Ok(started_unix), Ok(watchers)) => Ok(LiveGame {
                    playname: playname.to_string(),
                    started_unix,
                    watchers,
                    status: status.to_string(),
                }),
                (Err(_), _) | (_, Err(_)) => Err(malformed()),
            }
        }
        _ => Err(malformed()),
    }
}

/// The door hosts are trusted late.sh-owned services on the internal
/// network; auth is the derived client key (same policy as the game doors).
struct AcceptAnyHostKey;

impl Handler for AcceptAnyHostKey {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Connect, authenticate as the watch username, send the request, and open
/// the shell. No PTY: the stream is data, not a terminal.
async fn open_watch(
    target: &WatchTarget,
    request: &str,
) -> Result<(
    client::Handle<AcceptAnyHostKey>,
    russh::Channel<client::Msg>,
)> {
    let config = Arc::new(Config {
        // A roster stream idles between games; keepalives hold it open.
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });
    let mut session = timeout(
        SETUP_TIMEOUT,
        client::connect(
            config,
            (target.host.as_str(), target.port),
            AcceptAnyHostKey,
        ),
    )
    .await
    .context("watch connect timed out")?
    .with_context(|| format!("connecting to {}:{}", target.host, target.port))?;

    let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(target.key.clone()), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(WATCH_USERNAME, key),
    )
    .await
    .context("watch authenticate_publickey timed out")?
    .context("watch authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("door host rejected derived credentials for watch session");
    }

    let channel = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("watch channel_open_session timed out")?
        .context("watch channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        channel.set_env(false, WATCH_ENV_VAR, request),
    )
    .await
    .context("watch set_env timed out")?
    .context("watch set_env failed")?;
    timeout(SETUP_TIMEOUT, channel.request_shell(true))
        .await
        .context("watch request_shell timed out")?
        .context("watch request_shell failed")?;
    Ok((session, channel))
}

/// Follow the host's roster until the connection ends (host restart, network
/// drop) or `tx` closes. The caller owns the retry policy.
pub async fn run_roster_stream(target: WatchTarget, tx: mpsc::Sender<Vec<LiveGame>>) -> Result<()> {
    let (session, mut channel) = open_watch(&target, "list").await?;
    let mut decoder = RosterDecoder::default();
    let result = loop {
        let Some(msg) = channel.wait().await else {
            break Ok(());
        };
        match msg {
            ChannelMsg::Data { data } => match decoder.push(&data) {
                Ok(rosters) => {
                    for roster in rosters {
                        if tx.send(roster).await.is_err() {
                            break;
                        }
                    }
                    if tx.is_closed() {
                        break Ok(());
                    }
                }
                Err(e) => break Err(anyhow::Error::new(e).context("reading roster stream")),
            },
            ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. } => break Ok(()),
            _ => {}
        }
    };
    let _ = channel.close().await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    result
}

/// Where a watched game's stream stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchStatus {
    Connecting,
    Watching,
    /// The stream closed: the game ended, was never live, or the connection
    /// dropped. Terminal; a new watch starts a new process.
    Ended,
}

/// One session's read-only view of a player's game. Owns a background task
/// that applies the host's frames to a shared `vt100::Parser`; dropping the
/// process aborts the task, which closes the connection.
pub struct SpectateProcess {
    task: JoinHandle<()>,
    parser: Arc<Mutex<vt100::Parser>>,
    status: Arc<Mutex<WatchStatus>>,
}

impl SpectateProcess {
    pub fn spawn(
        game: DoorGame,
        target: WatchTarget,
        playname: String,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        let parser = Arc::new(Mutex::new(vt100::Parser::new(24, 80, 0)));
        let status = Arc::new(Mutex::new(WatchStatus::Connecting));
        let task_parser = parser.clone();
        let task_status = status.clone();
        let task = tokio::spawn(async move {
            let result = run_game_stream(
                &target,
                &playname,
                &task_parser,
                &task_status,
                repaint.as_ref(),
            )
            .await;
            let outcome = match result {
                Ok(()) => DoorWatchOutcome::Closed,
                Err(e) => {
                    tracing::warn!(error = ?e, game = game.key(), playname = %playname, "watch stream failed");
                    DoorWatchOutcome::Failed
                }
            };
            crate::metrics::record_door_watch_stream(game, outcome);
            *task_status.lock().expect("status mutex") = WatchStatus::Ended;
            if let Some(sig) = &repaint {
                sig.wake();
            }
        });
        Self {
            task,
            parser,
            status,
        }
    }

    pub fn status(&self) -> WatchStatus {
        *self.status.lock().expect("status mutex")
    }

    /// Run a closure against the watched screen (avoids cloning the grid).
    pub fn with_screen<R>(&self, f: impl FnOnce(&vt100::Screen) -> R) -> R {
        let guard = self.parser.lock().expect("parser mutex");
        f(guard.screen())
    }
}

impl Drop for SpectateProcess {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run_game_stream(
    target: &WatchTarget,
    playname: &str,
    parser: &Mutex<vt100::Parser>,
    status: &Mutex<WatchStatus>,
    repaint: Option<&Arc<RenderSignal>>,
) -> Result<()> {
    let (session, mut channel) = open_watch(target, &format!("game:{playname}")).await?;
    let mut decoder = FrameDecoder::default();
    let result = loop {
        let Some(msg) = channel.wait().await else {
            break Ok(());
        };
        match msg {
            ChannelMsg::Data { data } => match decoder.push(&data) {
                Ok(frames) => {
                    if frames.is_empty() {
                        continue;
                    }
                    {
                        let mut parser = parser.lock().expect("parser mutex");
                        for frame in frames {
                            frame.apply(&mut parser);
                        }
                    }
                    *status.lock().expect("status mutex") = WatchStatus::Watching;
                    if let Some(sig) = repaint {
                        sig.wake();
                    }
                }
                Err(e) => break Err(anyhow::Error::new(e).context("reading watch stream")),
            },
            ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. } => break Ok(()),
            _ => {}
        }
    };
    let _ = channel.close().await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    result
}

#[cfg(test)]
#[path = "proxy_test.rs"]
mod proxy_test;
