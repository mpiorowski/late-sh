use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use crate::app::chat::svc::SendMessageTask;
use crate::config::IrcConfig;
use crate::state::State;
use late_core::MutexRecover;
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_message_reaction::ChatMessageReaction,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    irc_token::IrcToken,
    profile::ProfileParams,
    server_ban::{ServerBan, ServerBanActivation},
    user::{RightSidebarMode, default_right_sidebar_components},
};
use late_core::shutdown::CancellationToken;
use late_core::test_utils::{TestDb, create_test_user};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::{Instant, timeout};

use crate::test_helpers::{new_test_db, test_app_state, test_config, wait_until};

struct IrcTestServer {
    _db: TestDb,
    state: State,
    addr: SocketAddr,
    shutdown: CancellationToken,
    task: JoinHandle<anyhow::Result<()>>,
}

impl IrcTestServer {
    async fn start() -> Self {
        Self::start_with_irc_config(IrcConfig {
            enabled: true,
            port: 0,
            ..crate::test_helpers::test_irc_config()
        })
        .await
    }

    async fn start_with_proxy_protocol() -> Self {
        Self::start_with_irc_config(IrcConfig {
            enabled: true,
            port: 0,
            proxy_protocol: true,
            proxy_trusted_cidrs: vec!["127.0.0.0/8".parse().expect("trusted proxy CIDR")],
            ..crate::test_helpers::test_irc_config()
        })
        .await
    }

    /// Proxy parsing is on, but a loopback transport peer is outside the
    /// trusted list, so any header such a peer sends must be ignored.
    async fn start_with_untrusted_proxy_peer() -> Self {
        Self::start_with_irc_config(IrcConfig {
            enabled: true,
            port: 0,
            proxy_protocol: true,
            proxy_trusted_cidrs: vec!["10.42.0.0/16".parse().expect("trusted proxy CIDR")],
            ..crate::test_helpers::test_irc_config()
        })
        .await
    }

    async fn start_with_irc_config(irc_config: IrcConfig) -> Self {
        let db = new_test_db().await;
        let mut config = test_config(db.db.config().clone());
        config.irc = irc_config;
        let state = test_app_state(db.db.clone(), config);
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind ircd test listener");
        let addr = listener.local_addr().expect("ircd listener addr");
        let shutdown = CancellationToken::new();
        let task_state = state.clone();
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move {
            crate::ircd::serve::run_with_listener(task_state, Some(task_shutdown), listener, None)
                .await
        });

        Self {
            _db: db,
            state,
            addr,
            shutdown,
            task,
        }
    }

    async fn seed_user(&self, username: &str) -> IrcUser {
        let client = self.state.db.get().await.expect("db client");
        let user = create_test_user(&self.state.db, username).await;
        let lounge = ChatRoom::ensure_lounge(&client)
            .await
            .expect("ensure lounge");
        ChatRoomMember::join(&client, lounge.id, user.id)
            .await
            .expect("join lounge");
        crate::usernames::upsert(
            &self.state.username_directory,
            user.id,
            user.username.clone(),
        );
        let token = IrcToken::mint(&client, user.id).await.expect("mint token");
        IrcUser {
            id: user.id,
            username: user.username,
            token,
            lounge_id: lounge.id,
        }
    }

    async fn connect(&self, token: &str) -> IrcClient {
        IrcClient::connect(self.addr, token).await
    }

    async fn connect_with_caps(&self, token: &str, caps: &str) -> IrcClient {
        IrcClient::connect_with_caps(self.addr, token, caps).await
    }
}

impl Drop for IrcTestServer {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.task.abort();
    }
}

struct IrcUser {
    id: uuid::Uuid,
    username: String,
    token: String,
    lounge_id: uuid::Uuid,
}

struct IrcClient {
    reader: BufReader<TcpStream>,
}

