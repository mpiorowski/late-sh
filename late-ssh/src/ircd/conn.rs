//! Per-connection state machine: registration → registered session.
//!
//! See devdocs/FRD-IRCD.md for the protocol surface contract (§7) and
//! channel/messaging semantics (§6, §8).

use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::IpAddr,
    time::Duration,
};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use irc_proto::{
    CapSubCommand, ChannelMode, Command, IrcCodec, Message, Mode, Response, message::Tag,
};
use late_core::{
    MutexRecover,
    models::{
        chat_room::ChatRoom, chat_room_member::ChatRoomMember, room_ban::RoomBan, user::User,
    },
    rate_limit::IpRateLimiter,
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::mpsc,
    time::Instant,
};
use tokio_util::codec::Framed;
use uuid::Uuid;

use super::{
    auth::{self, AuthOutcome},
    motd, proj,
    registry::IrcControl,
    replies::{self, NETWORK_NAME, SERVER_NAME, VERSION_STRING},
};
use crate::{
    app::chat::svc::ChatEvent,
    authz::Permissions,
    moderation::{
        command::{RoleAction, RoomModAction},
        event::ModerationEvent,
    },
    state::State,
    usernames,
};

const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(60);
const PING_INTERVAL: Duration = Duration::from_secs(60);
const PONG_GRACE: Duration = Duration::from_secs(240);
const RECENT_SENDS_MAX: usize = 64;
const COMMAND_RATE_WINDOW: Duration = Duration::from_secs(10);
const COMMAND_RATE_MAX: usize = 40;
/// How often to diff the online-user set for JOIN/QUIT projection (FRD §6.4).
const PRESENCE_POLL_INTERVAL: Duration = Duration::from_secs(30);
/// Tarpit delays after a failed auth: short normally, long once the per-IP
/// failure limiter trips. Tokens are 160-bit so this is abuse control, not a
/// security boundary (FRD §5.2 A4).
const AUTH_FAIL_DELAY: Duration = Duration::from_secs(1);
const AUTH_FAIL_DELAY_LIMITED: Duration = Duration::from_secs(8);
const SUPPORTED_CAPS: &[&str] = &["message-tags", "server-time", "echo-message"];

pub trait IrcIo: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> IrcIo for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

type IrcStream = Framed<Box<dyn IrcIo>, IrcCodec>;

pub async fn handle<S>(
    state: State,
    stream: S,
    peer_ip: IpAddr,
    auth_limiter: IpRateLimiter,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let codec = IrcCodec::new("utf8").map_err(|e| anyhow::anyhow!("irc codec: {e}"))?;
    let mut framed = Framed::new(Box::new(stream) as Box<dyn IrcIo>, codec);

    let Some(registration) = register(&state, &mut framed, peer_ip, &auth_limiter).await? else {
        return Ok(());
    };

    let ignored_user_ids = {
        let client = state.db.get().await?;
        User::ignored_user_ids(&client, registration.user_id)
            .await?
            .into_iter()
            .collect()
    };
    let conn_id = state.irc_registry.next_conn_id();
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    if !state.irc_registry.try_register(
        registration.user_id,
        conn_id,
        control_tx,
        state.config.irc.max_conns_per_user,
    ) {
        send_all(
            &mut framed,
            vec![replies::error("Too many IRC connections for your account")],
        )
        .await?;
        return Ok(());
    }

    let mut session = Session {
        state: state.clone(),
        user_id: registration.user_id,
        nick: registration.nick,
        is_admin: registration.is_admin,
        is_moderator: registration.is_moderator,
        joined: HashMap::new(),
        channels: HashMap::new(),
        dm_peers: HashMap::new(),
        non_dm_target_rooms: HashSet::new(),
        ignored_user_ids,
        caps: registration.caps,
        recent_sends: VecDeque::new(),
        recent_commands: VecDeque::new(),
        last_rate_notice: None,
        last_online: HashSet::new(),
    };

    let result = session.run(&mut framed, control_rx).await;
    state.irc_registry.unregister(session.user_id, conn_id);
    result
}

struct Registered {
    user_id: Uuid,
    nick: String,
    is_admin: bool,
    is_moderator: bool,
    caps: IrcCapabilities,
}

#[derive(Default)]
struct Pending {
    pass: Option<String>,
    nick_seen: bool,
    user_seen: bool,
    cap_open: bool,
    caps: IrcCapabilities,
}

impl Pending {
    fn ready(&self) -> bool {
        self.nick_seen && self.user_seen && !self.cap_open
    }
}

#[derive(Clone, Copy, Default)]
struct IrcCapabilities {
    message_tags: bool,
    server_time: bool,
    echo_message: bool,
}

impl IrcCapabilities {
    fn enable(&mut self, cap: &str) {
        match cap {
            "message-tags" => self.message_tags = true,
            "server-time" => self.server_time = true,
            "echo-message" => self.echo_message = true,
            _ => {}
        }
    }

    fn disable(&mut self, cap: &str) {
        match cap {
            "message-tags" => self.message_tags = false,
            "server-time" => self.server_time = false,
            "echo-message" => self.echo_message = false,
            _ => {}
        }
    }

    fn as_list(self) -> String {
        let mut caps = Vec::new();
        if self.message_tags {
            caps.push("message-tags");
        }
        if self.server_time {
            caps.push("server-time");
        }
        if self.echo_message {
            caps.push("echo-message");
        }
        caps.join(" ")
    }
}

fn supported_cap_list() -> String {
    SUPPORTED_CAPS.join(" ")
}

fn apply_cap_request(enabled: &mut IrcCapabilities, requested: &str) -> bool {
    let tokens: Vec<&str> = requested.split_whitespace().collect();
    if tokens.iter().any(|token| {
        let name = token.strip_prefix('-').unwrap_or(token);
        !SUPPORTED_CAPS.contains(&name)
    }) {
        return false;
    }
    for token in tokens {
        if let Some(name) = token.strip_prefix('-') {
            enabled.disable(name);
        } else {
            enabled.enable(token);
        }
    }
    true
}

fn cap_reply(nick: &str, subcommand: &str, caps: String) -> Message {
    replies::server_msg(Command::Raw(
        "CAP".to_string(),
        vec![nick.to_string(), subcommand.to_string(), caps],
    ))
}

