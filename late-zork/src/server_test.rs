use super::*;

use russh::client;
use russh::{ChannelMsg, Disconnect};
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;
use tokio::time::timeout;

struct Client;
impl client::Handler for Client {
    type Error = russh::Error;
    async fn check_server_key(&mut self, _: &PublicKey) -> std::result::Result<bool, Self::Error> {
        Ok(true)
    }
}

struct Fixture {
    root: tempfile::TempDir,
    port: u16,
    server: Server,
    shutdown: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        use russh::server::Server as _;
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("interpreter");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
case "$LATE_FROTZ_DOOR" in
inspect-*) printf '{"status":"missing"}'; exit 0;;
esac
trap 'printf stopped > stopped; exit 20' USR1
trap 'printf saved > stopped; exit 0' HUP TERM
printf 'READY %s %s secret=%s\n' "$HOME" "$LATE_FROTZ_DOOR" "$LATE_ZORK_SECRET"
printf 'TERM %s\n' "$TERM"
while IFS= read -r line; do
  case "$line" in
  quit) exit 0;;
  *) printf 'REPLY %s\n' "$line";;
  esac
done
"#,
        )
        .unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (shutdown, rx) = tokio::sync::watch::channel(false);
        let server = Server::new(
            &Config {
                bin: bin.to_string_lossy().into(),
                story_dir: root.path().to_string_lossy().into(),
                data_dir: root.path().join("data").to_string_lossy().into(),
                secret: "test-secret".into(),
                listen_addr: "127.0.0.1".into(),
                port: 0,
                idle_timeout: 60,
            },
            rx,
        );
        let key = crate::identity::derive_client_key("host-key");
        let config = Arc::new(russh::server::Config {
            keys: vec![key],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut accept_server = server.clone();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let handler = accept_server.new_client(None);
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(session) = russh::server::run_stream(config, stream, handler).await {
                        let _ = session.await;
                    }
                });
            }
        });
        Self {
            root,
            port,
            server,
            shutdown,
            task,
        }
    }
    async fn client(&self, user: &str, secret: &str) -> (client::Handle<Client>, bool) {
        let mut client = client::connect(
            Arc::new(client::Config::default()),
            ("127.0.0.1", self.port),
            Client,
        )
        .await
        .unwrap();
        let accepted = client
            .authenticate_publickey(
                user,
                russh::keys::PrivateKeyWithHashAlg::new(Arc::new(derive_client_key(secret)), None),
            )
            .await
            .unwrap()
            .success();
        (client, accepted)
    }
    fn active(&self) -> usize {
        self.server.shared.active_accounts.lock().unwrap().len()
    }
}
const ACCOUNT: &str = "late_000000000000000000000001";
const OTHER: &str = "late_000000000000000000000002";
async fn request(client: &client::Handle<Client>, command: &str) -> Channel<client::Msg> {
    request_with_term(client, command, "xterm-256color").await
}
async fn request_with_term(
    client: &client::Handle<Client>,
    command: &str,
    term: &str,
) -> Channel<client::Msg> {
    let channel = client.channel_open_session().await.unwrap();
    channel
        .request_pty(true, term, 80, 24, 0, 0, &[])
        .await
        .unwrap();
    channel.exec(true, command).await.unwrap();
    channel
}
async fn until(channel: &mut Channel<client::Msg>, text: &str) -> String {
    timeout(Duration::from_secs(3), async {
        let mut output = String::new();
        while let Some(msg) = channel.wait().await {
            if let ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } = msg {
                output.push_str(&String::from_utf8_lossy(&data));
                if output.contains(text) {
                    return output;
                }
            }
        }
        panic!("channel closed before {text}: {output}");
    })
    .await
    .unwrap()
}
async fn exit(channel: &mut Channel<client::Msg>) -> (u32, String) {
    timeout(Duration::from_secs(7), async {
        let mut output = String::new();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::ExitStatus { exit_status } => return (exit_status, output),
                ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                    output.push_str(&String::from_utf8_lossy(&data))
                }
                _ => {}
            }
        }
        panic!("no exit status: {output}");
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn client_terminal_names_do_not_reach_the_embedded_interpreter() {
    let fixture = Fixture::new().await;
    let (client, accepted) = fixture.client(ACCOUNT, "test-secret").await;
    assert!(accepted);
    for term in ["xterm-kitty", "xterm-ghostty", "../../etc/passwd", ""] {
        let mut game = request_with_term(&client, "play zork1 new", term).await;
        let output = until(&mut game, "TERM xterm-256color\r\n").await;
        assert!(output.contains("TERM xterm-256color\r\n"), "{output}");
        game.data(&b"quit\n"[..]).await.unwrap();
        assert_eq!(exit(&mut game).await.0, 0);
    }
}