impl IrcClient {
    async fn open(addr: SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).await.expect("connect ircd");
        Self {
            reader: BufReader::new(stream),
        }
    }

    async fn connect(addr: SocketAddr, token: &str) -> Self {
        let mut client = Self::open(addr).await;
        client
            .write_line(&format!("PASS {token}"))
            .await
            .expect("send PASS");
        client
            .write_line("NICK requested")
            .await
            .expect("send NICK");
        client
            .write_line("USER tester 0 * :Test User")
            .await
            .expect("send USER");
        client
    }

    async fn connect_with_caps(addr: SocketAddr, token: &str, caps: &str) -> Self {
        let mut client = Self::open(addr).await;
        client.write_line("CAP LS 302").await.expect("send CAP LS");
        let ls = client.read_until(" CAP * LS ").await;
        assert!(
            ls.contains("message-tags")
                && ls.contains("server-time")
                && ls.contains("echo-message"),
            "CAP LS should advertise Tier 1 caps: {ls}"
        );
        client
            .write_line(&format!("PASS {token}"))
            .await
            .expect("send PASS");
        client
            .write_line("NICK requested")
            .await
            .expect("send NICK");
        client
            .write_line("USER tester 0 * :Test User")
            .await
            .expect("send USER");
        client
            .write_line(&format!("CAP REQ :{caps}"))
            .await
            .expect("send CAP REQ");
        let ack = client.read_until(" CAP * ACK ").await;
        assert!(ack.ends_with(caps), "CAP REQ should be ACKed: {ack}");
        client.write_line("CAP END").await.expect("send CAP END");
        client
    }

    async fn connect_with_proxy(addr: SocketAddr, token: &str, source_ip: IpAddr) -> Self {
        let family = if source_ip.is_ipv4() { "TCP4" } else { "TCP6" };
        let proxy_line = format!(
            "PROXY {family} {source_ip} {} 54321 {}\r\n",
            addr.ip(),
            addr.port()
        );
        Self::connect_after_proxy_line(addr, token, &proxy_line).await
    }

    async fn connect_with_unknown_proxy(addr: SocketAddr, token: &str) -> Self {
        Self::connect_after_proxy_line(addr, token, "PROXY UNKNOWN\r\n").await
    }

    async fn connect_after_proxy_line(addr: SocketAddr, token: &str, proxy_line: &str) -> Self {
        let mut client = Self::open(addr).await;
        client
            .reader
            .get_mut()
            .write_all(proxy_line.as_bytes())
            .await
            .expect("send PROXY header");
        client
            .write_line(&format!("PASS {token}"))
            .await
            .expect("send PASS");
        client
            .write_line("NICK requested")
            .await
            .expect("send NICK");
        client
            .write_line("USER tester 0 * :Test User")
            .await
            .expect("send USER");
        client
    }

    async fn connect_for_registration(addr: SocketAddr) -> Self {
        Self::open(addr).await
    }

    async fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        let stream = self.reader.get_mut();
        stream.write_all(line.as_bytes()).await?;
        stream.write_all(b"\r\n").await?;
        stream.flush().await
    }

    async fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        let n = timeout(Duration::from_secs(3), self.reader.read_line(&mut line))
            .await
            .expect("IRC line timeout")
            .expect("read IRC line");
        if n == 0 {
            None
        } else {
            Some(line.trim_end_matches(['\r', '\n']).to_string())
        }
    }

    async fn read_until(&mut self, needle: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut transcript = Vec::new();
        while Instant::now() < deadline {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let mut line = String::new();
            let n = timeout(remaining, self.reader.read_line(&mut line))
                .await
                .expect("IRC line timeout")
                .expect("read IRC line");
            if n == 0 {
                break;
            }
            let line = line.trim_end_matches(['\r', '\n']).to_string();
            if line.contains(needle) {
                return line;
            }
            transcript.push(line);
        }
        panic!(
            "timed out waiting for {needle:?}; transcript:\n{}",
            transcript.join("\n")
        );
    }

    async fn read_available_for(&mut self, duration: Duration) -> Vec<String> {
        let deadline = Instant::now() + duration;
        let mut lines = Vec::new();
        while Instant::now() < deadline {
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            let mut line = String::new();
            match timeout(
                remaining.min(Duration::from_millis(100)),
                self.reader.read_line(&mut line),
            )
            .await
            {
                Ok(Ok(0)) => break,
                Ok(Ok(_)) => lines.push(line.trim_end_matches(['\r', '\n']).to_string()),
                Ok(Err(err)) => panic!("read IRC line: {err}"),
                Err(_) => {}
            }
        }
        lines
    }
}