/// Drive the connection through registration. Returns `None` when the
/// connection ended without registering (rejected, quit, timeout).
async fn register(
    state: &State,
    framed: &mut IrcStream,
    peer_ip: IpAddr,
    auth_limiter: &IpRateLimiter,
) -> Result<Option<Registered>> {
    let deadline = Instant::now() + REGISTRATION_TIMEOUT;
    let mut pending = Pending::default();

    while !pending.ready() {
        let next = tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                send_all(framed, vec![replies::error("Registration timeout")]).await?;
                return Ok(None);
            }
            next = framed.next() => next,
        };
        let message = match next {
            Some(Ok(message)) => message,
            Some(Err(err)) => {
                tracing::debug!(error = %err, "ircd: undecodable line during registration");
                continue;
            }
            None => return Ok(None),
        };
        match message.command {
            Command::PASS(pass) => pending.pass = Some(pass),
            // The preferred nick is ignored: the nick is locked to the
            // late.sh username (FRD §5.3). We only track that NICK arrived.
            Command::NICK(_) => pending.nick_seen = true,
            Command::USER(_, _, _) => pending.user_seen = true,
            Command::CAP(_, CapSubCommand::LS, _, _) => {
                pending.cap_open = true;
                framed
                    .send(cap_reply("*", "LS", supported_cap_list()))
                    .await?;
            }
            Command::CAP(_, CapSubCommand::REQ, caps, trailing) => {
                let requested = caps.or(trailing).unwrap_or_default();
                let subcommand = if apply_cap_request(&mut pending.caps, &requested) {
                    "ACK"
                } else {
                    "NAK"
                };
                framed.send(cap_reply("*", subcommand, requested)).await?;
            }
            Command::CAP(_, CapSubCommand::LIST, _, _) => {
                framed
                    .send(cap_reply("*", "LIST", pending.caps.as_list()))
                    .await?;
            }
            Command::CAP(_, CapSubCommand::END, _, _) => pending.cap_open = false,
            Command::CAP(_, _, _, _) => {}
            Command::PING(token, _) => {
                framed
                    .send(replies::server_msg(Command::PONG(
                        SERVER_NAME.to_string(),
                        Some(token),
                    )))
                    .await?;
            }
            Command::QUIT(_) => {
                send_all(framed, vec![replies::error("Closing Link")]).await?;
                return Ok(None);
            }
            _ => {
                framed
                    .send(replies::numeric(
                        "*",
                        Response::ERR_NOTREGISTERED,
                        vec!["You have not registered".to_string()],
                    ))
                    .await?;
            }
        }
    }

    let outcome = match &pending.pass {
        Some(pass) => auth::authenticate(&state.db, pass, peer_ip).await?,
        None => AuthOutcome::BadToken,
    };
    match outcome {
        AuthOutcome::Ok { user, token_id: _ } => {
            let nick = user.username.clone();
            let is_admin = user.is_admin || state.config.force_admin;
            let registered = Registered {
                user_id: user.id,
                nick,
                is_admin,
                is_moderator: user.is_moderator,
                caps: pending.caps,
            };
            welcome(state, framed, &registered).await?;
            Ok(Some(registered))
        }
        AuthOutcome::BadToken | AuthOutcome::Banned => {
            let allowed = auth_limiter.allow(peer_ip);
            tokio::time::sleep(if allowed {
                AUTH_FAIL_DELAY
            } else {
                AUTH_FAIL_DELAY_LIMITED
            })
            .await;
            let detail = match outcome {
                AuthOutcome::Banned => replies::numeric(
                    "*",
                    Response::ERR_YOUREBANNEDCREEP,
                    vec!["You are banned from this server".to_string()],
                ),
                _ => replies::numeric(
                    "*",
                    Response::ERR_PASSWDMISMATCH,
                    vec![
                        "Invalid IRC token; mint one via ssh late.sh → Settings → Account"
                            .to_string(),
                    ],
                ),
            };
            send_all(
                framed,
                vec![detail, replies::error("Authentication failed")],
            )
            .await?;
            Ok(None)
        }
    }
}

async fn welcome(state: &State, framed: &mut IrcStream, registered: &Registered) -> Result<()> {
    let nick = registered.nick.as_str();
    let online = state.active_users.lock_recover().len() + state.irc_registry.connection_count();
    let mut burst = vec![
        replies::numeric(
            nick,
            Response::RPL_WELCOME,
            vec![format!("Welcome to the {NETWORK_NAME} IRC network, {nick}")],
        ),
        replies::numeric(
            nick,
            Response::RPL_YOURHOST,
            vec![format!(
                "Your host is {SERVER_NAME}, running {VERSION_STRING}"
            )],
        ),
        replies::numeric(
            nick,
            Response::RPL_CREATED,
            vec!["This server was created for computer people".to_string()],
        ),
        replies::numeric(
            nick,
            Response::RPL_MYINFO,
            vec![
                SERVER_NAME.to_string(),
                VERSION_STRING.to_string(),
                "o".to_string(),
                "bimnst".to_string(),
            ],
        ),
        replies::numeric(
            nick,
            Response::RPL_ISUPPORT,
            vec![
                format!("NETWORK={NETWORK_NAME}"),
                "CASEMAPPING=ascii".to_string(),
                "CHANTYPES=#".to_string(),
                "PREFIX=(o)@".to_string(),
                "CHANMODES=b,,,imnst".to_string(),
                "NICKLEN=32".to_string(),
                "CHANNELLEN=64".to_string(),
                "TOPICLEN=300".to_string(),
                "MODES=1".to_string(),
                "UTF8ONLY".to_string(),
                "are supported by this server".to_string(),
            ],
        ),
        replies::numeric(
            nick,
            Response::RPL_LUSERCLIENT,
            vec![format!("There are {online} users on 1 server")],
        ),
    ];
    burst.extend(motd_burst(nick, &state.config.web_url));
    send_all(framed, burst).await?;
    Ok(())
}

fn motd_burst(nick: &str, web_url: &str) -> Vec<Message> {
    let mut out = vec![replies::numeric(
        nick,
        Response::RPL_MOTDSTART,
        vec![format!("- {SERVER_NAME} Message of the day -")],
    )];
    for line in motd::motd_lines(web_url) {
        out.push(replies::numeric(
            nick,
            Response::RPL_MOTD,
            vec![format!("- {line}")],
        ));
    }
    out.push(replies::numeric(
        nick,
        Response::RPL_ENDOFMOTD,
        vec!["End of /MOTD command".to_string()],
    ));
    out
}

struct JoinedChannel {
    name: String,
    slug: String,
    is_lounge: bool,
}

struct DmPeer {
    peer_nick: String,
}

struct Session {
    state: State,
    user_id: Uuid,
    nick: String,
    is_admin: bool,
    // Read once the moderation mapping lands (KICK/MODE ±b gating).
    #[allow(dead_code)]
    is_moderator: bool,
    /// room_id → channel info, for rooms this connection has joined.
    joined: HashMap<Uuid, JoinedChannel>,
    /// normalized channel name → room_id.
    channels: HashMap<String, Uuid>,
    /// DM room_id → peer info, lazily resolved.
    dm_peers: HashMap<Uuid, DmPeer>,
    /// Targeted rooms already proven not to be DMs, to avoid a DB lookup for
    /// every private-channel event this IRC session has not joined.
    non_dm_target_rooms: HashSet<Uuid>,
    /// Authors ignored by this user. Applied to channel messages, not DMs.
    ignored_user_ids: HashSet<Uuid>,
    caps: IrcCapabilities,
    /// Bodies sent from this connection, for self-echo suppression.
    recent_sends: VecDeque<(Uuid, String)>,
    /// Recent post-auth expensive commands for per-connection abuse control.
    recent_commands: VecDeque<Instant>,
    last_rate_notice: Option<Instant>,
    /// Online users at the last presence poll, for JOIN/QUIT projection.
    last_online: HashSet<Uuid>,
}

