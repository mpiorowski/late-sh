//! Integration test for `RebelsProxy` against a minimal stub rebels SSH server.
//!
//! The stub server accepts any publickey auth (the proxy authenticates with its
//! derived Ed25519 key), grants a PTY + shell, and writes recognizable bytes
//! back to the channel so the client's vt100 parser renders them. Mirrors the
//! russh server patterns in `late-ssh/src/ssh.rs` and the client/test patterns
//! in `late-ssh/tests/ssh_smoke.rs`.

use std::sync::Arc;
use std::time::Duration;

use getrandom::SysRng;
use late_ssh::app::door::rebels::proxy::{ProxyConfig, ProxyStatus, RebelsProxy};
use russh::keys::signature::rand_core::UnwrapErr;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::net::TcpListener;
use tokio::time::{Instant, sleep, timeout};
use uuid::Uuid;

/// Bytes the stub server writes back: clear-screen, home cursor, then "HELLO".
const REPLY: &[u8] = b"\x1b[2J\x1b[HHELLO";

#[derive(Clone)]
struct StubServer {
    /// When true, the server closes the channel right after the shell request
    /// instead of streaming data, exercising the Closed-status path.
    close_after_shell: bool,
}

struct StubHandler {
    close_after_shell: bool,
    channel: Option<Channel<Msg>>,
}

impl Server for StubServer {
    type Handler = StubHandler;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> StubHandler {
        StubHandler {
            close_after_shell: self.close_after_shell,
            channel: None,
        }
    }
}

impl Handler for StubHandler {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        _key: &russh::keys::PublicKey,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Reject {
            proceed_with_methods: Some(MethodSet::from(&[MethodKind::PublicKey][..])),
            partial_success: false,
        })
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
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        if self.close_after_shell {
            // Drop the channel so the client sees Eof/Close and goes Closed.
            session.eof(channel)?;
            session.close(channel)?;
        } else {
            // Stream recognizable bytes so the client's vt100 parser renders them.
            session.data(channel, REPLY.to_vec())?;
        }
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        _data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if !self.close_after_shell {
            session.data(channel, REPLY.to_vec())?;
        }
        Ok(())
    }
}

/// Bind a stub server on an ephemeral 127.0.0.1 port and return the bound port.
/// The server task is detached; it lives for the duration of the test process.
async fn spawn_stub_server(close_after_shell: bool) -> u16 {
    let key = PrivateKey::random(&mut UnwrapErr(SysRng), Algorithm::Ed25519)
        .expect("generate stub host key");
    let config = Arc::new(Config {
        inactivity_timeout: Some(Duration::from_secs(60)),
        auth_rejection_time: Duration::from_millis(1),
        keys: vec![key],
        ..Default::default()
    });

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind stub");
    let port = listener.local_addr().expect("local addr").port();

    let mut server = StubServer { close_after_shell };
    tokio::spawn(async move {
        loop {
            let Ok((stream, _addr)) = listener.accept().await else {
                break;
            };
            let config = Arc::clone(&config);
            let handler = server.new_client(None);
            tokio::spawn(async move {
                if let Ok(session) = russh::server::run_stream(config, stream, handler).await {
                    let _ = session.await;
                }
            });
        }
    });

    port
}

fn proxy_config(port: u16) -> ProxyConfig {
    ProxyConfig {
        host: "127.0.0.1".to_string(),
        port,
        secret: "integration-test-secret".to_string(),
        user_id: Uuid::from_u128(0xABCD),
        cols: 20,
        rows: 5,
        term: "xterm-256color".to_string(),
        repaint: None,
    }
}

/// Poll `predicate` every 30ms until it returns true or the deadline elapses.
async fn wait_until(label: &str, mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        sleep(Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for: {label}");
}

#[tokio::test]
async fn proxy_renders_remote_output() {
    let port = spawn_stub_server(false).await;
    let proxy = RebelsProxy::connect(proxy_config(port));

    timeout(
        Duration::from_secs(3),
        wait_until("proxy to reach Running", || {
            proxy.status() == ProxyStatus::Running
        }),
    )
    .await
    .expect("proxy did not reach Running in time");

    proxy.send_input(b"x".to_vec());

    timeout(
        Duration::from_secs(3),
        wait_until("HELLO to land in screen row 0", || {
            proxy.with_screen(|screen| row_text(screen, 0, 20).contains("HELLO"))
        }),
    )
    .await
    .expect("HELLO never appeared in proxy screen");

    assert!(proxy.is_running(), "proxy should still be running");
}

#[tokio::test]
async fn proxy_reports_closed_when_server_disconnects() {
    let port = spawn_stub_server(true).await;
    let proxy = RebelsProxy::connect(proxy_config(port));

    timeout(
        Duration::from_secs(3),
        wait_until("proxy to reach Closed after server disconnect", || {
            proxy.status() == ProxyStatus::Closed
        }),
    )
    .await
    .expect("proxy never reported Closed");

    assert_eq!(proxy.status(), ProxyStatus::Closed);
    assert!(!proxy.is_running());
}

/// Read the visible text of `row` across `cols` columns from a vt100 screen.
fn row_text(screen: &vt100::Screen, row: u16, cols: u16) -> String {
    (0..cols)
        .filter_map(|col| screen.cell(row, col))
        .map(|cell| cell.contents())
        .collect()
}
