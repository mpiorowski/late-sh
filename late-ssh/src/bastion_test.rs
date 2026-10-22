//! The real SSH bastion and real database-backed tunnel, with a controllable
//! TCP hop between them. No mocked application or WebSocket protocol.
use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use futures_util::{SinkExt, StreamExt};
use getrandom::SysRng;
use late_core::{
    MutexRecover,
    models::{
        server_ban::{ServerBan, ServerBanActivation},
        user::User,
    },
    shutdown::CancellationToken,
    test_utils::{TestDb, create_test_user},
};
use russh::{
    Channel, ChannelMsg, client,
    keys::{PrivateKey, PrivateKeyWithHashAlg, signature::rand_core::UnwrapErr},
};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
    sync::Mutex,
    task::{JoinHandle, JoinSet},
    time::timeout,
};

use crate::{
    authz::Permissions,
    state::State,
    test_helpers::{new_test_db, test_app_state, test_config, wait_until},
};

const WAIT: Duration = Duration::from_secs(10);

struct AnyHostKey;
impl client::Handler for AnyHostKey {
    type Error = russh::Error;
    async fn check_server_key(&mut self, _: &russh::keys::PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

struct Backend {
    state: State,
    addr: SocketAddr,
    cancel: CancellationToken,
    task: JoinHandle<anyhow::Result<()>>,
}

impl Backend {
    async fn spawn(db: &TestDb) -> Self {
        let mut config = test_config(db.db.config().clone());
        config.tunnel_trusted_cidrs = vec!["127.0.0.0/8".parse().unwrap()];
        let state = test_app_state(db.db.clone(), config);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let cancel = CancellationToken::new();
        let task = tokio::spawn(crate::tunnel::run_tunnel_server_with_listener(
            listener,
            state.clone(),
            Some(cancel.clone()),
        ));
        Self {
            state,
            addr,
            cancel,
            task,
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.task.abort();
    }
}

struct Hop {
    route: Arc<Mutex<(SocketAddr, CancellationToken)>>,
    addr: SocketAddr,
    task: JoinHandle<()>,
}

impl Hop {
    async fn spawn(target: SocketAddr) -> Self {
        let route = Arc::new(Mutex::new((target, CancellationToken::new())));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = route.clone();
        let task = tokio::spawn(async move {
            let mut clients = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut inbound, _) = accepted.unwrap();
                        let (target, cancel) = routes.lock().await.clone();
                        clients.spawn(async move {
                            let Ok(mut outbound) = TcpStream::connect(target).await else { return; };
                            tokio::select! {
                                _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound) => {},
                                _ = cancel.cancelled() => {},
                            }
                        });
                    }
                    _ = clients.join_next(), if !clients.is_empty() => {},
                }
            }
        });
        Self { route, addr, task }
    }

    async fn switch(&self, target: SocketAddr, drop_existing: bool) {
        let mut route = self.route.lock().await;
        if drop_existing {
            route.1.cancel();
        }
        *route = (target, CancellationToken::new());
    }
}

impl Drop for Hop {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Stack {
    db: TestDb,
    backend: Backend,
    hop: Hop,
    addr: SocketAddr,
    cancel: CancellationToken,
    bastion: JoinHandle<anyhow::Result<()>>,
    key_path: PathBuf,
}

impl Stack {
    async fn spawn() -> Self {
        let db = new_test_db().await;
        let backend = Backend::spawn(&db).await;
        let hop = Hop::spawn(backend.addr).await;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let key_path = std::env::temp_dir().join(format!("bastion-chain-{}", uuid::Uuid::now_v7()));
        let config = Arc::new(late_bastion::config::Config {
            ssh_port: 0,
            host_key_path: key_path.clone(),
            ssh_idle_timeout: 60,
            backend_tunnel_url: format!("ws://{}/tunnel", hop.addr),
            backend_shared_secret: "test-secret".into(),
            max_conns_global: 10,
            max_conns_per_ip: 10,
            max_attempts_per_ip: 1000,
            rate_limit_window_secs: 60,
            proxy_protocol: true,
            proxy_trusted_cidrs: vec!["127.0.0.0/8".parse().unwrap()],
        });
        let cancel = CancellationToken::new();
        let bastion = tokio::spawn(late_bastion::ssh::run_with_listener(
            listener,
            config,
            cancel.clone(),
        ));
        Self {
            db,
            backend,
            hop,
            addr,
            cancel,
            bastion,
            key_path,
        }
    }

