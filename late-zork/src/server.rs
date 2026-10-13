use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use russh::keys::PublicKey;
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet, Sig};

use crate::account;
use crate::config::Config;
use crate::host::{HostConfig, PtyHost, SessionLease};
use crate::identity::derive_client_key;
use crate::protocol::{Edition, Request, parse_request};

struct Shared {
    bin: String,
    story_dir: String,
    data_dir: String,
    authorized_key: PublicKey,
    active_accounts: Arc<Mutex<HashMap<String, Edition>>>,
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
                story_dir: config.story_dir.clone(),
                data_dir: config.data_dir.clone(),
                authorized_key: derive_client_key(&config.secret).public_key().clone(),
                active_accounts: Arc::new(Mutex::new(HashMap::new())),
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
            channels: HashMap::new(),
            terminals: HashMap::new(),
            host: None,
        }
    }
}

#[derive(Clone)]
struct Terminal {
    cols: u16,
    rows: u16,
}
impl Default for Terminal {
    fn default() -> Self {
        Self {
            cols: 108,
            rows: 24,
        }
    }
}

pub(crate) struct ClientHandler {
    shared: Arc<Shared>,
    account: Option<String>,
    channels: HashMap<ChannelId, Channel<Msg>>,
    terminals: HashMap<ChannelId, Terminal>,
    host: Option<(ChannelId, PtyHost)>,
}

fn reject() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
        partial_success: false,
    }
}

fn channel_error(
    session: &mut Session,
    channel: ChannelId,
    code: u32,
    message: &str,
) -> Result<()> {
    session.channel_success(channel)?;
    session.extended_data(channel, 1, message.as_bytes().to_vec())?;
    session.exit_status_request(channel, code)?;
    session.eof(channel)?;
    session.close(channel)?;
    Ok(())
}

impl Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(&mut self, user: &str, key: &PublicKey) -> Result<Auth> {
        if key.key_data() != self.shared.authorized_key.key_data() {
            return Ok(reject());
        }
        let Some(account) = account::sanitize(user) else {
            return Ok(reject());
        };
        self.account = Some(account);
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth> {
        Ok(reject())
    }
    async fn auth_keyboard_interactive(
        &mut self,
        _user: &str,
        _submethods: &str,
        _response: Option<russh::server::Response<'_>>,
    ) -> Result<Auth> {
        Ok(reject())
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool> {
        if self.channels.len() >= 4 || self.account.is_none() {
            return Ok(false);
        }
        self.channels.insert(channel.id(), channel);
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<()> {
        self.terminals.insert(
            channel,
            Terminal {
                cols: cols.clamp(20, 255) as u16,
                rows: rows.clamp(4, 255) as u16,
            },
        );
        session.channel_success(channel)?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut Session,
    ) -> Result<()> {
        let Some(account) = self.account.clone() else {
            anyhow::bail!("unauthenticated exec");
        };
        if self.channels.remove(&channel).is_none() {
            return channel_error(
                session,
                channel,
                64,
                "This channel already has a request.\n",
            );
        }
        if *self.shared.shutdown_rx.borrow() {
            return channel_error(
                session,
                channel,
                75,
                "Zork is shutting down. Please reconnect shortly.\n",
            );
        }
        match parse_request(command) {
            Some(Request::List) => {
                session.channel_success(channel)?;
                let shared = self.shared.clone();
                let handle = session.handle();
                tokio::spawn(async move {
                    let active = shared
                        .active_accounts
                        .lock()
                        .expect("active accounts mutex")
                        .get(&account)
                        .copied();
                    let catalogue = crate::slots::list(
                        &shared.bin,
                        &shared.story_dir,
                        &shared.data_dir,
                        &account,
                        active,
                    )
                    .await;
                    if let Ok(data) = serde_json::to_vec(&catalogue) {
                        let _ = handle.data(channel, data).await;
                    }
                    let _ = handle.exit_status_request(channel, 0).await;
                    let _ = handle.eof(channel).await;
                    let _ = handle.close(channel).await;
                });
            }
            Some(Request::Play(edition, action)) => {
                if self
                    .host
                    .as_ref()
                    .is_some_and(|(_, host)| host.is_finished())
                {
                    self.host = None;
                }
                let Some(lease) = SessionLease::acquire(
                    account.clone(),
                    edition,
                    self.shared.active_accounts.clone(),
                ) else {
                    return channel_error(
                        session,
                        channel,
                        75,
                        "Zork is already active for this account. Return from the other session first.\n",
                    );
                };
                let terminal = self.terminals.remove(&channel).unwrap_or_default();
                session.channel_success(channel)?;
                self.host = Some((
                    channel,
                    PtyHost::spawn(
                        HostConfig {
                            bin: self.shared.bin.clone(),
                            story_dir: self.shared.story_dir.clone(),
                            data_dir: self.shared.data_dir.clone(),
                            account,
                            edition,
                            action,
                            cols: terminal.cols,
                            rows: terminal.rows,
                        },
                        session.handle(),
                        channel,
                        self.shared.shutdown_rx.clone(),
                        lease,
                    ),
                ));
            }
            None => return channel_error(session, channel, 64, "Unsupported Zork request.\n"),
        }
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, session: &mut Session) -> Result<()> {
        session.channel_failure(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<()> {
        if let Some((id, host)) = &self.host
            && *id == channel
            && data.len() <= 8192
        {
            host.send_input(data.to_vec());
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        cols: u32,
        rows: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<()> {
        if let Some((id, host)) = &self.host
            && *id == channel
        {
            host.resize(cols.clamp(20, 255) as u16, rows.clamp(4, 255) as u16);
        }
        Ok(())
    }

    async fn signal(
        &mut self,
        channel: ChannelId,
        signal: Sig,
        _session: &mut Session,
    ) -> Result<()> {
        if matches!(signal, Sig::USR1)
            && let Some((id, host)) = &self.host
            && *id == channel
        {
            host.return_to_menu();
        }
        Ok(())
    }

    async fn channel_eof(&mut self, channel: ChannelId, session: &mut Session) -> Result<()> {
        self.channel_close(channel, session).await
    }

    async fn channel_close(&mut self, channel: ChannelId, _session: &mut Session) -> Result<()> {
        self.channels.remove(&channel);
        self.terminals.remove(&channel);
        if self.host.as_ref().is_some_and(|(id, _)| *id == channel) {
            self.host = None;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "server_test.rs"]
mod server_test;
