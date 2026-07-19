use std::net::SocketAddr;
use std::time::Duration;

use late_core::models::{
    chat_message::ChatMessage,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    irc_token::IrcToken,
    profile::ProfileParams,
    user::{RightSidebarMode, default_right_sidebar_components},
};
use late_core::shutdown::CancellationToken;
use late_core::test_utils::{TestDb, create_test_user};
use crate::config::IrcConfig;
use crate::state::State;
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
        let db = new_test_db().await;
        let mut config = test_config(db.db.config().clone());
        config.irc = IrcConfig {
            enabled: true,
            port: 0,
            ..IrcConfig::default()
        };
        let state = test_app_state(db.db.clone(), config);
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("bind ircd test listener");
        let addr = listener.local_addr().expect("ircd listener addr");
        let shutdown = CancellationToken::new();
        let task_state = state.clone();
        let task_shutdown = shutdown.clone();
        let task = tokio::spawn(async move {
            crate::ircd::serve::run_with_listener(
                task_state,
                Some(task_shutdown),
                listener,
                None,
            )
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
    async fn connect(addr: SocketAddr, token: &str) -> Self {
        let stream = TcpStream::connect(addr).await.expect("connect ircd");
        let mut client = Self {
            reader: BufReader::new(stream),
        };
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
async fn authenticates_with_token_and_forces_lounge_join() {
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
async fn rejects_bad_token_without_registering() {
    let server = IrcTestServer::start().await;
    let mut client = server.connect("late-irc-NOTAREALTOKEN").await;

    let passwd = client.read_until(" 464 ").await;
    assert!(
        passwd.contains("Invalid IRC token"),
        "bad token should get password mismatch detail: {passwd}"
    );
    let error = client.read_until("ERROR :Authentication failed").await;
    assert!(
        error.contains("Authentication failed"),
        "bad token should close with ERROR: {error}"
    );
    assert!(
        client.read_line().await.is_none(),
        "bad-token connection should close after ERROR"
    );
}

#[tokio::test]
async fn irc_only_connection_counts_as_active_until_disconnect() {
    let server = IrcTestServer::start().await;
    let user = server.seed_user("irc-active-user").await;
    let mut client = server.connect(&user.token).await;

    client.read_until(" 376 ").await;
    wait_until(
        || async {
            let active_users = server.state.active_users.lock().expect("active users");
            active_users.get(&user.id).is_some_and(|active| {
                active.username == user.username
                    && active.connection_count == 1
                    && active
                        .sessions
                        .iter()
                        .any(|session| session.token.starts_with("irc:"))
            })
        },
        "IRC-only user tracked as active",
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
            keep_composer_focused: false,
            start_with_music_muted: false,
            land_on_home: false,
            show_flag_fallback: false,
            show_pet_strip: true,
            favorite_room_ids: Vec::new(),
            birthday: None,
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