#[tokio::test]
async fn authenticates_valid_token_and_rejects_bad_token() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-good-user").await;
    let mut client = server.connect(&user.token).await;

    let welcome = client.read_until(" 001 ").await;
    assert!(
        welcome.contains(&format!(" 001 {} ", user.username)),
        "welcome should use canonical username: {welcome}"
    );
    client.read_until(" 376 ").await;
    client
        .read_until(&format!(
            ":{}!{}@late.sh JOIN #lounge",
            user.username, user.username
        ))
        .await;
    let names = client.read_until(" 353 ").await;
    assert!(
        names.contains("#lounge") && names.contains(&user.username),
        "forced lounge NAMES should include the connected user: {names}"
    );
    let names_end = client.read_until(" 366 ").await;
    assert!(
        names_end.contains("#lounge"),
        "forced lounge join should end NAMES for #lounge: {names_end}"
    );

    let mut bad_client = server.connect("late-irc-NOTAREALTOKEN").await;
    let passwd = bad_client.read_until(" 464 ").await;
    assert!(
        passwd.contains("Invalid IRC token"),
        "bad token should get password mismatch detail: {passwd}"
    );
    let error = bad_client.read_until("ERROR :Authentication failed").await;
    assert!(
        error.contains("Authentication failed"),
        "bad token should close with ERROR: {error}"
    );
    assert!(
        bad_client.read_line().await.is_none(),
        "bad-token connection should close after ERROR"
    );
}

#[tokio::test]
async fn trusted_proxy_client_ip_avoids_transport_ip_ban_and_is_tracked() {
    let server = IrcTestServer::start_with_proxy_protocol().await;
    let banned_transport_owner = create_test_user(&server.state.db, "irc-transport-ban").await;
    let client = server.state.db.get().await.expect("db client");
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: banned_transport_owner.id,
            fingerprint: Some(&banned_transport_owner.fingerprint),
            ip_address: Some("127.0.0.1"),
            snapshot_username: Some(&banned_transport_owner.username),
            actor_user_id: banned_transport_owner.id,
            reason: "test shared transport ban",
            expires_at: None,
        },
    )
    .await
    .expect("activate transport IP ban");
    drop(client);

    let user = server.seed_user("irc-proxied-user").await;
    let client_ip: IpAddr = "203.0.113.77".parse().expect("client IP");
    let mut irc = IrcClient::connect_with_proxy(server.addr, &user.token, client_ip).await;

    irc.read_until(" 001 ").await;
    let active_users = server.state.active_users.lock_recover();
    let active = active_users.get(&user.id).expect("active IRC user");
    assert_eq!(active.sessions.len(), 1);
    assert_eq!(active.sessions[0].peer_ip, Some(client_ip));
}

/// The complement of the test above: demoting the transport IP must not have
/// demoted IP bans themselves. A ban on the address the trusted proxy reports
/// still has to bite, ahead of the token lookup.
#[tokio::test]
async fn trusted_proxy_client_ip_is_still_matched_against_ip_bans() {
    let server = IrcTestServer::start_with_proxy_protocol().await;
    let banned_ip: IpAddr = "203.0.113.99".parse().expect("banned client IP");
    let banned_ip_text = banned_ip.to_string();
    let ban_owner = create_test_user(&server.state.db, "irc-client-ip-ban").await;
    let client = server.state.db.get().await.expect("db client");
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: ban_owner.id,
            fingerprint: Some(&ban_owner.fingerprint),
            ip_address: Some(&banned_ip_text),
            snapshot_username: Some(&ban_owner.username),
            actor_user_id: ban_owner.id,
            reason: "test client IP ban",
            expires_at: None,
        },
    )
    .await
    .expect("activate client IP ban");
    drop(client);

    let user = server.seed_user("irc-banned-client-ip-user").await;
    let mut irc = IrcClient::connect_with_proxy(server.addr, &user.token, banned_ip).await;

    let banned = irc.read_until(" 465 ").await;
    assert!(
        banned.contains("You are banned from this server"),
        "a valid token from a banned proxy-supplied IP must still be refused: {banned}"
    );
}

/// Only peers inside the trusted CIDRs may state a client IP. A header from
/// anyone else is ordinary connection data, and the transport address it tried
/// to talk its way out of still applies.
#[tokio::test]
async fn untrusted_peer_cannot_forge_a_client_ip_to_evade_an_ip_ban() {
    let server = IrcTestServer::start_with_untrusted_proxy_peer().await;
    let ban_owner = create_test_user(&server.state.db, "irc-forged-header-ban").await;
    let client = server.state.db.get().await.expect("db client");
    ServerBan::activate(
        &client,
        ServerBanActivation {
            target_user_id: ban_owner.id,
            fingerprint: Some(&ban_owner.fingerprint),
            ip_address: Some("127.0.0.1"),
            snapshot_username: Some(&ban_owner.username),
            actor_user_id: ban_owner.id,
            reason: "test transport IP ban",
            expires_at: None,
        },
    )
    .await
    .expect("activate transport IP ban");
    drop(client);

    let user = server.seed_user("irc-forging-user").await;
    let forged_ip: IpAddr = "203.0.113.200".parse().expect("forged client IP");
    let mut irc = IrcClient::connect_with_proxy(server.addr, &user.token, forged_ip).await;

    let banned = irc.read_until(" 465 ").await;
    assert!(
        banned.contains("You are banned from this server"),
        "a PROXY header from an untrusted peer must not replace the transport IP: {banned}"
    );
}

