use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use russh::keys::PublicKey;
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};

use crate::config::Config;
use crate::host::{HostConfig, PtyHost, SessionLease};
use crate::identity::derive_client_key;
use crate::playname;

/// Shared, connection-independent server state.
struct Shared {
    bin: String,
    /// Every child's shared, persistent `HOME` (see `Config::data_dir`).
    data_dir: String,
    /// The single client public key we accept (derived from the shared secret).
    authorized_key: PublicKey,
    /// Playnames with a live child right now. Every save lives in one shared
    /// `HOME` keyed by playname, so a second concurrent session under the
    /// same name would race the first's `<name>.save` (see
    /// `host::SessionLease`).
    active_playnames: Arc<Mutex<HashSet<String>>>,
}

#[derive(Clone)]
pub(crate) struct Server {
    shared: Arc<Shared>,
}

impl Server {
    pub(crate) fn new(config: &Config) -> Self {
        let authorized_key = derive_client_key(&config.secret).public_key().clone();
        Self {
            shared: Arc::new(Shared {
                bin: config.bin.clone(),
                data_dir: config.data_dir.clone(),
                authorized_key,
                active_playnames: Arc::new(Mutex::new(HashSet::new())),
            }),
        }
    }
}

impl russh::server::Server for Server {
    type Handler = ClientHandler;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> ClientHandler {
        ClientHandler {
            shared: self.shared.clone(),
            playname: None,
            channel: None,
            term: "xterm-256color".to_string(),
            cols: 80,
            rows: 24,
            host: None,
        }
    }
}

pub(crate) struct ClientHandler {
    shared: Arc<Shared>,
    /// The account's arcade handle, captured from the SSH username at auth
    /// time and re-sanitized. Becomes `BASHQUEST_AUTOLOGIN` for the child.
    /// `Some` only after the shared-secret key check passes, so it doubles as
    /// the authorization record: one field, so the two cannot disagree.
    playname: Option<String>,
    /// Session channel, set on open and consumed when the shell starts.
    channel: Option<Channel<Msg>>,
    term: String,
    cols: u16,
    rows: u16,
    /// The running bashquest.sh child, once the shell is requested.
    host: Option<PtyHost>,
}

fn reject() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
        partial_success: false,
    }
}

impl Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        // Compare the key DATA, not the whole `PublicKey`: its `PartialEq`
        // includes the comment field, but a key received over the wire carries no
        // comment while our locally-derived `authorized_key` does, so a full-struct
        // comparison would always reject. The key bytes are what authorize.
        if key.key_data() != self.shared.authorized_key.key_data() {
            tracing::warn!(user, "rejected: client key does not match shared secret");
            return Ok(reject());
        }
        let name = playname::sanitize(user);
        tracing::info!(playname = %name, "client authorized");
        self.playname = Some(name);
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, user: &str, _password: &str) -> Result<Auth, Self::Error> {
        tracing::debug!(user, "password auth rejected: public key required");
        Ok(reject())
    }

    async fn auth_keyboard_interactive(
        &mut self,
        user: &str,
        _submethods: &str,
        _response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth, Self::Error> {
        tracing::debug!(
            user,
            "keyboard-interactive auth rejected: public key required"
        );
        Ok(reject())
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
        _channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.term = term.to_string();
        self.cols = col_width.max(1) as u16;
        self.rows = row_height.max(1) as u16;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = session.channel_success(channel);

        // A playname is recorded only by a successful `auth_publickey`, so its
        // absence means the shell request arrived unauthenticated. russh does
        // not deliver one before auth, so this is an impossible state: fail the
        // session loudly rather than launching the child under a fallback name.
        let Some(playname) = self.playname.clone() else {
            tracing::error!("shell requested before authentication");
            return Err(anyhow::anyhow!("unauthenticated shell request"));
        };

        // Drop the stored Channel handle; we drive the channel by id via the
        // session handle from here on.
        let _ = self.channel.take();

        // This account may already have a live child. Both would hold their own
        // copy of the player's progress and overwrite each other's save, so
        // refuse rather than silently losing one of them. Close without writing
        // to the channel: late-ssh leaves the BashQuest screen on the same tick
        // it sees the close, so anything sent here is parsed into a vt100 screen
        // the client never paints. The log is the only place this is observable.
        let Some(lease) =
            SessionLease::claim(playname.clone(), self.shared.active_playnames.clone())
        else {
            tracing::info!(playname, "rejected: session already active for this playname");
            session.eof(channel)?;
            session.close(channel)?;
            return Ok(());
        };

        self.host = Some(PtyHost::spawn(
            HostConfig {
                bin: self.shared.bin.clone(),
                data_dir: self.shared.data_dir.clone(),
                playname,
                cols: self.cols,
                rows: self.rows,
                term: self.term.clone(),
            },
            session.handle(),
            channel,
            lease,
        ));
        Ok(())
    }

    async fn data(
        &mut self,
        _channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(host) = &self.host {
            host.send_input(data.to_vec());
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(host) = &self.host {
            host.resize(col_width.max(1) as u16, row_height.max(1) as u16);
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Client hung up; dropping the host kills the child. bashquest.sh saves
        // continuously, so this loses at most the current unanswered challenge.
        self.host = None;
        Ok(())
    }

    async fn channel_close(
        &mut self,
        _channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.host = None;
        Ok(())
    }
}

#[cfg(test)]
#[path = "server_test.rs"]
mod server_test;
