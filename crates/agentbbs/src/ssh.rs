//! The anonymous SSH front door.
//!
//! Every connection is anonymous: we accept `none` auth and any public key,
//! mint a throwaway [`agentbbs_core::Identity`] per session, and never log the
//! client's key or address. All sessions share one [`Store`] so callers see the
//! same boards and messages.
//!
//! ## Rendering
//!
//! Rather than implement a [`ratatui::backend::Backend`] from scratch, we reuse
//! crossterm's [`CrosstermBackend`] pointed at a shared in-memory byte buffer
//! ([`SinkBuffer`]). On each redraw we drive the ratatui [`Terminal`] (which
//! emits ANSI positioned writes + SGR colors via crossterm), then drain the
//! accumulated bytes and ship them over the russh channel. This is the same
//! approach the in-tree `late-ssh` server uses.
//!
//! ## Input
//!
//! Inbound SSH bytes are decoded by [`crate::keys::KeyDecoder`] into crossterm
//! key events and fed to the [`App`]; we redraw after each batch and tear the
//! session down when the app returns [`Control::Quit`].

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use crossterm::terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use russh::keys::signature::rand_core::UnwrapErr;
use russh::keys::PrivateKey;
use russh::server::{Auth, Config, Handler, Msg, Server as RusshServer, Session};
use russh::{Channel, ChannelId};
use tokio::sync::Mutex as TokioMutex;

use agentbbs_core::store::{MemoryStore, RedbStore};
use agentbbs_core::Store;
use agentbbs_tui::{App, Control};

use crate::keys::KeyDecoder;

/// A clonable, shared byte sink. crossterm writes ANSI into it; we drain it and
/// forward the bytes over SSH.
#[derive(Clone, Default)]
pub struct SinkBuffer {
    inner: Arc<Mutex<Vec<u8>>>,
}

impl SinkBuffer {
    fn take(&self) -> Vec<u8> {
        let mut g = self.inner.lock().expect("sink poisoned");
        std::mem::take(&mut *g)
    }
}

impl Write for SinkBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner
            .lock()
            .expect("sink poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Per-session terminal state: a ratatui terminal whose backend writes into a
/// shared [`SinkBuffer`], plus the AgentBBS app and the input decoder.
struct SessionTerm {
    terminal: Terminal<CrosstermBackend<SinkBuffer>>,
    sink: SinkBuffer,
    app: App,
    decoder: KeyDecoder,
}

impl SessionTerm {
    fn new(
        store: Arc<dyn Store>,
        presence: Arc<agentbbs_core::Presence>,
        cols: u16,
        rows: u16,
    ) -> Result<Self> {
        let sink = SinkBuffer::default();
        let backend = CrosstermBackend::new(sink.clone());
        let viewport = Viewport::Fixed(Rect::new(0, 0, cols.max(1), rows.max(1)));
        let terminal = Terminal::with_options(backend, TerminalOptions { viewport })
            .context("create ssh terminal")?;
        Ok(SessionTerm {
            terminal,
            sink,
            // Share the node-wide presence registry so all SSH sessions see
            // each other in Who's Online.
            app: App::with_presence(store, presence),
            decoder: KeyDecoder::new(),
        })
    }

    /// Render the app and return the ANSI bytes to send to the client.
    fn render(&mut self) -> Result<Vec<u8>> {
        let app = &mut self.app;
        self.terminal
            .draw(|f| app.render(f))
            .context("draw frame")?;
        Ok(self.sink.take())
    }

    fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.terminal
            .resize(Rect::new(0, 0, cols.max(1), rows.max(1)))
            .context("resize terminal")?;
        Ok(())
    }

    /// Feed raw input bytes; returns true if the app asked to quit.
    fn handle_input(&mut self, data: &[u8]) -> bool {
        let mut quit = false;
        for key in self.decoder.feed(data) {
            if self.app.on_key(key) == Control::Quit {
                quit = true;
            }
        }
        quit
    }
}

/// Bytes to switch the client into the alternate screen with a hidden cursor.
fn enter_alt_screen() -> Vec<u8> {
    let mut buf = Vec::new();
    execute!(
        buf,
        EnterAlternateScreen,
        cursor::Hide,
        Clear(ClearType::All)
    )
    .expect("compose enter-alt-screen");
    buf
}