#[tokio::test]
async fn unknown_proxy_address_is_never_persisted_as_transport_ip() {
    let server = IrcTestServer::start_with_proxy_protocol().await;
    let user = server.seed_user("irc-unknown-proxy-user").await;
    let mut irc = IrcClient::connect_with_unknown_proxy(server.addr, &user.token).await;

    irc.read_until(" 001 ").await;
    let active_users = server.state.active_users.lock_recover();
    let active = active_users.get(&user.id).expect("active IRC user");
    assert_eq!(active.sessions.len(), 1);
    assert_eq!(active.sessions[0].peer_ip, None);
}

#[tokio::test]
async fn trusted_proxy_without_header_is_accepted_without_transport_ip() {
    let server = IrcTestServer::start_with_proxy_protocol().await;
    let user = server.seed_user("irc-proxy-rollout-user").await;
    let mut irc = server.connect(&user.token).await;

    irc.read_until(" 001 ").await;
    let active_users = server.state.active_users.lock_recover();
    let active = active_users.get(&user.id).expect("active IRC user");
    assert_eq!(active.sessions.len(), 1);
    assert_eq!(active.sessions[0].peer_ip, None);
}

#[tokio::test]
async fn cap_negotiation_advertises_acks_lists_and_naks_tier1_caps() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-cap-user").await;
    let mut client = IrcClient::connect_for_registration(server.addr).await;

    client.write_line("CAP LS 302").await.expect("send CAP LS");
    let ls = client.read_until(" CAP * LS ").await;
    assert!(
        ls.contains(":message-tags server-time echo-message"),
        "CAP LS should advertise only Tier 1 caps: {ls}"
    );

    client
        .write_line(&format!("PASS {}", user.token))
        .await
        .expect("send PASS");
    client
        .write_line("NICK requested")
        .await
        .expect("send NICK");
    client
        .write_line("USER tester 0 * :Test User")
        .await
        .expect("send USER");
    client
        .write_line("CAP REQ :message-tags server-time echo-message")
        .await
        .expect("send CAP REQ");
    let ack = client.read_until(" CAP * ACK ").await;
    assert!(
        ack.ends_with(":message-tags server-time echo-message"),
        "supported caps should be ACKed: {ack}"
    );

    client.write_line("CAP LIST").await.expect("send CAP LIST");
    let list = client.read_until(" CAP * LIST ").await;
    assert!(
        list.ends_with(":message-tags server-time echo-message"),
        "CAP LIST should show acknowledged caps: {list}"
    );

    client
        .write_line("CAP REQ :chathistory")
        .await
        .expect("send unsupported CAP REQ");
    let nak = client.read_until(" CAP * NAK ").await;
    assert!(
        nak.ends_with("chathistory"),
        "unsupported cap should be NAKed: {nak}"
    );

    client.write_line("CAP END").await.expect("send CAP END");
    client.read_until(" 001 ").await;
    client.read_until(" 376 ").await;
}

#[tokio::test]
async fn projects_dotted_usernames_to_irc_nicks() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc.dot.user").await;
    let nick = "irc^dot^user";
    let mut client = server.connect(&user.token).await;

    let welcome = client.read_until(" 001 ").await;
    assert!(
        welcome.contains(&format!(" 001 {nick} ")),
        "welcome should use IRC-safe nick: {welcome}"
    );
    client.read_until(" 376 ").await;
    client
        .read_until(&format!(":{nick}!{nick}@late.sh JOIN #lounge"))
        .await;
    let names = client.read_until(" 353 ").await;
    assert!(
        names.contains(nick) && !names.contains(&user.username),
        "NAMES should include projected nick, not raw dotted username: {names}"
    );
    client.read_until(" 366 ").await;

    client
        .write_line(&format!("WHOIS {nick}"))
        .await
        .expect("send WHOIS");
    let whois = client.read_until(" 311 ").await;
    assert!(
        whois.contains(nick) && !whois.contains(&user.username),
        "WHOIS should resolve projected nick: {whois}"
    );

    client
        .write_line(&format!("USERHOST {nick}"))
        .await
        .expect("send USERHOST");
    let userhost = client.read_until(" 302 ").await;
    assert!(
        userhost.contains(&format!("{nick}=+{nick}@late.sh")),
        "USERHOST should return projected nick and ident: {userhost}"
    );

    client
        .write_line(&format!("ISON {nick}"))
        .await
        .expect("send ISON");
    let ison = client.read_until(" 303 ").await;
    assert!(
        ison.contains(nick) && !ison.contains(&user.username),
        "ISON should return projected nick: {ison}"
    );
}

