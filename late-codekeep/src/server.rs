use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use russh::keys::PublicKey;
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};

use crate::account;
use crate::config::Config;
use crate::host::{HostConfig, PtyHost, SessionLease};
use crate::identity::derive_client_key;

struct Shared {
    bin: String,
    data_dir: String,
    authorized_key: PublicKey,
    active_accounts: Arc<Mutex<HashSet<String>>>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

#[derive(Clone)]
pub(crate) struct Server {
    shared: Arc<Shared>,
}

impl Server {
    pub(crate) fn new(config: &Config, shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Self {
        Self {
            shared: Arc::new(Shared {
                bin: config.bin.clone(),
                data_dir: config.data_dir.clone(),
                authorized_key: derive_client_key(&config.secret).public_key().clone(),
                active_accounts: Arc::new(Mutex::new(HashSet::new())),
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
            account: None,
            channel: None,
            term: "xterm-256color".to_string(),
            cols: 108,
            rows: 24,
            host: None,
        }
    }
}

pub(crate) struct ClientHandler {
    shared: Arc<Shared>,
    account: Option<String>,
    channel: Option<Channel<Msg>>,
    term: String,
    cols: u16,
    rows: u16,
    host: Option<PtyHost>,
}

fn reject() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
        partial_success: false,
    }
}

fn effective_term(requested: &str) -> String {
    if !requested.is_empty()
        && requested
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+'))
    {
        requested.to_string()
    } else {
        "xterm-256color".to_string()
    }
}

impl Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        if key.key_data() != self.shared.authorized_key.key_data() {
            tracing::warn!(user, "rejected: client key does not match shared secret");
            return Ok(reject());
        }
        let Some(account) = account::sanitize(user) else {
            tracing::warn!(user, "rejected: invalid CodeKeep account label");
            return Ok(reject());
        };
        tracing::info!(account, "client authorized");
        self.account = Some(account);
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(reject())
    }

    async fn auth_keyboard_interactive(
        &mut self,
        _user: &str,
        _submethods: &str,
        _response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth, Self::Error> {
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
        let Some(account) = self.account.clone() else {
            return Err(anyhow::anyhow!("unauthenticated shell request"));
        };
        let _ = self.channel.take();

        let lease = {
            let mut active = self
                .shared
                .active_accounts
                .lock()
                .expect("active accounts mutex");
            if !active.insert(account.clone()) {
                // The previous session still owns this account, usually because
                // its child is inside the SIGHUP-save grace. Close without
                // writing to the channel: late-ssh leaves the CodeKeep screen on
                // the same tick it sees the close, so anything sent here is
                // parsed into a vt100 screen the client never paints. The log is
                // the only place this is observable.
                tracing::info!(account, "rejected: session already active for this account");
                session.eof(channel)?;
                session.close(channel)?;
                return Ok(());
            }
            SessionLease::new(account.clone(), self.shared.active_accounts.clone())
        };

        self.host = Some(PtyHost::spawn(
            HostConfig {
                bin: self.shared.bin.clone(),
                data_dir: self.shared.data_dir.clone(),
                account,
                cols: self.cols,
                rows: self.rows,
                term: effective_term(&self.term),
            },
            session.handle(),
            channel,
            self.shared.shutdown_rx.clone(),
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