/// Bytes to restore the client's normal screen on disconnect.
fn leave_alt_screen() -> Vec<u8> {
    let mut buf = Vec::new();
    execute!(
        buf,
        Clear(ClearType::All),
        cursor::Show,
        LeaveAlternateScreen
    )
    .expect("compose leave-alt-screen");
    buf
}

/// Default anonymous SSH connection budget per source: 30 new connections per
/// minute (keyed by a non-cryptographic hash of the IP, never the raw address).
const SSH_CONN_PER_MIN: u32 = 30;

/// A non-cryptographic bucket key for `addr`'s IP — enough to rate-limit a
/// source without retaining or logging the raw address (anonymity).
fn peer_bucket(peer: Option<std::net::SocketAddr>) -> Option<String> {
    use std::hash::{Hash, Hasher};
    peer.map(|p| {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        p.ip().hash(&mut h);
        format!("{:016x}", h.finish())
    })
}

/// The russh server: holds the shared store cloned into each session, plus a
/// per-source connection-rate limiter shared across all connections.
#[derive(Clone)]
struct BbsServer {
    store: Arc<dyn Store>,
    rate: Arc<agentbbs_core::RateLimiter>,
    /// Node-wide presence registry shared by every session.
    presence: Arc<agentbbs_core::Presence>,
    started: std::time::Instant,
}

impl RusshServer for BbsServer {
    type Handler = BbsHandler;

    fn new_client(&mut self, peer: Option<std::net::SocketAddr>) -> BbsHandler {
        // Bound new-connection rate per source (DoS mitigation). We key on a
        // hash of the IP and never store or log the address itself.
        let now_ms = self.started.elapsed().as_millis() as u64;
        let throttled = match peer_bucket(peer) {
            Some(key) => !self.rate.allow(&key, now_ms),
            None => false,
        };
        // Occasionally prune stale buckets so the map stays bounded.
        self.rate.gc(now_ms);
        BbsHandler {
            store: self.store.clone(),
            presence: self.presence.clone(),
            channel: None,
            term: None,
            cols: 80,
            rows: 24,
            throttled,
        }
    }
}

/// One anonymous SSH connection.
struct BbsHandler {
    store: Arc<dyn Store>,
    presence: Arc<agentbbs_core::Presence>,
    channel: Option<Channel<Msg>>,
    /// The live session terminal, shared with the render path.
    term: Option<Arc<TokioMutex<SessionTerm>>>,
    cols: u16,
    rows: u16,
    /// Whether this connection's source exceeded the connection-rate budget;
    /// if so, auth is rejected.
    throttled: bool,
}

impl BbsHandler {
    /// Accept anonymous auth unless this source exceeded the connection budget.
    fn auth_decision(&self) -> Auth {
        if self.throttled {
            Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        } else {
            Auth::Accept
        }
    }
}

impl Handler for BbsHandler {
    type Error = anyhow::Error;

    // --- Anonymous auth: accept everything (subject to rate limiting). ---

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        Ok(self.auth_decision())
    }

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        // Accept any key; we never store or log it.
        Ok(self.auth_decision())
    }

    async fn auth_keyboard_interactive(
        &mut self,
        _user: &str,
        _submethods: &str,
        _response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth, Self::Error> {
        Ok(self.auth_decision())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        self.channel = Some(channel);
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.cols = (col_width as u16).max(1);
        self.rows = (row_height as u16).max(1);
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let Some(chan) = self.channel.take() else {
            return Ok(());
        };
        let channel_id = chan.id();
        let handle = session.handle();

        // Build the per-session terminal over a clone of the shared store, so
        // every anonymous caller sees the same boards and messages.
        let mut term = SessionTerm::new(
            self.store.clone(),
            self.presence.clone(),
            self.cols,
            self.rows,
        )?;
        let init = enter_alt_screen();
        let _ = handle.data(channel_id, init).await;
        // Paint the initial splash + menu.
        let first = term.render()?;
        let _ = handle.data(channel_id, first).await;

        let term = Arc::new(TokioMutex::new(term));
        self.term = Some(term.clone());

        // Pump input: the `data` handler forwards bytes through the channel's
        // own reader. We instead drive rendering reactively from `data()`; here
        // we just keep the channel object alive by dropping `chan` — russh
        // continues to deliver `data` callbacks to this handler.
        drop(chan);
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(term) = self.term.clone() else {
            return Ok(());
        };
        let handle = session.handle();
        let mut term = term.lock().await;
        let quit = term.handle_input(data);
        let frame = term.render()?;
        drop(term);
        let _ = handle.data(channel, frame).await;
        if quit {
            let _ = handle.data(channel, leave_alt_screen()).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.cols = (col_width as u16).max(1);
        self.rows = (row_height as u16).max(1);
        if let Some(term) = self.term.clone() {
            let handle = session.handle();
            let mut term = term.lock().await;
            term.resize(self.cols, self.rows)?;
            let frame = term.render()?;
            drop(term);
            let _ = handle.data(channel, frame).await;
        }
        Ok(())
    }
}

/// The AgentBBS data directory: `$XDG_DATA_HOME/agentbbs` if set, otherwise
/// `$HOME/.local/share/agentbbs`. Falls back to a relative `./agentbbs-data`
/// only if neither environment variable is present (rare; e.g. minimal CI).
fn data_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(xdg).join("agentbbs");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home).join(".local/share/agentbbs");
    }
    PathBuf::from("agentbbs-data")
}