#[tokio::test]
async fn concurrent_irc_connections_share_online_presence_until_last_disconnect() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-active-user").await;
    let mut client = server.connect(&user.token).await;
    let mut second_client = server.connect(&user.token).await;

    client.read_until(" 376 ").await;
    second_client.read_until(" 376 ").await;
    wait_until(
        || async {
            let active_users = server.state.active_users.lock().expect("active users");
            active_users.get(&user.id).is_some_and(|active| {
                active.username == user.username
                    && active.connection_count == 2
                    && server
                        .state
                        .leaderboard_service
                        .online_user_is_active(user.id)
                    && active
                        .sessions
                        .iter()
                        .any(|session| session.token.starts_with("irc:"))
            })
        },
        "concurrent IRC user tracked once as active",
    )
    .await;

    client.write_line("QUIT :bye").await.expect("send QUIT");
    client.read_until("ERROR :Closing Link").await;
    assert!(
        client.read_line().await.is_none(),
        "QUIT should close IRC connection"
    );
    wait_until(
        || async {
            let active_users = server.state.active_users.lock().expect("active users");
            active_users.get(&user.id).is_some_and(|active| {
                active.connection_count == 1
                    && server
                        .state
                        .leaderboard_service
                        .online_user_is_active(user.id)
            })
        },
        "one remaining IRC connection keeps online-time tracking active",
    )
    .await;

    second_client
        .write_line("QUIT :bye")
        .await
        .expect("send second QUIT");
    second_client.read_until("ERROR :Closing Link").await;
    assert!(
        second_client.read_line().await.is_none(),
        "second QUIT should close IRC connection"
    );
    wait_until(
        || async {
            !server
                .state
                .active_users
                .lock()
                .expect("active users")
                .contains_key(&user.id)
        },
        "IRC-only user removed from active users",
    )
    .await;
    assert!(
        !server
            .state
            .leaderboard_service
            .online_user_is_active(user.id),
        "last IRC disconnect stops online-time tracking"
    );
}

#[tokio::test]
async fn profile_username_change_projects_to_live_irc_session() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-rename-old").await;
    let mut client = server.connect(&user.token).await;

    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server.state.profile_service.edit_profile(
        user.id,
        ProfileParams {
            username: "irc.rename.new".to_string(),
            bio: String::new(),
            country: None,
            timezone: None,
            ide: None,
            terminal: None,
            os: None,
            langs: Vec::new(),
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: None,
            enable_background_color: false,
            text_brightness_adjustment: 0,
            show_right_sidebar: true,
            right_sidebar_mode: RightSidebarMode::On,
            right_sidebar_components: default_right_sidebar_components(),
            show_room_list_sidebar: true,
            room_list_mode: late_core::models::user::RoomListMode::On,
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            translate_to: late_core::models::message_translation::TranslateLang::En,
            auto_translate: false,
            translate_mine_to_en: false,
            favorite_room_ids: Vec::new(),
            favorite_theme_ids: Vec::new(),
        },
    );

    let nick = client.read_until(" NICK ").await;
    assert!(
        nick.contains(":irc-rename-old!irc-rename-old@late.sh NICK irc^rename^new"),
        "profile rename should project as IRC NICK: {nick}"
    );

    client.write_line("LUSERS").await.expect("send LUSERS");
    let lusers = client.read_until(" 251 ").await;
    assert!(
        lusers.contains(" 251 irc^rename^new "),
        "subsequent numerics should target the new nick: {lusers}"
    );
}

#[tokio::test]
async fn refuses_part_lounge_and_rejoins() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-sticky-user").await;
    let mut client = server.connect(&user.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client.write_line("PART #lounge").await.expect("send PART");

    let restricted = client.read_until(" 484 ").await;
    assert!(
        restricted.contains("You cannot leave the lounge"),
        "PART #lounge should be refused: {restricted}"
    );
    client.read_until("Everyone stays in #lounge").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;
}