impl Session {
    async fn run(
        &mut self,
        framed: &mut IrcStream,
        mut control_rx: mpsc::UnboundedReceiver<IrcControl>,
    ) -> Result<()> {
        let mut chat_events = self.state.chat_service.subscribe_events();
        let mut moderation_events = self.state.chat_service.subscribe_moderation_events();
        let mut ping_timer = tokio::time::interval(PING_INTERVAL);
        ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ping_timer.tick().await; // consume the immediate first tick
        let mut presence_timer = tokio::time::interval(PRESENCE_POLL_INTERVAL);
        presence_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        presence_timer.tick().await; // consume the immediate first tick
        let mut last_activity = Instant::now();

        self.force_join_lounge(framed).await?;
        self.last_online = self.online_user_ids();

        loop {
            tokio::select! {
                next = framed.next() => {
                    let message = match next {
                        Some(Ok(message)) => message,
                        Some(Err(err)) => {
                            tracing::debug!(error = %err, "ircd: undecodable line");
                            continue;
                        }
                        None => return Ok(()),
                    };
                    last_activity = Instant::now();
                    if !self.handle_command(framed, message).await? {
                        return Ok(());
                    }
                }
                event = chat_events.recv() => {
                    match event {
                        Ok(event) => self.project_chat_event(framed, event).await?,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "ircd: chat event receiver lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                event = moderation_events.recv() => {
                    match event {
                        Ok(event) => self.project_moderation_event(framed, event).await?,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "ircd: moderation event receiver lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                    }
                }
                control = control_rx.recv() => {
                    match control {
                        Some(IrcControl::Disconnect { reason }) => {
                            send_all(framed, vec![replies::error(reason)]).await?;
                            return Ok(());
                        }
                        None => return Ok(()),
                    }
                }
                _ = presence_timer.tick() => {
                    self.project_presence_changes(framed).await?;
                }
                _ = ping_timer.tick() => {
                    if last_activity.elapsed() > PONG_GRACE {
                        send_all(framed, vec![replies::error("Ping timeout")]).await?;
                        return Ok(());
                    }
                    framed
                        .send(replies::server_msg(Command::PING(
                            SERVER_NAME.to_string(),
                            None,
                        )))
                        .await?;
                }
            }
        }
    }

    /// Returns false when the connection should close.
    async fn handle_command(&mut self, framed: &mut IrcStream, message: Message) -> Result<bool> {
        if is_rate_limited_command(&message.command) && !self.allow_command() {
            if !matches!(message.command, Command::NOTICE(_, _)) && self.should_send_rate_notice() {
                framed
                    .send(replies::server_notice(
                        &self.nick,
                        "Slow down: IRC command rate limit exceeded",
                    ))
                    .await?;
            }
            return Ok(true);
        }
        match message.command {
            Command::PING(token, _) => {
                framed
                    .send(replies::server_msg(Command::PONG(
                        SERVER_NAME.to_string(),
                        Some(token),
                    )))
                    .await?;
            }
            Command::PONG(_, _) => {}
            Command::QUIT(_) => {
                send_all(framed, vec![replies::error("Closing Link")]).await?;
                return Ok(false);
            }
            Command::PRIVMSG(target, text) => {
                self.handle_privmsg(framed, &target, text, true).await?;
            }
            Command::NOTICE(target, text) => {
                // RFC: never generate error replies to NOTICE.
                self.handle_privmsg(framed, &target, text, false).await?;
            }
            Command::JOIN(chanlist, _, _) => {
                for name in chanlist.split(',').filter(|n| !n.is_empty()) {
                    self.handle_join(framed, name).await?;
                }
            }
            Command::PART(chanlist, _) => {
                for name in chanlist.split(',').filter(|n| !n.is_empty()) {
                    self.handle_part(framed, name).await?;
                }
            }
            Command::NICK(_) => {
                let mut out = vec![replies::numeric(
                    &self.nick,
                    Response::ERR_RESTRICTED,
                    vec!["Your nick is locked to your late.sh username".to_string()],
                )];
                out.push(replies::server_notice(
                    &self.nick,
                    "Nicks are locked to late.sh usernames; change it via the late.sh TUI",
                ));
                send_all(framed, out).await?;
            }
            Command::LIST(_, _) => self.handle_list(framed).await?,
            Command::NAMES(Some(chanlist), _) => {
                for name in chanlist.split(',').filter(|n| !n.is_empty()) {
                    self.send_names(framed, name).await?;
                }
            }
            Command::NAMES(None, _) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_ENDOFNAMES,
                        vec!["*".to_string(), "End of /NAMES list".to_string()],
                    ))
                    .await?;
            }
            Command::TOPIC(channel, None) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_NOTOPIC,
                        vec![channel, "No topic is set".to_string()],
                    ))
                    .await?;
            }
            Command::TOPIC(channel, Some(_)) => {
                // TODO(FRD §9.4): allow ops to set room topics once rooms
                // grow an editable topic concept.
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::ERR_CHANOPRIVSNEEDED,
                        vec![channel, "Topics are managed in the late.sh TUI".to_string()],
                    ))
                    .await?;
            }
            Command::WHO(Some(mask), _) => self.handle_who(framed, &mask).await?,
            Command::WHO(None, _) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_ENDOFWHO,
                        vec!["*".to_string(), "End of /WHO list".to_string()],
                    ))
                    .await?;
            }
            Command::WHOIS(_, masks) => self.handle_whois(framed, &masks).await?,
            Command::WHOWAS(nick, _, _) => {
                send_all(
                    framed,
                    vec![
                        replies::numeric(
                            &self.nick,
                            Response::ERR_WASNOSUCHNICK,
                            vec![nick.clone(), "There was no such nickname".to_string()],
                        ),
                        replies::numeric(
                            &self.nick,
                            Response::RPL_ENDOFWHOWAS,
                            vec![nick, "End of WHOWAS".to_string()],
                        ),
                    ],
                )
                .await?;
            }
            Command::ChannelMODE(channel, modes) => {
                self.handle_channel_mode(framed, &channel, modes).await?;
            }
            Command::UserMODE(_, modes) => {
                // User self-modes do nothing here (FRD §7.2); answer queries and
                // silently accept +i/-i.
                if modes.is_empty() {
                    framed
                        .send(replies::numeric(
                            &self.nick,
                            Response::RPL_UMODEIS,
                            vec!["+".to_string()],
                        ))
                        .await?;
                }
            }
            Command::MOTD(_) => {
                send_all(framed, motd_burst(&self.nick, &self.state.config.web_url)).await?;
            }
            Command::VERSION(_) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_VERSION,
                        vec![
                            VERSION_STRING.to_string(),
                            SERVER_NAME.to_string(),
                            String::new(),
                        ],
                    ))
                    .await?;
            }
            Command::TIME(_) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_TIME,
                        vec![SERVER_NAME.to_string(), chrono::Utc::now().to_rfc2822()],
                    ))
                    .await?;
            }
            Command::LUSERS(_, _) => {
                let online = self.state.active_users.lock_recover().len()
                    + self.state.irc_registry.connection_count();
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_LUSERCLIENT,
                        vec![format!("There are {online} users on 1 server")],
                    ))
                    .await?;
            }
            Command::USERHOST(nicks) => {
                let directory = usernames::snapshot(&self.state.username_directory);
                let replies_text: Vec<String> = nicks
                    .iter()
                    .filter_map(|nick| {
                        lookup_user_by_nick(&directory, nick)
                            .map(|(_, name)| format!("{name}=+{name}@{}", replies::USER_HOSTNAME))
                    })
                    .collect();
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_USERHOST,
                        vec![replies_text.join(" ")],
                    ))
                    .await?;
            }
            Command::ISON(nicks) => {
                let directory = usernames::snapshot(&self.state.username_directory);
                let online: Vec<String> = nicks
                    .iter()
                    .filter_map(|nick| lookup_user_by_nick(&directory, nick))
                    .filter(|(id, _)| self.is_user_online(*id))
                    .map(|(_, name)| name)
                    .collect();
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_ISON,
                        vec![online.join(" ")],
                    ))
                    .await?;
            }
            Command::AWAY(Some(_)) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_NOWAWAY,
                        vec!["You have been marked as being away".to_string()],
                    ))
                    .await?;
            }
            Command::AWAY(None) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::RPL_UNAWAY,
                        vec!["You are no longer marked as being away".to_string()],
                    ))
                    .await?;
            }
            Command::OPER(_, _) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::ERR_NOOPERHOST,
                        vec!["Operator status comes from your late.sh admin tier".to_string()],
                    ))
                    .await?;
            }
            Command::INVITE(_, _) => {
                framed
                    .send(replies::server_notice(
                        &self.nick,
                        "INVITE is not supported yet; manage private rooms in the late.sh TUI",
                    ))
                    .await?;
            }
            Command::KICK(channel, users, reason) => {
                self.handle_kick(framed, &channel, &users, reason.as_deref())
                    .await?;
            }
            Command::KILL(nick, reason) => {
                self.handle_kill(framed, &nick, &reason).await?;
            }
            Command::CAP(_, CapSubCommand::LS, _, _) => {
                framed
                    .send(cap_reply(&self.nick, "LS", supported_cap_list()))
                    .await?;
            }
            Command::CAP(_, CapSubCommand::LIST, _, _) => {
                framed
                    .send(cap_reply(&self.nick, "LIST", self.caps.as_list()))
                    .await?;
            }
            Command::CAP(_, CapSubCommand::REQ, caps, trailing) => {
                let requested = caps.or(trailing).unwrap_or_default();
                let subcommand = if apply_cap_request(&mut self.caps, &requested) {
                    "ACK"
                } else {
                    "NAK"
                };
                framed
                    .send(cap_reply(&self.nick, subcommand, requested))
                    .await?;
            }
            Command::CAP(_, _, _, _) => {}
            Command::Raw(cmd, _) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::ERR_UNKNOWNCOMMAND,
                        vec![cmd, "Unknown command".to_string()],
                    ))
                    .await?;
            }
            other => {
                let cmd = String::from(&other);
                let cmd = cmd.split(' ').next().unwrap_or("?").to_string();
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::ERR_UNKNOWNCOMMAND,
                        vec![cmd, "Unknown command".to_string()],
                    ))
                    .await?;
            }
        }
        Ok(true)
    }

    async fn handle_privmsg(
        &mut self,
        framed: &mut IrcStream,
        target: &str,
        text: String,
        reply_errors: bool,
    ) -> Result<()> {
        let body = match proj::parse_ctcp_action(&text) {
            Some(action) => proj::action_to_body(action),
            None if text.starts_with('\u{1}') => {
                // Non-ACTION CTCP: answer VERSION/PING minimally, drop the rest.
                return self.handle_ctcp(framed, target, &text).await;
            }
            None => text,
        };
        if target.starts_with('#') {
            let Some((room_id, _)) = self.authorized_joined_channel(target).await? else {
                if reply_errors {
                    framed
                        .send(replies::numeric(
                            &self.nick,
                            Response::ERR_CANNOTSENDTOCHAN,
                            vec![
                                target.to_string(),
                                "You are not in that channel".to_string(),
                            ],
                        ))
                        .await?;
                }
                return Ok(());
            };
            let slug = self.joined.get(&room_id).map(|c| c.slug.clone());
            self.recent_sends.push_back((room_id, body.clone()));
            while self.recent_sends.len() > RECENT_SENDS_MAX {
                self.recent_sends.pop_front();
            }
            self.state.chat_service.send_message_task(
                self.user_id,
                room_id,
                slug,
                body,
                Uuid::new_v4(),
                self.is_admin,
            );
        } else {
            let directory = usernames::snapshot(&self.state.username_directory);
            let Some((target_id, _)) = lookup_user_by_nick(&directory, target) else {
                if reply_errors {
                    framed
                        .send(replies::numeric(
                            &self.nick,
                            Response::ERR_NOSUCHNICK,
                            vec![target.to_string(), "No such nick".to_string()],
                        ))
                        .await?;
                }
                return Ok(());
            };
            let client = self.state.db.get().await?;
            let room = ChatRoom::get_or_create_dm(&client, self.user_id, target_id).await?;
            self.dm_peers.insert(
                room.id,
                DmPeer {
                    peer_nick: target.to_string(),
                },
            );
            self.state.chat_service.send_message_task(
                self.user_id,
                room.id,
                None,
                body,
                Uuid::new_v4(),
                self.is_admin,
            );
        }
        Ok(())
    }

    async fn handle_ctcp(&self, framed: &mut IrcStream, _target: &str, text: &str) -> Result<()> {
        let inner = text.trim_matches('\u{1}');
        let mut parts = inner.splitn(2, ' ');
        let kind = parts.next().unwrap_or_default();
        match kind {
            "VERSION" => {
                framed
                    .send(replies::server_notice(
                        &self.nick,
                        format!("\u{1}VERSION {VERSION_STRING}\u{1}"),
                    ))
                    .await?;
            }
            "PING" => {
                let arg = parts.next().unwrap_or_default();
                framed
                    .send(replies::server_notice(
                        &self.nick,
                        format!("\u{1}PING {arg}\u{1}"),
                    ))
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn force_join_lounge(&mut self, framed: &mut IrcStream) -> Result<()> {
        let client = self.state.db.get().await?;
        let Some(lounge) = ChatRoom::find_lounge(&client).await? else {
            tracing::warn!("ircd: no lounge room found; skipping forced join");
            return Ok(());
        };
        let name = proj::channel_name(&lounge).unwrap_or_else(|| "#lounge".to_string());
        if let Err(err) = ChatRoomMember::join(&client, lounge.id, self.user_id).await {
            tracing::debug!(error = %err, "ircd: forced lounge join refused");
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_BANNEDFROMCHAN,
                    vec![name, "Cannot join channel".to_string()],
                ))
                .await?;
            return Ok(());
        }
        drop(client);
        self.join_room(framed, &lounge).await
    }

    async fn handle_join(&mut self, framed: &mut IrcStream, name: &str) -> Result<()> {
        if self.channels.contains_key(&proj::normalize_channel(name)) {
            return Ok(()); // already joined; ignore
        }
        let Some(slug) = proj::slug_for_channel(name) else {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_NOSUCHCHANNEL,
                    vec![name.to_string(), "No such channel".to_string()],
                ))
                .await?;
            return Ok(());
        };
        let client = self.state.db.get().await?;
        let room = ChatRoom::find_irc_channel_by_slug_for_user(&client, slug, self.user_id).await?;
        let Some(room) = room.filter(proj::is_irc_channel_kind) else {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_NOSUCHCHANNEL,
                    vec![name.to_string(), "No such channel".to_string()],
                ))
                .await?;
            return Ok(());
        };
        if room.visibility == "private"
            && !ChatRoomMember::is_member(&client, room.id, self.user_id).await?
        {
            // Private channels present as invite-only (FRD §6.3 J4).
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_INVITEONLYCHAN,
                    vec![name.to_string(), "Cannot join channel (+i)".to_string()],
                ))
                .await?;
            return Ok(());
        }
        if ChatRoomMember::is_banned_from_room(&client, room.id, self.user_id).await? {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_BANNEDFROMCHAN,
                    vec![name.to_string(), "Cannot join channel (+b)".to_string()],
                ))
                .await?;
            return Ok(());
        }
        if let Err(err) = ChatRoomMember::join(&client, room.id, self.user_id).await {
            tracing::debug!(error = %err, slug, "ircd: join refused");
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_BANNEDFROMCHAN,
                    vec![name.to_string(), "Cannot join channel".to_string()],
                ))
                .await?;
            return Ok(());
        }
        drop(client);
        self.join_room(framed, &room).await
    }

    /// Record the join and send the JOIN burst (JOIN + NAMES).
    async fn join_room(&mut self, framed: &mut IrcStream, room: &ChatRoom) -> Result<()> {
        let Some(name) = proj::channel_name(room) else {
            return Ok(());
        };
        let slug = room.slug.clone().unwrap_or_default();
        self.channels
            .insert(proj::normalize_channel(&name), room.id);
        self.joined.insert(
            room.id,
            JoinedChannel {
                name: name.clone(),
                slug,
                is_lounge: proj::is_lounge(room),
            },
        );
        framed
            .send(replies::from_user(
                &self.nick,
                Command::JOIN(name.clone(), None, None),
            ))
            .await?;
        self.send_names(framed, &name).await
    }

    async fn handle_part(&mut self, framed: &mut IrcStream, name: &str) -> Result<()> {
        let normalized = proj::normalize_channel(name);
        let Some(room_id) = self.channels.get(&normalized).copied() else {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_NOTONCHANNEL,
                    vec![name.to_string(), "You're not on that channel".to_string()],
                ))
                .await?;
            return Ok(());
        };
        let is_lounge = self.joined.get(&room_id).is_some_and(|c| c.is_lounge);
        if is_lounge {
            // Sticky join (FRD §6.3 J2): refuse, and always re-send the JOIN
            // burst so tab-close clients converge back to a joined lounge.
            let channel_name = self.joined.get(&room_id).map(|c| c.name.clone());
            send_all(
                framed,
                vec![
                    replies::numeric(
                        &self.nick,
                        Response::ERR_RESTRICTED,
                        vec![name.to_string(), "You cannot leave the lounge".to_string()],
                    ),
                    replies::server_notice(
                        &self.nick,
                        "Everyone stays in #lounge; it's the late.sh living room",
                    ),
                ],
            )
            .await?;
            if let Some(channel_name) = channel_name {
                framed
                    .send(replies::from_user(
                        &self.nick,
                        Command::JOIN(channel_name.clone(), None, None),
                    ))
                    .await?;
                self.send_names(framed, &channel_name).await?;
            }
            return Ok(());
        }
        // v1: PART detaches the IRC view only; room membership is unchanged
        // (FRD §6.3 J6).
        self.channels.remove(&normalized);
        let channel = self.joined.remove(&room_id);
        framed
            .send(replies::from_user(
                &self.nick,
                Command::PART(
                    channel.map(|c| c.name).unwrap_or_else(|| name.to_string()),
                    None,
                ),
            ))
            .await?;
        Ok(())
    }

    async fn handle_list(&mut self, framed: &mut IrcStream) -> Result<()> {
        let client = self.state.db.get().await?;
        let rooms = ChatRoom::list_irc_channel_summaries(&client, self.user_id).await?;
        let mut out = vec![replies::numeric(
            &self.nick,
            Response::RPL_LISTSTART,
            vec!["Channel".to_string(), "Users  Name".to_string()],
        )];
        for (room, count) in &rooms {
            let Some(name) = proj::channel_name(room) else {
                continue;
            };
            out.push(replies::numeric(
                &self.nick,
                Response::RPL_LIST,
                vec![name, count.to_string(), String::new()],
            ));
        }
        out.push(replies::numeric(
            &self.nick,
            Response::RPL_LISTEND,
            vec!["End of /LIST".to_string()],
        ));
        send_all(framed, out).await?;
        Ok(())
    }

    /// 353/366 burst for a channel. Lists currently-online room members
    /// (FRD §6.4 P1), with @ for mods/admins.
    async fn send_names(&mut self, framed: &mut IrcStream, name: &str) -> Result<()> {
        let Some((room_id, channel_name)) = self.authorized_joined_channel(name).await? else {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::RPL_ENDOFNAMES,
                    vec![name.to_string(), "End of /NAMES list".to_string()],
                ))
                .await?;
            return Ok(());
        };
        let client = self.state.db.get().await?;
        let member_ids = ChatRoomMember::list_user_ids(&client, room_id).await?;
        let online: Vec<Uuid> = member_ids
            .into_iter()
            .filter(|id| *id == self.user_id || self.is_user_online(*id))
            .collect();
        let staff = User::staff_flags_by_ids(&client, &online).await?;
        drop(client);
        let directory = usernames::snapshot(&self.state.username_directory);
        let mut names: Vec<String> = online
            .iter()
            .filter_map(|id| {
                let username = directory.get(id)?;
                let is_staff = staff.contains_key(id);
                Some(if is_staff {
                    format!("@{username}")
                } else {
                    username.clone()
                })
            })
            .collect();
        names.sort();
        let mut out = Vec::new();
        // ~10 names per 353 line keeps lines comfortably under 512 bytes.
        for chunk in names.chunks(10) {
            out.push(replies::numeric(
                &self.nick,
                Response::RPL_NAMREPLY,
                vec!["=".to_string(), channel_name.clone(), chunk.join(" ")],
            ));
        }
        out.push(replies::numeric(
            &self.nick,
            Response::RPL_ENDOFNAMES,
            vec![channel_name, "End of /NAMES list".to_string()],
        ));
        send_all(framed, out).await?;
        Ok(())
    }

    async fn handle_who(&mut self, framed: &mut IrcStream, mask: &str) -> Result<()> {
        let mut out = Vec::new();
        if mask.starts_with('#') {
            if let Some((room_id, _)) = self.authorized_joined_channel(mask).await? {
                let client = self.state.db.get().await?;
                let member_ids = ChatRoomMember::list_user_ids(&client, room_id).await?;
                let online: Vec<Uuid> = member_ids
                    .into_iter()
                    .filter(|id| *id == self.user_id || self.is_user_online(*id))
                    .collect();
                let staff = User::staff_flags_by_ids(&client, &online).await?;
                drop(client);
                let directory = usernames::snapshot(&self.state.username_directory);
                for id in online {
                    let Some(username) = directory.get(&id) else {
                        continue;
                    };
                    let flags = if staff.contains_key(&id) { "H@" } else { "H" };
                    out.push(replies::numeric(
                        &self.nick,
                        Response::RPL_WHOREPLY,
                        vec![
                            mask.to_string(),
                            username.clone(),
                            replies::USER_HOSTNAME.to_string(),
                            SERVER_NAME.to_string(),
                            username.clone(),
                            flags.to_string(),
                            format!("0 {username}"),
                        ],
                    ));
                }
            }
        } else {
            let directory = usernames::snapshot(&self.state.username_directory);
            if let Some((id, username)) = lookup_user_by_nick(&directory, mask)
                && (self.is_user_online(id) || id == self.user_id)
            {
                out.push(replies::numeric(
                    &self.nick,
                    Response::RPL_WHOREPLY,
                    vec![
                        "*".to_string(),
                        username.clone(),
                        replies::USER_HOSTNAME.to_string(),
                        SERVER_NAME.to_string(),
                        username.clone(),
                        "H".to_string(),
                        format!("0 {username}"),
                    ],
                ));
            }
        }
        out.push(replies::numeric(
            &self.nick,
            Response::RPL_ENDOFWHO,
            vec![mask.to_string(), "End of /WHO list".to_string()],
        ));
        send_all(framed, out).await?;
        Ok(())
    }

    async fn handle_whois(&mut self, framed: &mut IrcStream, masks: &str) -> Result<()> {
        let target = masks.split(',').next().unwrap_or_default();
        let directory = usernames::snapshot(&self.state.username_directory);
        let Some((id, username)) = lookup_user_by_nick(&directory, target) else {
            send_all(
                framed,
                vec![
                    replies::numeric(
                        &self.nick,
                        Response::ERR_NOSUCHNICK,
                        vec![target.to_string(), "No such nick".to_string()],
                    ),
                    replies::numeric(
                        &self.nick,
                        Response::RPL_ENDOFWHOIS,
                        vec![target.to_string(), "End of /WHOIS list".to_string()],
                    ),
                ],
            )
            .await?;
            return Ok(());
        };
        let client = self.state.db.get().await?;
        let staff = User::staff_flags_by_ids(&client, &[id]).await?;
        drop(client);
        let mut out = vec![
            replies::numeric(
                &self.nick,
                Response::RPL_WHOISUSER,
                vec![
                    username.clone(),
                    username.clone(),
                    replies::USER_HOSTNAME.to_string(),
                    "*".to_string(),
                    username.clone(),
                ],
            ),
            replies::numeric(
                &self.nick,
                Response::RPL_WHOISSERVER,
                vec![
                    username.clone(),
                    SERVER_NAME.to_string(),
                    NETWORK_NAME.to_string(),
                ],
            ),
        ];
        if staff.get(&id).is_some_and(|(is_admin, _)| *is_admin) {
            out.push(replies::numeric(
                &self.nick,
                Response::RPL_WHOISOPERATOR,
                vec![username.clone(), "is an IRC operator".to_string()],
            ));
        }
        out.push(replies::numeric(
            &self.nick,
            Response::RPL_ENDOFWHOIS,
            vec![username, "End of /WHOIS list".to_string()],
        ));
        send_all(framed, out).await?;
        Ok(())
    }

    async fn handle_channel_mode(
        &mut self,
        framed: &mut IrcStream,
        channel: &str,
        modes: Vec<Mode<ChannelMode>>,
    ) -> Result<()> {
        let Some((room_id, _)) = self.authorized_joined_channel(channel).await? else {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_NOSUCHCHANNEL,
                    vec![channel.to_string(), "No such channel".to_string()],
                ))
                .await?;
            return Ok(());
        };
        if modes.is_empty() {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::RPL_CHANNELMODEIS,
                    vec![channel.to_string(), "+nt".to_string()],
                ))
                .await?;
            return Ok(());
        }

        let mut refused = false;
        for mode in modes {
            match mode {
                Mode::NoPrefix(ChannelMode::Ban) => {
                    self.send_banlist(framed, channel, room_id).await?;
                }
                Mode::Plus(ChannelMode::Ban, Some(mask)) => {
                    self.handle_ban_mode(framed, channel, &mask, true).await?;
                }
                Mode::Minus(ChannelMode::Ban, Some(mask)) => {
                    self.handle_ban_mode(framed, channel, &mask, false).await?;
                }
                Mode::Plus(ChannelMode::InviteOnly, _)
                | Mode::Minus(ChannelMode::InviteOnly, _)
                | Mode::Plus(ChannelMode::Unknown('i'), _)
                | Mode::Minus(ChannelMode::Unknown('i'), _) => {}
                _ => refused = true,
            }
        }

        if refused {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_CHANOPRIVSNEEDED,
                    vec![
                        channel.to_string(),
                        "Channel modes are tied to late.sh moderation tiers".to_string(),
                    ],
                ))
                .await?;
        }
        Ok(())
    }

    async fn handle_kick(
        &mut self,
        framed: &mut IrcStream,
        channel: &str,
        users: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        if !self.is_channel_op() {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_CHANOPRIVSNEEDED,
                    vec![
                        channel.to_string(),
                        "You're not channel operator".to_string(),
                    ],
                ))
                .await?;
            return Ok(());
        }
        if !self
            .channels
            .contains_key(&proj::normalize_channel(channel))
        {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_NOSUCHCHANNEL,
                    vec![channel.to_string(), "No such channel".to_string()],
                ))
                .await?;
            return Ok(());
        }
        let reason = reason.unwrap_or("IRC KICK");
        for nick in users.split(',').filter(|nick| !nick.trim().is_empty()) {
            let command = format!("kick {channel} @{} {reason}", nick.trim());
            self.run_moderation_command(framed, channel, &command)
                .await?;
        }
        Ok(())
    }

    async fn handle_kill(
        &mut self,
        framed: &mut IrcStream,
        nick: &str,
        reason: &str,
    ) -> Result<()> {
        if !self.is_admin {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_NOPRIVILEGES,
                    vec!["Permission Denied- You're not an IRC operator".to_string()],
                ))
                .await?;
            return Ok(());
        }
        let command = format!("kick server @{nick} {reason}");
        self.run_moderation_command(framed, nick, &command).await
    }

    async fn handle_ban_mode(
        &mut self,
        framed: &mut IrcStream,
        channel: &str,
        mask: &str,
        add: bool,
    ) -> Result<()> {
        if !self.is_channel_op() {
            framed
                .send(replies::numeric(
                    &self.nick,
                    Response::ERR_CHANOPRIVSNEEDED,
                    vec![
                        channel.to_string(),
                        "You're not channel operator".to_string(),
                    ],
                ))
                .await?;
            return Ok(());
        }
        let Some(nick) = nick_from_ban_mask(mask) else {
            framed
                .send(replies::server_notice(
                    &self.nick,
                    "Only nick!*@* ban masks are supported by late.sh",
                ))
                .await?;
            return Ok(());
        };
        let command = if add {
            format!("ban {channel} @{nick} IRC MODE +b")
        } else {
            format!("unban {channel} @{nick} IRC MODE -b")
        };
        self.run_moderation_command(framed, channel, &command).await
    }

    async fn send_banlist(
        &mut self,
        framed: &mut IrcStream,
        channel: &str,
        room_id: Uuid,
    ) -> Result<()> {
        let client = self.state.db.get().await?;
        let bans = RoomBan::active_for_room_with_usernames(&client, room_id, 200).await?;
        drop(client);
        let mut out = Vec::new();
        for item in bans {
            let target = item
                .target_username
                .unwrap_or_else(|| item.ban.target_user_id.to_string());
            let actor = item
                .actor_username
                .unwrap_or_else(|| SERVER_NAME.to_string());
            out.push(replies::numeric(
                &self.nick,
                Response::RPL_BANLIST,
                vec![channel.to_string(), format!("{target}!*@*"), actor],
            ));
        }
        out.push(replies::numeric(
            &self.nick,
            Response::RPL_ENDOFBANLIST,
            vec![channel.to_string(), "End of channel ban list".to_string()],
        ));
        send_all(framed, out).await
    }

    async fn run_moderation_command(
        &mut self,
        framed: &mut IrcStream,
        context: &str,
        command: &str,
    ) -> Result<()> {
        let permissions = self.reload_permissions().await?;
        match self
            .state
            .chat_service
            .run_mod_command(self.user_id, permissions, command)
            .await
        {
            Ok(lines) => {
                for line in lines {
                    framed
                        .send(replies::server_notice(&self.nick, &line))
                        .await?;
                }
            }
            Err(err) => {
                framed
                    .send(replies::numeric(
                        &self.nick,
                        Response::ERR_CHANOPRIVSNEEDED,
                        vec![context.to_string(), err.to_string()],
                    ))
                    .await?;
            }
        }
        Ok(())
    }

    async fn project_moderation_event(
        &mut self,
        framed: &mut IrcStream,
        event: ModerationEvent,
    ) -> Result<()> {
        match event {
            ModerationEvent::RoomAction {
                actor_user_id,
                target_user_id,
                room_id,
                action,
                reason,
                ..
            } if matches!(
                action,
                RoomModAction::Kick | RoomModAction::Ban | RoomModAction::Unban
            ) =>
            {
                let Some(channel) = self.joined.get(&room_id).map(|joined| joined.name.clone())
                else {
                    return Ok(());
                };
                let directory = usernames::snapshot(&self.state.username_directory);
                let actor = directory
                    .get(&actor_user_id)
                    .cloned()
                    .unwrap_or_else(|| SERVER_NAME.to_string());
                let target = directory
                    .get(&target_user_id)
                    .cloned()
                    .unwrap_or_else(|| target_user_id.to_string());
                match action {
                    RoomModAction::Kick => {
                        framed
                            .send(replies::from_user(
                                &actor,
                                Command::KICK(channel.clone(), target, Some(reason)),
                            ))
                            .await?;
                        if target_user_id == self.user_id {
                            self.channels.remove(&proj::normalize_channel(&channel));
                            self.joined.remove(&room_id);
                        }
                    }
                    RoomModAction::Ban => {
                        send_all(
                            framed,
                            vec![
                                replies::server_msg(Command::ChannelMODE(
                                    channel.clone(),
                                    vec![Mode::Plus(
                                        ChannelMode::Ban,
                                        Some(format!("{target}!*@*")),
                                    )],
                                )),
                                replies::from_user(
                                    &actor,
                                    Command::KICK(channel.clone(), target, Some(reason)),
                                ),
                            ],
                        )
                        .await?;
                        if target_user_id == self.user_id {
                            self.channels.remove(&proj::normalize_channel(&channel));
                            self.joined.remove(&room_id);
                        }
                    }
                    RoomModAction::Unban => {
                        framed
                            .send(replies::server_msg(Command::ChannelMODE(
                                channel,
                                vec![Mode::Minus(ChannelMode::Ban, Some(format!("{target}!*@*")))],
                            )))
                            .await?;
                    }
                }
            }
            ModerationEvent::RoleAction {
                target_user_id,
                action,
                permissions,
                ..
            } => {
                let directory = usernames::snapshot(&self.state.username_directory);
                let Some(target) = directory.get(&target_user_id).cloned() else {
                    return Ok(());
                };
                if target_user_id == self.user_id {
                    self.is_admin = permissions.is_admin();
                    self.is_moderator = permissions.is_moderator();
                }
                let mode = match action {
                    RoleAction::GrantMod => Mode::Plus(ChannelMode::Oper, Some(target)),
                    RoleAction::RevokeMod => Mode::Minus(ChannelMode::Oper, Some(target)),
                };
                let messages = self
                    .joined
                    .values()
                    .map(|channel| {
                        replies::server_msg(Command::ChannelMODE(
                            channel.name.clone(),
                            vec![mode.clone()],
                        ))
                    })
                    .collect();
                send_all(framed, messages).await?;
            }
            _ => {}
        }
        Ok(())
    }

    fn permissions(&self) -> Permissions {
        Permissions::new(self.is_admin, self.is_moderator)
    }

    async fn reload_permissions(&mut self) -> Result<Permissions> {
        let client = self.state.db.get().await?;
        let Some(user) = User::get(&client, self.user_id).await? else {
            anyhow::bail!("user no longer exists");
        };
        self.is_admin = user.is_admin || self.state.config.force_admin;
        self.is_moderator = user.is_moderator;
        Ok(self.permissions())
    }

    fn is_channel_op(&self) -> bool {
        self.is_admin || self.is_moderator
    }

    fn allow_command(&mut self) -> bool {
        let now = Instant::now();
        while self
            .recent_commands
            .front()
            .is_some_and(|at| now.duration_since(*at) > COMMAND_RATE_WINDOW)
        {
            self.recent_commands.pop_front();
        }
        if self.recent_commands.len() >= COMMAND_RATE_MAX {
            return false;
        }
        self.recent_commands.push_back(now);
        true
    }

    fn should_send_rate_notice(&mut self) -> bool {
        let now = Instant::now();
        let should_send = match self.last_rate_notice {
            Some(last) => now.duration_since(last) > COMMAND_RATE_WINDOW,
            None => true,
        };
        if should_send {
            self.last_rate_notice = Some(now);
        }
        should_send
    }

    async fn authorized_joined_channel(&mut self, name: &str) -> Result<Option<(Uuid, String)>> {
        let normalized = proj::normalize_channel(name);
        let Some(room_id) = self.channels.get(&normalized).copied() else {
            return Ok(None);
        };
        let Some(channel_name) = self
            .joined
            .get(&room_id)
            .map(|channel| channel.name.clone())
        else {
            self.channels.remove(&normalized);
            return Ok(None);
        };

        let client = self.state.db.get().await?;
        let Some(room) = ChatRoom::get(&client, room_id).await? else {
            drop(client);
            self.forget_joined_channel(room_id, &channel_name);
            return Ok(None);
        };
        let is_allowed_private = room.visibility != "private"
            || ChatRoomMember::is_member(&client, room_id, self.user_id).await?;
        let is_banned = ChatRoomMember::is_banned_from_room(&client, room_id, self.user_id).await?;
        if !proj::is_irc_channel_kind(&room) || !is_allowed_private || is_banned {
            drop(client);
            self.forget_joined_channel(room_id, &channel_name);
            return Ok(None);
        }

        Ok(Some((room_id, channel_name)))
    }

    fn forget_joined_channel(&mut self, room_id: Uuid, channel_name: &str) {
        self.channels.remove(&proj::normalize_channel(channel_name));
        self.joined.remove(&room_id);
    }

    async fn project_chat_event(&mut self, framed: &mut IrcStream, event: ChatEvent) -> Result<()> {
        match event {
            ChatEvent::MessageCreated {
                message,
                target_user_ids,
                author_username,
                ..
            } => {
                self.project_message(framed, message, target_user_ids, author_username, false)
                    .await?;
            }
            ChatEvent::MessageEdited {
                message,
                target_user_ids,
                author_username,
                ..
            } => {
                self.project_message(framed, message, target_user_ids, author_username, true)
                    .await?;
            }
            ChatEvent::IgnoreListUpdated {
                user_id,
                ignored_user_ids,
                ..
            } if user_id == self.user_id => {
                self.ignored_user_ids = ignored_user_ids.into_iter().collect();
            }
            _ => {}
        }
        Ok(())
    }

    async fn project_message(
        &mut self,
        framed: &mut IrcStream,
        message: late_core::models::chat_message::ChatMessage,
        target_user_ids: Option<Vec<Uuid>>,
        author_username: Option<String>,
        is_edit: bool,
    ) -> Result<()> {
        if let Some(channel) = self.joined.get(&message.room_id) {
            if target_user_ids
                .as_ref()
                .is_some_and(|targets| !targets.contains(&self.user_id))
            {
                let channel_name = channel.name.clone();
                self.forget_joined_channel(message.room_id, &channel_name);
                return Ok(());
            }
            if message.user_id != self.user_id && self.ignored_user_ids.contains(&message.user_id) {
                return Ok(());
            }
            if message.user_id == self.user_id && !is_edit && !self.caps.echo_message {
                // Self-echo suppression: skip exactly one copy of a body this
                // connection sent; copies from the TUI or other connections
                // still flow (bouncer behavior, FRD §5.4 M3).
                if let Some(pos) = self
                    .recent_sends
                    .iter()
                    .position(|(room, body)| *room == message.room_id && *body == message.body)
                {
                    self.recent_sends.remove(pos);
                    return Ok(());
                }
            }
            let author = match author_username {
                Some(author) => author,
                None => {
                    let directory = usernames::snapshot(&self.state.username_directory);
                    match directory.get(&message.user_id) {
                        Some(name) => name.clone(),
                        None => return Ok(()),
                    }
                }
            };
            let channel_name = channel.name.clone();
            self.deliver_privmsg(framed, &author, &channel_name, &message, is_edit)
                .await?;
            return Ok(());
        }

        // Not a joined channel: deliver DMs addressed to us (FRD §8 S2).
        let targets = target_user_ids.unwrap_or_default();
        if !targets.contains(&self.user_id) || message.user_id == self.user_id {
            return Ok(());
        }
        if self.non_dm_target_rooms.contains(&message.room_id) {
            return Ok(());
        }
        if !self.dm_peers.contains_key(&message.room_id) {
            let client = self.state.db.get().await?;
            let Some(room) = ChatRoom::get(&client, message.room_id).await? else {
                return Ok(());
            };
            if room.kind != "dm" {
                self.non_dm_target_rooms.insert(message.room_id);
                return Ok(());
            }
            drop(client);
            let directory = usernames::snapshot(&self.state.username_directory);
            let Some(peer_nick) = directory.get(&message.user_id).cloned() else {
                return Ok(());
            };
            self.dm_peers.insert(message.room_id, DmPeer { peer_nick });
        }
        let Some(peer) = self.dm_peers.get(&message.room_id) else {
            return Ok(());
        };
        let author = peer.peer_nick.clone();
        let target = self.nick.clone();
        self.deliver_privmsg(framed, &author, &target, &message, is_edit)
            .await?;
        Ok(())
    }

    async fn deliver_privmsg(
        &mut self,
        framed: &mut IrcStream,
        author: &str,
        target: &str,
        message: &late_core::models::chat_message::ChatMessage,
        is_edit: bool,
    ) -> Result<()> {
        let mut lines = proj::split_body(&message.body, proj::PRIVMSG_CHUNK_BYTES);
        if is_edit {
            lines = lines
                .into_iter()
                .map(|line| format!("[edit] {line}"))
                .collect();
        }
        let messages: Vec<Message> = lines
            .into_iter()
            .enumerate()
            .map(|(idx, line)| {
                replies::from_user_with_tags(
                    author,
                    Command::PRIVMSG(target.to_string(), line),
                    self.privmsg_tags(message, idx == 0, is_edit),
                )
            })
            .collect();
        send_all(framed, messages).await?;
        Ok(())
    }

    fn privmsg_tags(
        &self,
        message: &late_core::models::chat_message::ChatMessage,
        first_line: bool,
        is_edit: bool,
    ) -> Vec<Tag> {
        let mut tags = Vec::new();
        if self.caps.server_time {
            tags.push(Tag(
                "time".to_string(),
                Some(proj::server_time(message.created)),
            ));
        }
        if self.caps.message_tags && first_line && !is_edit {
            tags.push(Tag("msgid".to_string(), Some(proj::msgid(message.id))));
            if let Some(reply_to_message_id) = message.reply_to_message_id {
                tags.push(Tag(
                    "+reply".to_string(),
                    Some(proj::msgid(reply_to_message_id)),
                ));
            }
        }
        tags
    }

    fn is_user_online(&self, user_id: Uuid) -> bool {
        self.state
            .active_users
            .lock_recover()
            .contains_key(&user_id)
            || self.state.irc_registry.is_online(user_id)
    }

    fn online_user_ids(&self) -> std::collections::HashSet<Uuid> {
        let mut online: std::collections::HashSet<Uuid> = self
            .state
            .active_users
            .lock_recover()
            .keys()
            .copied()
            .collect();
        online.extend(self.state.irc_registry.online_user_ids());
        online
    }

    /// Diff the online-user set and emit QUIT for departures plus JOIN (per
    /// shared channel) for arrivals, so clients' member lists track late.sh
    /// presence (FRD §6.4 P2).
    async fn project_presence_changes(&mut self, framed: &mut IrcStream) -> Result<()> {
        let now_online = self.online_user_ids();
        let directory = usernames::snapshot(&self.state.username_directory);
        let mut out = Vec::new();

        for departed in self.last_online.difference(&now_online) {
            if *departed == self.user_id {
                continue;
            }
            if let Some(nick) = directory.get(departed) {
                out.push(replies::from_user(
                    nick,
                    Command::QUIT(Some("left late.sh".to_string())),
                ));
            }
        }

        let arrivals: Vec<Uuid> = now_online
            .difference(&self.last_online)
            .filter(|id| **id != self.user_id)
            .copied()
            .collect();
        if !arrivals.is_empty() && !self.joined.is_empty() {
            let client = self.state.db.get().await?;
            let joined_room_ids: Vec<Uuid> = self.joined.keys().copied().collect();
            let memberships = ChatRoomMember::list_memberships_for_users_in_rooms(
                &client,
                &arrivals,
                &joined_room_ids,
            )
            .await?;
            for (arrived, room_id) in memberships {
                let Some(nick) = directory.get(&arrived).cloned() else {
                    continue;
                };
                if let Some(channel) = self.joined.get(&room_id) {
                    out.push(replies::from_user(
                        &nick,
                        Command::JOIN(channel.name.clone(), None, None),
                    ));
                }
            }
        }

        self.last_online = now_online;
        send_all(framed, out).await?;
        Ok(())
    }
}

