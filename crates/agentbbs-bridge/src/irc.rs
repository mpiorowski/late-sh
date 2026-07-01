//! Minimal standalone IRC listener bridging PRIVMSG → board posts (ADR-0031
//! Phase 1). This is **not** `late-ssh::ircd` — see ADR-0031's scope note for
//! why that 2100-line, Postgres-user-model-coupled handler has no extension
//! seam for board-routed, unauthenticated bridge traffic. Instead this reuses
//! the existing ADR-0025 bridge-signing identity ([`crate::inbound`]) and
//! loop guard, and delivers through the ordinary `POST /api/boards/{slug}/signed`
//! HTTP endpoint — no changes to `agentbbs-core` or `agentbbs-web`.
//!
//! The protocol handling here is deliberately small: `NICK`/`JOIN`/`PRIVMSG`/
//! `PING` only, no TLS, no SASL/PASS auth, no rate limiting, no PII scrub.
//! Those are explicit Phase 2 follow-ups (see the ADR's "Negative/risks").

use crate::inbound::{sign_inbound, BridgeIdentity, Inbound, SeenSet};
use agentbbs_core::Message;
use chrono::Utc;
use std::collections::HashMap;
use std::future::Future;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

/// channel name (without leading `#`) → board slug. Opt-in allowlist: a
/// channel absent from the map is never bridged, mirroring the Slack/Teams
/// board mapping's own opt-in design.
pub type ChannelMap = HashMap<String, String>;

/// Parse `"general=general,ops=ops-board"` into a [`ChannelMap`]. Malformed
/// or empty pairs are skipped rather than erroring — a typo in one mapping
/// shouldn't take down the whole listener.
pub fn parse_channel_map(spec: &str) -> ChannelMap {
    spec.split(',')
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let ch = it.next()?.trim();
            let board = it.next()?.trim();
            if ch.is_empty() || board.is_empty() {
                return None;
            }
            Some((ch.to_string(), board.to_string()))
        })
        .collect()
}

/// The subset of IRC events the bridge acts on.
#[derive(Debug, PartialEq, Eq)]
pub enum IrcEvent {
    Nick(String),
    Join(String),
    Privmsg { channel: String, text: String },
    Ping(String),
    Other,
}

/// Parse a single raw IRC line. Deliberately minimal — no prefix/tag parsing,
/// no numeric replies; enough to drive the bridge's own state machine.
pub fn parse_line(line: &str) -> IrcEvent {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(rest) = line.strip_prefix("PING ") {
        return IrcEvent::Ping(rest.trim_start_matches(':').to_string());
    }
    let mut parts = line.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").trim();
    match cmd.to_ascii_uppercase().as_str() {
        "NICK" if !rest.is_empty() => IrcEvent::Nick(rest.to_string()),
        "JOIN" if !rest.is_empty() => IrcEvent::Join(rest.trim_start_matches('#').to_string()),
        "PRIVMSG" => {
            let mut p = rest.splitn(2, " :");
            let target = p.next().unwrap_or("").trim();
            let text = p.next().unwrap_or("");
            match target.strip_prefix('#') {
                Some(ch) if !ch.is_empty() && !text.is_empty() => IrcEvent::Privmsg {
                    channel: ch.to_string(),
                    text: text.to_string(),
                },
                _ => IrcEvent::Other,
            }
        }
        _ => IrcEvent::Other,
    }
}

/// Turn a PRIVMSG on a mapped channel into a signed board [`Message`],
/// applying the channel allowlist and the loop guard. `None` means: not a
/// bridged channel, or `external_msg_id` was already seen (duplicate
/// delivery, or the bridge's own traffic echoing back).
#[allow(clippy::too_many_arguments)]
pub fn handle_privmsg(
    id: &BridgeIdentity,
    seen: &mut SeenSet,
    map: &ChannelMap,
    network: &str,
    nick: &str,
    channel: &str,
    text: &str,
    external_msg_id: &str,
) -> Option<Message> {
    let board = map.get(channel)?;
    if seen.seen_or_record(external_msg_id) {
        return None;
    }
    let inb = Inbound {
        platform: "irc".to_string(),
        workspace: network.to_string(),
        user_id: nick.to_string(),
        display_name: nick.to_string(),
        text: text.to_string(),
        external_msg_id: external_msg_id.to_string(),
        board: board.clone(),
    };
    Some(sign_inbound(id, &inb, Utc::now()))
}