#[tokio::test]
async fn privmsg_lounge_persists_to_chat() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-privmsg-user").await;
    let mut client = server.connect(&user.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line("PRIVMSG #lounge :hello from irc")
        .await
        .expect("send PRIVMSG");

    wait_until(
        || async {
            let client = server.state.db.get().await.expect("db client");
            let messages = ChatMessage::list_recent(&client, user.lounge_id, 5)
                .await
                .expect("recent messages");
            messages
                .iter()
                .any(|msg| msg.user_id == user.id && msg.body == "hello from irc")
        },
        "IRC PRIVMSG persisted",
    )
    .await;

    let lines = client.read_available_for(Duration::from_millis(250)).await;
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("PRIVMSG #lounge :hello from irc")),
        "sender connection should suppress one self echo: {lines:?}"
    );
}

#[tokio::test]
async fn echo_message_client_receives_own_privmsg_with_time_and_msgid() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-echo-user").await;
    let mut client = server
        .connect_with_caps(&user.token, "message-tags server-time echo-message")
        .await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line("PRIVMSG #lounge :hello tagged irc")
        .await
        .expect("send PRIVMSG");

    let echo = client.read_until("PRIVMSG #lounge :hello tagged irc").await;
    assert!(
        echo.starts_with("@time="),
        "echo should include server-time: {echo}"
    );
    assert!(
        echo.contains(";msgid="),
        "echo should include msgid: {echo}"
    );
    assert!(
        echo.contains(&format!(" :{}!{}@late.sh ", user.username, user.username)),
        "echo should retain user prefix: {echo}"
    );
}

#[tokio::test]
async fn tag_unaware_client_receives_plain_tui_privmsg_fallback() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-plain-tui-user").await;
    let mut client = server.connect(&user.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server.state.chat_service.send_message_task(
        user.id,
        user.lounge_id,
        Some("lounge".to_string()),
        "plain from tui".to_string(),
        uuid::Uuid::new_v4(),
        false,
    );

    let line = client.read_until("PRIVMSG #lounge :plain from tui").await;
    assert!(
        !line.starts_with('@')
            && line.contains(&format!(":{}!{}@late.sh ", user.username, user.username)),
        "tag-unaware client should receive an untagged PRIVMSG fallback: {line}"
    );
}

#[tokio::test]
async fn tui_reply_projects_reply_tag_to_tag_aware_client() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-tui-reply-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "reply parent from tui".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server.connect_with_caps(&user.token, "message-tags").await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server
        .state
        .chat_service
        .send_message_with_reply_task(SendMessageTask {
            user_id: user.id,
            room_id: user.lounge_id,
            room_slug: Some("lounge".to_string()),
            body: "reply from tui".to_string(),
            reply_to_message_id: Some(parent.id),
            request_id: uuid::Uuid::new_v4(),
            is_admin: false,
        });

    let line = client.read_until("PRIVMSG #lounge :reply from tui").await;
    assert!(
        line.starts_with("@msgid=")
            && line.contains(&format!(";+draft/reply={}", parent.id))
            && line.contains(&format!(";+reply={}", parent.id))
            && line.contains(&format!(":{}!{}@late.sh ", user.username, user.username)),
        "tag-aware client should receive msgid and both reply tags for TUI replies: {line}"
    );
}

#[tokio::test]
async fn tagged_privmsg_reply_persists_reply_target() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-reply-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "parent from tui".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server.connect_with_caps(&user.token, "message-tags").await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line(&format!(
            "@+reply={} PRIVMSG #lounge :child from irc",
            parent.id
        ))
        .await
        .expect("send tagged reply");

    wait_until(
        || async {
            let client = server.state.db.get().await.expect("db client");
            let messages = ChatMessage::list_recent(&client, user.lounge_id, 5)
                .await
                .expect("recent messages");
            messages.iter().any(|msg| {
                msg.user_id == user.id
                    && msg.body == "child from irc"
                    && msg.reply_to_message_id == Some(parent.id)
            })
        },
        "IRC tagged reply persisted",
    )
    .await;
}