fn is_rate_limited_command(command: &Command) -> bool {
    matches!(
        command,
        Command::PRIVMSG(_, _)
            | Command::NOTICE(_, _)
            | Command::JOIN(_, _, _)
            | Command::PART(_, _)
            | Command::LIST(_, _)
            | Command::NAMES(_, _)
            | Command::TOPIC(_, _)
            | Command::WHO(_, _)
            | Command::WHOIS(_, _)
            | Command::WHOWAS(_, _, _)
            | Command::ChannelMODE(_, _)
            | Command::LUSERS(_, _)
            | Command::USERHOST(_)
            | Command::ISON(_)
            | Command::KICK(_, _, _)
            | Command::KILL(_, _)
    )
}

fn nick_from_ban_mask(mask: &str) -> Option<&str> {
    let (nick, hostmask) = mask.split_once('!')?;
    if hostmask != "*@*" {
        return None;
    }
    let nick = nick.trim();
    if nick.is_empty() || nick.contains(['*', '?', '@', ',', ' ']) {
        return None;
    }
    Some(nick.trim_start_matches('@'))
}

fn lookup_user_by_nick(directory: &HashMap<Uuid, String>, nick: &str) -> Option<(Uuid, String)> {
    directory
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(nick))
        .map(|(id, name)| (*id, name.clone()))
}