    async fn connect(&self, source: &str, key: Arc<PrivateKey>) -> client::Handle<AnyHostKey> {
        let source: IpAddr = source.parse().unwrap();
        let (family, destination) = if source.is_ipv4() {
            ("TCP4", "127.0.0.1")
        } else {
            ("TCP6", "::1")
        };
        let mut tcp = TcpStream::connect(self.addr).await.unwrap();
        tcp.write_all(
            format!(
                "PROXY {family} {source} {destination} 12345 {}\r\n",
                self.addr.port()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
        let mut session = timeout(
            WAIT,
            client::connect_stream(Arc::new(client::Config::default()), tcp, AnyHostKey),
        )
        .await
        .unwrap()
        .unwrap();
        assert!(
            session
                .authenticate_publickey(
                    "untrusted-login-hint",
                    PrivateKeyWithHashAlg::new(key, None)
                )
                .await
                .unwrap()
                .success()
        );
        session
    }

    async fn cleanup(&self) {
        timeout(WAIT, self.backend.state.tunnel_sessions.wait_empty())
            .await
            .expect("sessions leaked");
        assert!(self.backend.state.active_users.lock_recover().is_empty());
        assert!(self.backend.state.conn_counts.lock_recover().is_empty());
        assert_eq!(self.backend.state.conn_limit.available_permits(), 100);
    }
}

impl Drop for Stack {
    fn drop(&mut self) {
        self.cancel.cancel();
        self.bastion.abort();
        let _ = std::fs::remove_file(&self.key_path);
    }
}

fn key() -> Arc<PrivateKey> {
    Arc::new(PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519).unwrap())
}

async fn exec_token(session: &client::Handle<AnyHostKey>) -> String {
    let (stdout, stderr, status) = exec_output(session).await;
    assert_eq!(
        status,
        0,
        "exec failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    serde_json::from_slice::<serde_json::Value>(&stdout).unwrap()["session_token"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn exec_output(session: &client::Handle<AnyHostKey>) -> (Vec<u8>, Vec<u8>, u32) {
    let mut channel = session.channel_open_session().await.unwrap();
    channel.exec(true, "late-cli-token-v1").await.unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut status = None;
    timeout(WAIT, async {
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, .. } => stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => status = Some(exit_status),
                _ => {}
            }
        }
    })
    .await
    .expect("exec did not complete");
    (stdout, stderr, status.expect("exec exit status"))
}

async fn shell(session: &client::Handle<AnyHostKey>) -> Channel<client::Msg> {
    let channel = session.channel_open_session().await.unwrap();
    channel
        .request_pty(true, "xterm-256color", 120, 40, 0, 0, &[])
        .await
        .unwrap();
    channel.request_shell(true).await.unwrap();
    channel
}