/// The default on-disk path for the persisted SSH ed25519 host key:
/// `<data_dir>/ssh_host_ed25519`.
fn default_host_key_path() -> PathBuf {
    data_dir().join("ssh_host_ed25519")
}

/// The default on-disk path for the durable redb store: `<data_dir>/bbs.redb`.
pub fn default_store_path() -> PathBuf {
    data_dir().join("bbs.redb")
}

/// Resolve the durable store path from `$AGENTBBS_STORE` or the default.
pub fn store_path_from_env() -> PathBuf {
    std::env::var_os("AGENTBBS_STORE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_store_path)
}

/// Generate a fresh ed25519 [`PrivateKey`].
fn generate_host_key() -> Result<PrivateKey> {
    PrivateKey::random(
        &mut UnwrapErr(getrandom::SysRng),
        russh::keys::Algorithm::Ed25519,
    )
    .context("generate ed25519 host key")
}

/// Persist `key` to `path` in OpenSSH format with `0600` permissions, creating
/// parent directories as needed.
fn persist_host_key(key: &PrivateKey, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create data dir {}", parent.display()))?;
    }
    let pem = key
        .to_openssh(russh::keys::ssh_key::LineEnding::LF)
        .context("encode host key to OpenSSH")?;
    std::fs::write(path, pem.as_bytes())
        .with_context(|| format!("write host key to {}", path.display()))?;
    set_key_permissions(path)?;
    Ok(())
}

/// Restrict the host-key file to owner read/write (`0600`) on Unix.
#[cfg(unix)]
fn set_key_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)
        .with_context(|| format!("chmod 0600 {}", path.display()))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_key_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

/// Load a persisted ed25519 host key from `path`; if it does not exist, generate
/// one, persist it (`0600`), and return it. Subsequent calls reload the same
/// key, giving a host key that is **stable across restarts** so clients using
/// `StrictHostKeyChecking` do not see key churn (and an active MITM is no longer
/// masked by normal restarts — threat S-3).
fn load_or_create_host_key(path: &Path) -> Result<PrivateKey> {
    if path.exists() {
        return russh::keys::load_secret_key(path, None)
            .with_context(|| format!("load persisted host key from {}", path.display()));
    }
    let key = generate_host_key()?;
    persist_host_key(&key, path)?;
    Ok(key)
}

/// Resolve the host key. With an explicit `--host-key PATH`, load exactly that
/// (override preserved). Otherwise load-or-create the persisted default key.
fn host_key(path: Option<&str>) -> Result<PrivateKey> {
    match path {
        Some(p) => {
            russh::keys::load_secret_key(p, None).with_context(|| format!("load host key from {p}"))
        }
        None => load_or_create_host_key(&default_host_key_path()),
    }
}

/// Open the durable redb store at `path`, falling back to an in-memory store if
/// it cannot be opened (e.g. read-only filesystem). The default front door uses
/// a durable store so boards/messages survive restarts.
pub fn open_store(path: &Path) -> Arc<dyn Store> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match RedbStore::open(path) {
        Ok(store) => {
            eprintln!("AgentBBS using durable store at {}", path.display());
            Arc::new(store)
        }
        Err(e) => {
            eprintln!(
                "warning: could not open durable store at {} ({e}); falling back to in-memory",
                path.display()
            );
            Arc::new(MemoryStore::new())
        }
    }
}