async fn send_all(framed: &mut IrcStream, messages: Vec<Message>) -> Result<()> {
    for message in messages {
        framed.feed(message).await?;
    }
    framed.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{IrcCapabilities, apply_cap_request, nick_from_ban_mask};

    #[test]
    fn ban_mask_accepts_nick_identity_shape() {
        assert_eq!(nick_from_ban_mask("alice!*@*"), Some("alice"));
        assert_eq!(nick_from_ban_mask("Alice_123!*@*"), Some("Alice_123"));
    }

    #[test]
    fn ban_mask_rejects_wildcards_hosts_and_plain_nicks() {
        assert_eq!(nick_from_ban_mask("*!*@*"), None);
        assert_eq!(nick_from_ban_mask("alice!*@example.com"), None);
        assert_eq!(nick_from_ban_mask("alice@host!*@*"), None);
        assert_eq!(nick_from_ban_mask("alice"), None);
    }

    #[test]
    fn cap_request_enables_and_lists_supported_caps() {
        let mut caps = IrcCapabilities::default();

        assert!(apply_cap_request(
            &mut caps,
            "message-tags server-time echo-message"
        ));

        assert!(caps.message_tags);
        assert!(caps.server_time);
        assert!(caps.echo_message);
        assert_eq!(caps.as_list(), "message-tags server-time echo-message");
    }

    #[test]
    fn cap_request_naks_unknown_without_changing_enabled_caps() {
        let mut caps = IrcCapabilities::default();
        assert!(apply_cap_request(&mut caps, "message-tags"));

        assert!(!apply_cap_request(&mut caps, "server-time chathistory"));

        assert!(caps.message_tags);
        assert!(!caps.server_time);
        assert!(!caps.echo_message);
        assert_eq!(caps.as_list(), "message-tags");
    }

    #[test]
    fn cap_request_can_disable_supported_caps() {
        let mut caps = IrcCapabilities::default();
        assert!(apply_cap_request(&mut caps, "message-tags server-time"));

        assert!(apply_cap_request(&mut caps, "-server-time"));

        assert!(caps.message_tags);
        assert!(!caps.server_time);
        assert_eq!(caps.as_list(), "message-tags");
    }
}
