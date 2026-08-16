//! Listener smoke test for the stats stream against a stub door host,
//! patterned on `door/rebels/proxy_test.rs`: a minimal russh server that
//! accepts publickey auth, records the cursors env request, and streams
//! framed lines on shell. Covers the client's handshake order (env before
//! shell), frame reassembly across chunk boundaries, and clean end-of-stream.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use getrandom::SysRng;
use russh::keys::signature::rand_core::UnwrapErr;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, Config, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, MethodKind, MethodSet};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::stream::{CURSORS_ENV_VAR, StatsFrame, StreamConfig, run_stats_stream};

#[derive(Clone)]
struct StubServer {
    seen_cursors: Arc<Mutex<Option<String>>>,
}

struct StubHandler {
    seen_cursors: Arc<Mutex<Option<String>>>,
    channel: Option<Channel<Msg>>,
}

impl Server for StubServer {
    type Handler = StubHandler;

    fn new_client(&mut self, _peer: Option<std::net::SocketAddr>) -> StubHandler {
        StubHandler {
            seen_cursors: self.seen_cursors.clone(),
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

    async fn env_request(
        &mut self,
        _channel: ChannelId,
        name: &str,
        value: &str,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name == CURSORS_ENV_VAR {
            *self.seen_cursors.lock().expect("cursors mutex") = Some(value.to_string());
        }
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        // Two frames split mid-line across writes, then a clean close.
        session.data(channel, b"logfile\t120\tname=A:sc=1\nmilest".to_vec())?;
        session.data(channel, b"ones\t45\tname=B:type=orb\n".to_vec())?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }
}

async fn spawn_stub(seen_cursors: Arc<Mutex<Option<String>>>) -> u16 {
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
    let mut server = StubServer { seen_cursors };
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

#[tokio::test]
async fn streams_frames_and_pushes_cursors() {
    let seen_cursors = Arc::new(Mutex::new(None));
    let port = spawn_stub(seen_cursors.clone()).await;

    let (tx, mut rx) = mpsc::channel::<StatsFrame>(16);
    let cursors = std::collections::HashMap::from([("logfile".to_string(), 120i64)]);
    let stream = tokio::spawn(run_stats_stream(
        StreamConfig {
            host: "127.0.0.1".to_string(),
            port,
            key: crate::app::door::dcss::identity::derive_client_key("smoke-secret"),
            cursors,
        },
        tx,
    ));

    let first = timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("first frame in time")
        .expect("first frame");
    assert_eq!(
        first,
        StatsFrame {
            file: "logfile".to_string(),
            next_offset: 120,
            line: "name=A:sc=1".to_string(),
        }
    );
    let second = timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("second frame in time")
        .expect("second frame");
    assert_eq!(second.file, "milestones");
    assert_eq!(second.next_offset, 45);

    // Server closed after the second frame: the stream ends cleanly.
    assert!(
        timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("stream end in time")
            .is_none()
    );
    timeout(Duration::from_secs(3), stream)
        .await
        .expect("stream task end in time")
        .expect("stream task join")
        .expect("stream result");

    assert_eq!(
        seen_cursors.lock().expect("cursors mutex").as_deref(),
        Some("logfile:120")
    );
}