async fn until(channel: &mut Channel<client::Msg>, expected: &str) -> String {
    timeout(WAIT, async {
        let mut parser = vt100::Parser::new(40, 120, 0);
        loop {
            match channel.wait().await.expect("SSH channel ended") {
                ChannelMsg::Data { data } => {
                    parser.process(&data);
                    let screen = parser.screen().contents();
                    if screen.contains(expected) {
                        return screen;
                    }
                }
                ChannelMsg::Close | ChannelMsg::Eof => panic!("SSH ended before {expected}"),
                _ => {}
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no terminal frame containing {expected}"))
}

#[tokio::test]
async fn bastion_chain_exec_shell_identity_and_clean_disconnect() {
    let stack = Stack::spawn().await;
    let key = key();
    let fingerprint = key
        .public_key()
        .fingerprint(russh::keys::HashAlg::Sha256)
        .to_string();
    let client = stack.connect("198.18.0.1", key).await;
    let token = exec_token(&client).await;
    assert!(
        stack
            .backend
            .state
            .session_registry
            .has_session(&token)
            .await
    );
    let mut channel = shell(&client).await;
    until(&mut channel, "late").await;
    {
        let users = stack.backend.state.active_users.lock_recover();
        assert_eq!(users.len(), 1);
        let user = users.values().next().unwrap();
        assert_eq!(user.fingerprint.as_deref(), Some(fingerprint.as_str()));
        assert_ne!(user.username, "untrusted-login-hint");
        assert_eq!(user.sessions[0].peer_ip.unwrap().to_string(), "198.18.0.1");
        assert_eq!(user.sessions[0].token, token);
    }
    channel.window_change(0, u32::MAX, 0, 0).await.unwrap();
    client
        .disconnect(russh::Disconnect::ByApplication, "test done", "")
        .await
        .unwrap();
    drop(channel);
    drop(client);
    stack.cleanup().await;
    assert!(
        !stack
            .backend
            .state
            .session_registry
            .has_session(&token)
            .await
    );
}

#[tokio::test]
async fn bastion_chain_exec_only_disconnect_does_not_leak() {
    let stack = Stack::spawn().await;
    let client = stack.connect("198.18.0.2", key()).await;
    let token = exec_token(&client).await;
    client
        .disconnect(russh::Disconnect::ByApplication, "exec only", "")
        .await
        .unwrap();
    drop(client);
    stack.cleanup().await;
    assert!(
        !stack
            .backend
            .state
            .session_registry
            .has_session(&token)
            .await
    );
}

#[tokio::test]
async fn bastion_chain_transport_loss_preserves_ssh_identity_and_pairing() {
    let stack = Stack::spawn().await;
    let client = stack.connect("198.18.0.3", key()).await;
    let token = exec_token(&client).await;
    let mut channel = shell(&client).await;
    until(&mut channel, "late").await;
    let user_id = stack
        .backend
        .state
        .session_registry
        .user_for(&token)
        .await
        .unwrap();
    let next = Backend::spawn(&stack.db).await;
    let client_ip: IpAddr = "198.18.0.3".parse().unwrap();
    assert_eq!(
        stack.backend.state.active_users.lock_recover()[&user_id].sessions[0].peer_ip,
        Some(client_ip)
    );
    stack.hop.switch(next.addr, true).await;
    until(&mut channel, "Reconnected").await;
    assert!(
        !client.is_closed(),
        "reconnect dropped the user's SSH transport"
    );
    assert_eq!(
        next.state.session_registry.user_for(&token).await,
        Some(user_id)
    );
    assert!(
        next.state
            .session_registry
            .send_message(&token, crate::session::SessionMessage::Heartbeat)
            .await
    );
    assert_eq!(
        next.state.active_users.lock_recover()[&user_id].sessions[0].peer_ip,
        Some(client_ip)
    );
    assert_eq!(
        next.state.conn_counts.lock_recover().get(&client_ip),
        Some(&1)
    );
    assert_pair_preserves_client_ip(&next.state, &token, user_id, client_ip).await;
    client
        .disconnect(russh::Disconnect::ByApplication, "done", "")
        .await
        .unwrap();
    drop(channel);
    drop(client);
    timeout(WAIT, next.state.tunnel_sessions.wait_empty())
        .await
        .unwrap();
    stack.cleanup().await;
}

async fn assert_pair_preserves_client_ip(
    state: &State,
    token: &str,
    user_id: uuid::Uuid,
    client_ip: IpAddr,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let shutdown = CancellationToken::new();
    let api_state = state.clone();
    let api_shutdown = shutdown.clone();
    let task = tokio::spawn(crate::api::run_api_server_with_listener(
        listener,
        api_state,
        Some(api_shutdown),
    ));
    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/ws/pair?token={token}"))
            .await
            .unwrap();
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        r#"{"event":"heartbeat"}"#.into(),
    ))
    .await
    .unwrap();
    let response = timeout(WAIT, ws.next()).await.unwrap().unwrap().unwrap();
    let message: serde_json::Value = serde_json::from_str(response.to_text().unwrap()).unwrap();
    assert_eq!(message["event"], "set_playback_source");
    assert_eq!(
        state.active_users.lock_recover()[&user_id].sessions[0].peer_ip,
        Some(client_ip)
    );
    ws.close(None).await.unwrap();
    shutdown.cancel();
    timeout(WAIT, task).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn bastion_chain_extra_channel_is_rejected_without_losing_shell() {
    let stack = Stack::spawn().await;
    let client = stack.connect("198.18.0.4", key()).await;
    let mut channel = shell(&client).await;
    until(&mut channel, "late").await;
    assert!(client.channel_open_session().await.is_err());
    assert!(!client.is_closed());
    client
        .disconnect(russh::Disconnect::ByApplication, "done", "")
        .await
        .unwrap();
    drop(channel);
    drop(client);
    stack.cleanup().await;
}

async fn disconnect(client: client::Handle<AnyHostKey>) {
    let _ = client
        .disconnect(russh::Disconnect::ByApplication, "test done", "")
        .await;
}

async fn assert_shell_closed(channel: &mut Channel<client::Msg>) {
    timeout(WAIT, async {
        while let Some(message) = channel.wait().await {
            if matches!(message, ChannelMsg::Close | ChannelMsg::Eof) {
                return;
            }
        }
    })
    .await
    .expect("moderated user's shell stayed open");
}

async fn assert_exec_rejected(stack: &Stack, ip: &str, key: Arc<PrivateKey>) {
    let client = stack.connect(ip, key).await;
    let (stdout, stderr, status) = exec_output(&client).await;
    assert_eq!(status, 1);
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("tunnel exec failed"));
    disconnect(client).await;
}

async fn client_ip_moderation_isolation(alice_ip: &str, bob_ip: &str) {
    let stack = Stack::spawn().await;
    let state = &stack.backend.state;
    let actor = create_test_user(&stack.db.db, "client-ip-admin").await;
    let transport_owner = create_test_user(&stack.db.db, "transport-ban-owner").await;
    let db = stack.db.db.get().await.unwrap();
    // Even an existing poisoned infrastructure ban must not affect clients
    // whose forwarded addresses are known.
    ServerBan::activate(
        &db,
        ServerBanActivation {
            target_user_id: transport_owner.id,
            fingerprint: Some(&transport_owner.fingerprint),
            ip_address: Some("127.0.0.1"),
            snapshot_username: Some(&transport_owner.username),
            actor_user_id: actor.id,
            reason: "unrelated transport IP ban",
            expires_at: None,
        },
    )
    .await
    .unwrap();

    let alice_key = key();
    let bob_key = key();
    let alice_client = stack.connect(alice_ip, alice_key.clone()).await;
    let alice_token = exec_token(&alice_client).await;
    let mut alice_shell = shell(&alice_client).await;
    until(&mut alice_shell, "late").await;
    let bob_client = stack.connect(bob_ip, bob_key.clone()).await;
    let bob_token = exec_token(&bob_client).await;
    let mut bob_shell = shell(&bob_client).await;
    until(&mut bob_shell, "late").await;
    let alice_id = state.session_registry.user_for(&alice_token).await.unwrap();
    let bob_id = state.session_registry.user_for(&bob_token).await.unwrap();
    let alice = User::get(&db, alice_id).await.unwrap().unwrap();
    let alice_ip_addr: IpAddr = alice_ip.parse().unwrap();
    let bob_ip_addr: IpAddr = bob_ip.parse().unwrap();
    {
        let users = state.active_users.lock_recover();
        assert_eq!(users[&alice_id].sessions[0].peer_ip, Some(alice_ip_addr));
        assert_eq!(users[&bob_id].sessions[0].peer_ip, Some(bob_ip_addr));
        let counts = state.conn_counts.lock_recover();
        assert_eq!(counts.len(), 2);
        assert_eq!(counts[&alice_ip_addr], 1);
        assert_eq!(counts[&bob_ip_addr], 1);
    }

    state
        .chat_service
        .run_mod_command(
            actor.id,
            Permissions::new(false, true),
            &format!("kick server @{} client IP regression", alice.username),
        )
        .await
        .unwrap();
    assert_shell_closed(&mut alice_shell).await;
    wait_until(
        || async { !state.active_users.lock_recover().contains_key(&alice_id) },
        "kicked account retired",
    )
    .await;
    assert!(
        ServerBan::find_active_for_user_id(&db, alice_id)
            .await
            .unwrap()
            .is_none()
    );
    disconnect(alice_client).await;

    // Kick is temporary: the same identity and client IP can return.
    let alice_client = stack.connect(alice_ip, alice_key.clone()).await;
    let _ = exec_token(&alice_client).await;
    let mut alice_shell = shell(&alice_client).await;
    until(&mut alice_shell, "late").await;
    state
        .chat_service
        .run_mod_command(
            actor.id,
            Permissions::new(false, true),
            &format!("ban server @{} 1h client IP regression", alice.username),
        )
        .await
        .unwrap();
    assert_shell_closed(&mut alice_shell).await;
    let ban = ServerBan::find_active_for_user_id(&db, alice_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ban.ip_address.as_deref(), Some(alice_ip));
    assert_eq!(ban.fingerprint.as_deref(), Some(alice.fingerprint.as_str()));
    disconnect(alice_client).await;

    // Check both account and IP enforcement, independently of the TCP peer.
    assert_exec_rejected(&stack, bob_ip, alice_key).await;
    assert_exec_rejected(&stack, alice_ip, key()).await;
    assert!(!bob_client.is_closed());
    assert!(state.session_registry.has_session(&bob_token).await);
    assert_eq!(
        state.active_users.lock_recover()[&bob_id].sessions[0].peer_ip,
        Some(bob_ip_addr)
    );
    let bob_reconnect = stack.connect(bob_ip, bob_key).await;
    let _ = exec_token(&bob_reconnect).await;
    disconnect(bob_reconnect).await;
    disconnect(bob_client).await;
    drop(bob_shell);
    stack.cleanup().await;
}

#[tokio::test]
async fn bastion_client_ip_moderation_isolates_ipv4_clients() {
    client_ip_moderation_isolation("203.0.113.11", "203.0.113.12").await;
}

#[tokio::test]
async fn bastion_client_ip_moderation_isolates_ipv6_clients() {
    client_ip_moderation_isolation("2001:db8::11", "2001:db8::12").await;
}