#[tokio::test]
async fn malformed_tagged_reply_is_rejected() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-bad-reply-user").await;
    let mut client = server.connect_with_caps(&user.token, "message-tags").await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line("@+reply=not-a-uuid PRIVMSG #lounge :bad child")
        .await
        .expect("send malformed tagged reply");

    let error = client
        .read_until("IRC reply tag is not a valid msgid")
        .await;
    assert!(
        error.contains(" 404 ") && error.contains("#lounge"),
        "malformed reply should be rejected with channel send error: {error}"
    );
}

#[tokio::test]
async fn tagged_reaction_toggles_late_reaction_without_storing_fallback_body() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-react-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "reaction parent".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server.connect_with_caps(&user.token, "message-tags").await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line(&format!(
            "@+reply={};+draft/react=👀 TAGMSG #lounge",
            parent.id
        ))
        .await
        .expect("send tagged reaction");

    wait_until(
        || async {
            let client = server.state.db.get().await.expect("db client");
            ChatMessageReaction::get_by_user_and_message(&client, parent.id, user.id)
                .await
                .expect("reaction lookup")
                .is_some_and(|reaction| reaction.icon == "👀")
        },
        "IRC tagged reaction persisted",
    )
    .await;

    client
        .write_line(&format!(
            "@+reply={};+draft/react=🔥 PRIVMSG #lounge :fallback body",
            parent.id
        ))
        .await
        .expect("send reaction-bearing PRIVMSG");

    wait_until(
        || async {
            let client = server.state.db.get().await.expect("db client");
            ChatMessageReaction::get_by_user_and_message(&client, parent.id, user.id)
                .await
                .expect("reaction lookup")
                .is_some_and(|reaction| reaction.icon == "🔥")
        },
        "IRC reaction-bearing PRIVMSG replaced reaction",
    )
    .await;

    {
        let client = server.state.db.get().await.expect("db client");
        let messages = ChatMessage::list_recent(&client, user.lounge_id, 10)
            .await
            .expect("recent messages");
        assert!(
            messages
                .iter()
                .all(|message| message.body != "fallback body"),
            "reaction-bearing PRIVMSG should not persist fallback body: {messages:?}"
        );
    }

    client
        .write_line(&format!(
            "@+reply={};+draft/react=🔥 TAGMSG #lounge",
            parent.id
        ))
        .await
        .expect("send duplicate tagged reaction");

    wait_until(
        || async {
            let client = server.state.db.get().await.expect("db client");
            ChatMessageReaction::get_by_user_and_message(&client, parent.id, user.id)
                .await
                .expect("reaction lookup")
                .is_none()
        },
        "duplicate IRC tagged reaction toggled off",
    )
    .await;
}

#[tokio::test]
async fn tagged_reaction_without_dm_room_is_rejected_and_creates_none() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-dm-react-user").await;
    let peer = server.seed_user("irc-dm-react-peer").await;
    let mut client = server.connect_with_caps(&user.token, "message-tags").await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line(&format!(
            "@+reply={};+draft/react=👀 TAGMSG {}",
            uuid::Uuid::new_v4(),
            peer.username
        ))
        .await
        .expect("send tagged reaction without a DM room");

    let error = client
        .read_until("IRC reply target is not in this conversation")
        .await;
    assert!(
        error.contains(" 404 "),
        "reaction without a DM room should be rejected: {error}"
    );

    let db = server.state.db.get().await.expect("db client");
    assert!(
        ChatRoom::get_dm(&db, user.id, peer.id)
            .await
            .expect("dm room lookup")
            .is_none(),
        "rejected reaction must not create a DM room"
    );
}

#[tokio::test]
async fn outbound_reaction_delta_projects_tagmsg() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-reaction-echo-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "reaction echo parent".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server
        .connect_with_caps(&user.token, "message-tags server-time echo-message")
        .await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server
        .state
        .chat_service
        .toggle_message_reaction(user.id, parent.id, "👀")
        .await
        .expect("toggle reaction");

    let tagmsg = client.read_until("TAGMSG #lounge").await;
    assert!(
        tagmsg.starts_with("@time=")
            && tagmsg.contains(";msgid=")
            && tagmsg.contains(&format!(";+draft/reply={}", parent.id))
            && tagmsg.contains(&format!(";+reply={}", parent.id))
            && tagmsg.contains("+draft/react=👀")
            && tagmsg.contains(&format!(":{}!{}@late.sh ", user.username, user.username)),
        "reaction delta should project as a time/msgid-tagged TAGMSG: {tagmsg}"
    );
}