#[tokio::test]
async fn rejects_wrong_keys_and_unsafe_accounts() {
    let fixture = Fixture::new().await;
    assert!(!fixture.client(ACCOUNT, "wrong-secret").await.1);
    assert!(!fixture.client("../../escape", "test-secret").await.1);
    assert_eq!(fixture.active(), 0);
}

#[tokio::test]
async fn account_lease_spans_editions_and_metadata_does_not_stop_game() {
    let fixture = Fixture::new().await;
    let (client, accepted) = fixture.client(ACCOUNT, "test-secret").await;
    assert!(accepted);
    let mut game = request(&client, "play zork1 new").await;
    let ready = until(&mut game, "READY").await;
    assert!(ready.contains("/zork1 new secret=\r\n"), "{ready}");
    let mut list = request(&client, "list").await;
    let (code, json) = exit(&mut list).await;
    assert_eq!(code, 0);
    let catalogue: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(catalogue["active"], "zork1");
    assert_eq!(catalogue["editions"].as_array().unwrap().len(), 3);
    let mut busy = request(&client, "play zork2 new").await;
    assert_eq!(exit(&mut busy).await.0, 75);
    game.data(&b"look\n"[..]).await.unwrap();
    until(&mut game, "REPLY look").await;
    game.signal(Sig::USR1).await.unwrap();
    assert_eq!(exit(&mut game).await.0, 20);
    assert_eq!(fixture.active(), 0);
    let mut second = request(&client, "play zork2 auto").await;
    until(&mut second, "READY").await;
    second.data(&b"quit\n"[..]).await.unwrap();
    assert_eq!(exit(&mut second).await.0, 0);
    assert_eq!(fixture.active(), 0);
}

#[tokio::test]
async fn separate_accounts_run_together_and_disconnect_flushes_before_unlock() {
    let fixture = Fixture::new().await;
    let (first, _) = fixture.client(ACCOUNT, "test-secret").await;
    let (second, _) = fixture.client(OTHER, "test-secret").await;
    let mut one = request(&first, "play zork1 new").await;
    let mut two = request(&second, "play zork3 new").await;
    until(&mut one, "READY").await;
    until(&mut two, "READY").await;
    assert_eq!(fixture.active(), 2);
    first
        .disconnect(Disconnect::ByApplication, "", "en")
        .await
        .unwrap();
    drop(first);
    timeout(Duration::from_secs(7), async {
        while fixture.active() != 1 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(
            fixture
                .root
                .path()
                .join("data")
                .join(ACCOUNT)
                .join("zork1/stopped")
        )
        .unwrap(),
        "saved"
    );
    two.data(&b"quit\n"[..]).await.unwrap();
    assert_eq!(exit(&mut two).await.0, 0);
}

#[tokio::test]
async fn invalid_requests_and_spawn_failure_release_account() {
    let fixture = Fixture::new().await;
    let (client, _) = fixture.client(ACCOUNT, "test-secret").await;
    let mut invalid = request(&client, "play ../../escape new").await;
    assert_eq!(exit(&mut invalid).await.0, 64);
    std::fs::remove_file(fixture.root.path().join("interpreter")).unwrap();
    let mut failed = request(&client, "play zork1 new").await;
    let (code, message) = exit(&mut failed).await;
    assert_eq!(code, 1);
    assert!(message.contains("Unable to start Zork"));
    assert_eq!(fixture.active(), 0);
}
