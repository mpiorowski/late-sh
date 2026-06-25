use std::sync::Arc;

use anyhow::Result;
use russh::keys::PublicKey;
use russh::server::{Auth, Handler, Msg, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};

use crate::config::Config;
use crate::host::{HostConfig, PtyHost};
use crate::identity::derive_client_key;
use crate::playname;

/// Shared, connection-independent server state.
struct Shared {
    bin: String,
    data_dir: String,
    /// The single client public key we accept (derived from the shared secret).
    authorized_key: PublicKey,
}

#[derive(Clone)]
pub struct Server {
    shared: Arc<Shared>,
}

impl Server {
    pub fn new(config: &Config) -> Self {
        let authorized_key = derive_client_key(&config.secret).public_key().clone();
        Self {
            shared: Arc::new(Shared {
                bin: config.bin.clone(),
                data_dir: config.data_dir.clone(),
                authorized_key,
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

pub struct ClientHandler {
    shared: Arc<Shared>,
    /// Sanitized `-u` playname from the authenticated SSH username.
    playname: Option<String>,
    /// Session channel, set on open and consumed when the shell starts.
    channel: Option<Channel<Msg>>,
    term: String,
    cols: u16,
    rows: u16,
    /// The running NetHack child, once the shell is requested.
    host: Option<PtyHost>,
}

fn reject() -> Auth {
    Auth::Reject {
        proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
        partial_success: false,
    }
}

/// Whether the host has a terminfo entry for `term` (Debian layout:
/// `<dir>/<first-char>/<name>`, plus the hex-dir variant some installs use).
fn term_supported(term: &str) -> bool {
    // Reject anything that isn't a plausible terminfo name; this also blocks path
    // traversal via a hostile TERM before we build a path from it.
    if term.is_empty()
        || !term
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'+'))
    {
        return false;
    }
    let first = &term[..1];
    let hex = format!("{:02x}", term.as_bytes()[0]);
    ["/usr/share/terminfo", "/etc/terminfo", "/lib/terminfo"]
        .iter()
        .any(|dir| {
            let base = std::path::Path::new(dir);
            base.join(first).join(term).exists() || base.join(&hex).join(term).exists()
        })
}

/// The TERM to hand the nethack child: the client's own if the host can resolve
/// its terminfo, otherwise a universal fallback. nethack (ncurses) aborts with
/// "Unknown terminal type" on an unrecognized TERM, but every modern terminal
/// renders `xterm-256color`, so this keeps clients on exotic terminals
/// (kitty/ghostty/wezterm, which ship their own terminfo) playable.
fn effective_term(requested: &str) -> String {
    if term_supported(requested) {
        requested.to_string()
    } else {
        "xterm-256color".to_string()
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

        let Some(playname) = self.playname.clone() else {
            tracing::error!("shell requested without an authenticated playname");
            return Err(anyhow::anyhow!("unauthenticated shell request"));
        };
        // Drop the stored Channel handle; we drive the channel by id via the
        // session handle from here on.
        let _ = self.channel.take();

        // nethack's ncurses aborts on a TERM it has no terminfo for; fall back to
        // a universal one so clients on exotic terminals still play.
        let term = effective_term(&self.term);
        if term != self.term {
            tracing::info!(
                requested = %self.term,
                effective = %term,
                "client TERM has no terminfo on host; using fallback"
            );
        }

        self.host = Some(PtyHost::spawn(
            HostConfig {
                bin: self.shared.bin.clone(),
                data_dir: self.shared.data_dir.clone(),
                playname,
                cols: self.cols,
                rows: self.rows,
                term,
            },
            session.handle(),
            channel,
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
        // Client hung up; dropping the host kills the child.
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
mod tests {
    use super::*;

    #[test]
    fn unknown_term_falls_back_to_xterm_256color() {
        // A terminfo name the host cannot possibly have (and which is charset-valid)
        // must fall back so nethack doesn't abort with "Unknown terminal type".
        assert_eq!(
            effective_term("definitely-not-a-real-term-xyz"),
            "xterm-256color"
        );
    }

    #[test]
    fn hostile_term_is_rejected_and_falls_back() {
        // Path-traversal / junk TERM never reaches the child's argv-env verbatim.
        assert_eq!(effective_term("../../etc/passwd"), "xterm-256color");
        assert_eq!(effective_term(""), "xterm-256color");
    }

    #[test]
    fn supported_term_passes_through() {
        // xterm-256color ships in ncurses-base, so it is present anywhere tests run
        // (and on the host); it must pass through unchanged.
        assert_eq!(effective_term("xterm-256color"), "xterm-256color");
    }
}