#[tokio::test]
async fn tag_unaware_client_does_not_receive_reaction_noise() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-reaction-silent-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "silent reaction parent".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server.connect(&user.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server
        .state
        .chat_service
        .toggle_message_reaction(user.id, parent.id, "👀")
        .await
        .expect("toggle reaction");

    let lines = client.read_available_for(Duration::from_millis(250)).await;
    assert!(
        lines
            .iter()
            .all(|line| !line.contains("TAGMSG") && !line.contains("draft/react")),
        "tag-unaware clients should not receive reaction fallback noise: {lines:?}"
    );
}

#[tokio::test]
async fn non_echo_client_does_not_receive_own_reaction_tagmsg() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-reaction-noecho-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "noecho reaction parent".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server.connect_with_caps(&user.token, "message-tags").await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server
        .state
        .chat_service
        .toggle_message_reaction(user.id, parent.id, "👀")
        .await
        .expect("toggle reaction");

    let lines = client.read_available_for(Duration::from_millis(250)).await;
    assert!(
        lines.iter().all(|line| !line.contains("TAGMSG")),
        "non-echo clients should not receive their own reaction TAGMSG: {lines:?}"
    );
}

#[tokio::test]
async fn replacement_reaction_projects_unreact_then_react() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-reaction-replace-user").await;
    let db = server.state.db.get().await.expect("db client");
    let parent = ChatMessage::create_with_reply_to(
        &db,
        ChatMessageParams {
            room_id: user.lounge_id,
            user_id: user.id,
            body: "replacement reaction parent".to_string(),
        },
        None,
    )
    .await
    .expect("create parent message");
    drop(db);
    let mut client = server
        .connect_with_caps(&user.token, "message-tags echo-message")
        .await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server
        .state
        .chat_service
        .toggle_message_reaction(user.id, parent.id, "👀")
        .await
        .expect("initial reaction");
    client.read_until("+draft/react=👀").await;

    server
        .state
        .chat_service
        .toggle_message_reaction(user.id, parent.id, "🔥")
        .await
        .expect("replace reaction");

    let unreact = client.read_until("+draft/unreact=👀").await;
    let react = client.read_until("+draft/react=🔥").await;
    assert!(
        unreact.contains(&format!("+reply={}", parent.id))
            && react.contains(&format!("+reply={}", parent.id)),
        "replacement should reference the same parent msgid: unreact={unreact}, react={react}"
    );
}

#[tokio::test]
async fn irc_payload_mentions_are_rewritten_to_late_usernames() {
    let server = IrcTestServer::start().await;
    let mentioned = server.seed_user("irc.mention.target").await;
    let sender = server.seed_user("irc-mention-sender").await;
    let mut client = server.connect(&sender.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    client
        .write_line("PRIVMSG #lounge :@irc^mention^target hello")
        .await
        .expect("send PRIVMSG");

    wait_until(
        || async {
            let client = server.state.db.get().await.expect("db client");
            let messages = ChatMessage::list_recent(&client, sender.lounge_id, 5)
                .await
                .expect("recent messages");
            messages.iter().any(|msg| {
                msg.user_id == sender.id && msg.body == format!("@{} hello", mentioned.username)
            })
        },
        "IRC mention persisted as late.sh username",
    )
    .await;
}

#[tokio::test]
async fn late_payload_mentions_are_rewritten_to_irc_nicks() {
    let server = IrcTestServer::start().await;
    let mentioned = server.seed_user("irc.payload.target").await;
    let sender = server.seed_user("irc-payload-sender").await;
    let mut client = server.connect(&mentioned.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;
    client.read_until(" 366 ").await;

    server.state.chat_service.send_message_task(
        sender.id,
        sender.lounge_id,
        Some("lounge".to_string()),
        format!("@{} hello", mentioned.username),
        uuid::Uuid::new_v4(),
        false,
    );

    let line = client.read_until("PRIVMSG #lounge").await;
    assert!(
        line.contains("@irc^payload^target hello"),
        "IRC payload should mention projected nick: {line}"
    );
}

#[tokio::test]
async fn token_revoke_disconnects_live_connection() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-revoke-user").await;
    let mut client = server.connect(&user.token).await;
    client.read_until(" 376 ").await;
    client.read_until(" JOIN #lounge").await;

    server.state.profile_service.revoke_irc_token(user.id);

    let error = client.read_until("ERROR :IRC token revoked").await;
    assert!(
        error.contains("IRC token revoked"),
        "revoke should send ERROR before closing: {error}"
    );
    assert!(
        client.read_line().await.is_none(),
        "revoked connection should close"
    );
}