/// Run the anonymous SSH front door on `port`, blocking until shutdown.
///
/// The shared store is the durable redb store at `$AGENTBBS_STORE` (or
/// `<data_dir>/bbs.redb`), falling back to in-memory if it cannot be opened.
pub async fn run(port: u16, host_key_path: Option<String>) -> Result<()> {
    let store_path = std::env::var_os("AGENTBBS_STORE")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_store_path);
    let store: Arc<dyn Store> = open_store(&store_path);
    let key = host_key(host_key_path.as_deref())?;

    let config = Config {
        inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
        auth_rejection_time: std::time::Duration::from_secs(1),
        keys: vec![key],
        ..Default::default()
    };
    let config = Arc::new(config);
    let mut server = BbsServer {
        store,
        rate: Arc::new(agentbbs_core::RateLimiter::new(SSH_CONN_PER_MIN, 60_000)),
        presence: Arc::new(agentbbs_core::Presence::default()),
        started: std::time::Instant::now(),
    };

    eprintln!("AgentBBS SSH front door listening on 0.0.0.0:{port} (anonymous)");
    server
        .run_on_address(config, ("0.0.0.0", port))
        .await
        .context("ssh server error")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_bucket_keys_by_ip_only() {
        let a: std::net::SocketAddr = "1.2.3.4:10".parse().unwrap();
        let b: std::net::SocketAddr = "1.2.3.4:55".parse().unwrap();
        let c: std::net::SocketAddr = "1.2.3.5:10".parse().unwrap();
        assert_eq!(peer_bucket(Some(a)), peer_bucket(Some(b)));
        assert_ne!(peer_bucket(Some(a)), peer_bucket(Some(c)));
        assert!(peer_bucket(None).is_none());
    }

    #[test]
    fn ssh_connections_are_rate_limited_per_source() {
        let mut server = BbsServer {
            store: Arc::new(MemoryStore::new()),
            rate: Arc::new(agentbbs_core::RateLimiter::new(3, 60_000)),
            presence: Arc::new(agentbbs_core::Presence::default()),
            started: std::time::Instant::now(),
        };
        let ip: std::net::SocketAddr = "10.0.0.1:2222".parse().unwrap();
        let throttled = (0..5)
            .filter(|_| server.new_client(Some(ip)).throttled)
            .count();
        assert_eq!(throttled, 2, "4th and 5th connection from one IP throttled");
        // A different source is independent.
        let other: std::net::SocketAddr = "10.0.0.2:2222".parse().unwrap();
        assert!(!server.new_client(Some(other)).throttled);
    }

    /// Stable fingerprint helper: a key's public OpenSSH string.
    fn pubstr(k: &PrivateKey) -> String {
        k.public_key().to_openssh().expect("encode public key")
    }

    #[test]
    fn host_key_persisted_then_reloaded_is_stable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh_host_ed25519");

        // First call creates and persists the key.
        assert!(!path.exists());
        let k1 = load_or_create_host_key(&path).unwrap();
        assert!(path.exists(), "host key file should be written");

        // Second call reloads the *same* key, not a fresh one.
        let k2 = load_or_create_host_key(&path).unwrap();
        assert_eq!(
            pubstr(&k1),
            pubstr(&k2),
            "reloaded key must match persisted key"
        );
    }

    #[cfg(unix)]
    #[test]
    fn persisted_host_key_is_0600() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh_host_ed25519");
        let _ = load_or_create_host_key(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "host key must be owner-only");
    }

    #[test]
    fn explicit_host_key_override_loads_that_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom_key");
        // Seed a known key at the override path.
        let seeded = generate_host_key().unwrap();
        persist_host_key(&seeded, &path).unwrap();

        let loaded = host_key(Some(path.to_str().unwrap())).unwrap();
        assert_eq!(pubstr(&seeded), pubstr(&loaded));
    }

    #[test]
    fn open_store_creates_durable_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bbs.redb");
        let _store = open_store(&path);
        assert!(path.exists(), "redb store file should be created");
    }
}
