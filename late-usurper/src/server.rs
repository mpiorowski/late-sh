use std::sync::Arc;

use anyhow::Result;
use russh::keys::PublicKey;
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};

use crate::config::Config;
use crate::dropfile;
use crate::host::{HostConfig, PtyHost};
use crate::identity::derive_client_key;
use crate::nodes::Nodes;
use crate::playname;

/// Shared, connection-independent server state.
struct Shared {
    bin: String,
    game_dir: String,
    /// The single client public key we accept (derived from the shared secret).
    authorized_key: PublicKey,
    /// Node-number leases; one per live session, capped at the config's max.
    nodes: Nodes,
    /// Flips to `true` on host SIGTERM/SIGINT; each live `PtyHost` watches it
    /// and tears its child down so a pod shutdown doesn't strand sessions.
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

#[derive(Clone)]
pub struct Server {
    shared: Arc<Shared>,
}

impl Server {
    pub fn new(config: &Config, shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Self {
        let authorized_key = derive_client_key(&config.secret).public_key().clone();
        Self {
            shared: Arc::new(Shared {
                bin: config.bin.clone(),
                game_dir: config.game_dir.clone(),
                authorized_key,
                nodes: Nodes::new(config.max_nodes),
                shutdown_rx,
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
            cols: 80,
            rows: 25,
            host: None,
        }
    }
}

pub struct ClientHandler {
    shared: Arc<Shared>,
    /// Sanitized player name from the authenticated SSH username; becomes the
    /// DOOR32.SYS identity.
    playname: Option<String>,
    /// Session channel, set on open and consumed when the shell starts.
    channel: Option<Channel<Msg>>,
    cols: u16,
    rows: u16,
    /// The running game child, once the shell is requested.
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
        // includes the comment field, but a key received over the wire carries
        // no comment while our locally-derived `authorized_key` does, so a
        // full-struct comparison would always reject. The key bytes are what
        // authorize.
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
        _term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // The client TERM is deliberately ignored: the child always runs with
        // TERM=xterm because its output feeds late-ssh's vt100 parser, not the
        // player's terminal (see host.rs).
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

        let Some(playname) = self.playname.clone() else {
            tracing::error!("shell requested without an authenticated playname");
            return Err(anyhow::anyhow!("unauthenticated shell request"));
        };
        // Drop the stored Channel handle; we drive the channel by id via the
        // session handle from here on.
        let _ = self.channel.take();
        let handle = session.handle();

        // One node lease per live session. When the pool is dry, say so and
        // close; the client treats it like any other exit and returns to the
        // Games hub.
        let Some(node) = self.shared.nodes.acquire() else {
            tracing::warn!(playname = %playname, "no free usurper node; turning session away");
            let handle = handle.clone();
            tokio::spawn(async move {
                let _ = handle
                    .data(
                        channel,
                        b"\r\nAll of Usurper's nodes are busy right now - try again in a moment.\r\n"
                            .to_vec(),
                    )
                    .await;
                let _ = handle.eof(channel).await;
                let _ = handle.close(channel).await;
            });
            return Ok(());
        };

        // Fresh dropfile for this session: node + identity.
        let drop_rel = match dropfile::write_door32(&self.shared.game_dir, node.number(), &playname)
        {
            Ok(rel) => rel,
            Err(e) => {
                tracing::error!(error = ?e, "failed to write usurper dropfile");
                let handle = handle.clone();
                tokio::spawn(async move {
                    let _ = handle.eof(channel).await;
                    let _ = handle.close(channel).await;
                });
                return Ok(());
            }
        };

        tracing::info!(playname = %playname, node = node.number(), "starting usurper session");
        self.host = Some(PtyHost::spawn(
            HostConfig {
                bin: self.shared.bin.clone(),
                game_dir: self.shared.game_dir.clone(),
                drop_rel,
                node,
                cols: self.cols,
                rows: self.rows,
            },
            handle,
            channel,
            self.shared.shutdown_rx.clone(),
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
        // Client hung up; dropping the host tears the bridge down.
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