/// Drive one IRC connection to completion: read lines, respond to PING, and
/// call `deliver` for every signed message a mapped PRIVMSG produces. Generic
/// over the stream and the delivery sink so tests can use an in-memory
/// duplex/loopback stream and an in-memory sink instead of real sockets/HTTP —
/// the same mockable-transport idiom as `agentbbs_federation::CommandRunner`.
pub async fn run_connection<S, D, Fut>(
    stream: S,
    id: &BridgeIdentity,
    seen: &std::sync::Mutex<SeenSet>,
    map: &ChannelMap,
    network: &str,
    conn_tag: &str,
    mut deliver: D,
) -> std::io::Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
    D: FnMut(Message) -> Fut,
    Fut: Future<Output = ()>,
{
    let (r, mut w) = tokio::io::split(stream);
    let mut lines = BufReader::new(r).lines();
    let mut nick = "unknown".to_string();
    let mut counter: u64 = 0;
    while let Some(line) = lines.next_line().await? {
        match parse_line(&line) {
            IrcEvent::Nick(n) => nick = n,
            IrcEvent::Ping(token) => {
                w.write_all(format!("PONG :{token}\r\n").as_bytes()).await?;
            }
            IrcEvent::Privmsg { channel, text } => {
                counter += 1;
                let ext_id = format!("irc:{network}:{conn_tag}:{counter}");
                let msg_opt = {
                    let mut seen = seen.lock().unwrap();
                    handle_privmsg(id, &mut seen, map, network, &nick, &channel, &text, &ext_id)
                };
                if let Some(msg) = msg_opt {
                    deliver(msg).await;
                }
            }
            IrcEvent::Join(_) | IrcEvent::Other => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_channel_map_skipping_malformed_pairs() {
        let m = parse_channel_map("general=general, ops=ops-board,bad,=x,y=");
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("general"), Some(&"general".to_string()));
        assert_eq!(m.get("ops"), Some(&"ops-board".to_string()));
    }

    #[test]
    fn parses_nick_join_privmsg_ping() {
        assert_eq!(parse_line("NICK alice"), IrcEvent::Nick("alice".into()));
        assert_eq!(
            parse_line("JOIN #general"),
            IrcEvent::Join("general".into())
        );
        assert_eq!(
            parse_line("PRIVMSG #general :hello there"),
            IrcEvent::Privmsg {
                channel: "general".into(),
                text: "hello there".into()
            }
        );
        assert_eq!(parse_line("PING :abc123"), IrcEvent::Ping("abc123".into()));
    }

    #[test]
    fn privmsg_to_a_user_not_a_channel_is_ignored() {
        assert_eq!(parse_line("PRIVMSG alice :dm me not"), IrcEvent::Other);
    }

    #[test]
    fn handle_privmsg_signs_a_mapped_message() {
        let id = BridgeIdentity::from_seed([9u8; 32]);
        let mut seen = SeenSet::new();
        let map = parse_channel_map("general=general");
        let msg = handle_privmsg(
            &id,
            &mut seen,
            &map,
            "libera",
            "alice",
            "general",
            "hi board",
            "irc:libera:c1:1",
        )
        .expect("mapped channel produces a message");
        assert!(msg.verify().is_ok());
        assert_eq!(msg.body.board, "general");
        assert_eq!(msg.body.body, "hi board");
        assert!(msg.body.handle.starts_with("bridge:irc:alice"));
    }

    #[test]
    fn unmapped_channel_is_not_bridged() {
        let id = BridgeIdentity::from_seed([9u8; 32]);
        let mut seen = SeenSet::new();
        let map = parse_channel_map("general=general");
        assert!(handle_privmsg(
            &id,
            &mut seen,
            &map,
            "libera",
            "alice",
            "secret",
            "psst",
            "irc:libera:c1:1"
        )
        .is_none());
    }

    #[test]
    fn duplicate_external_id_is_loop_guarded() {
        let id = BridgeIdentity::from_seed([9u8; 32]);
        let mut seen = SeenSet::new();
        let map = parse_channel_map("general=general");
        assert!(handle_privmsg(
            &id,
            &mut seen,
            &map,
            "libera",
            "a",
            "general",
            "one",
            "irc:libera:c1:1"
        )
        .is_some());
        assert!(handle_privmsg(
            &id,
            &mut seen,
            &map,
            "libera",
            "a",
            "general",
            "one-again",
            "irc:libera:c1:1"
        )
        .is_none());
    }

    /// The ADR's own testing bar: drive a *real* raw IRC client socket
    /// against the listener (loopback TCP, not an in-memory duplex) and
    /// assert a signed message lands — with delivery captured in-memory so
    /// the test never performs a real HTTP call.
    #[tokio::test]
    async fn a_real_socket_join_and_privmsg_produces_a_delivered_signed_message() {
        use tokio::io::AsyncWriteExt as _;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let id = BridgeIdentity::from_seed([3u8; 32]);
        let seen = std::sync::Mutex::new(SeenSet::new());
        let map = parse_channel_map("general=general");
        let delivered = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Message>::new()));
        let delivered_srv = delivered.clone();

        let server = tokio::spawn(async move {
            let (sock, _) = listener.accept().await.unwrap();
            run_connection(sock, &id, &seen, &map, "libera", "conn-1", |msg| {
                let delivered = delivered_srv.clone();
                async move {
                    delivered.lock().unwrap().push(msg);
                }
            })
            .await
            .unwrap();
        });

        let mut client = TcpStream::connect(addr).await.unwrap();
        client
            .write_all(
                b"NICK alice\r\nJOIN #general\r\nPRIVMSG #general :hello from a real socket\r\n",
            )
            .await
            .unwrap();
        drop(client); // EOF ends the server's read loop

        server.await.unwrap();

        let msgs = delivered.lock().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].body.board, "general");
        assert_eq!(msgs[0].body.body, "hello from a real socket");
        assert!(msgs[0].body.handle.starts_with("bridge:irc:alice"));
        assert!(msgs[0].verify().is_ok());
    }
}
