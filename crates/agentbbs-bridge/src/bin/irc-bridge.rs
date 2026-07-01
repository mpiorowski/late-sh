//! `agentbbs-irc-bridge` — the runnable surface for ADR-0031 Phase 1.
//!
//! A minimal standalone IRC listener: `JOIN #channel` + `PRIVMSG #channel
//! :text` on a mapped channel is bridge-signed (ADR-0025) and delivered to a
//! running `agentbbs-web`/genesis node via `POST /api/boards/{slug}/signed`.
//! This is a separate optional process — it makes no changes to
//! `agentbbs-core` or `agentbbs-web` and can be pointed at any node.
//!
//! ```text
//! AGENTBBS_IRC_BRIDGE_SEED_HEX=<64 hex chars, keep secret> \
//! agentbbs-irc-bridge \
//!   --listen 0.0.0.0:6667 \
//!   --base-url https://agentbbs-web-63rzcdswba-uc.a.run.app \
//!   --network libera \
//!   --channels general=general,ops=ops
//! ```
//!
//! Not implemented (see ADR-0031 Phase 2 follow-ups): TLS, SASL/PASS auth,
//! per-connection rate limiting, PII scrub on egress, NICK collision
//! handling. Treat this as a private/internal bridge, not a public listener,
//! until those land.

use agentbbs_bridge::irc::{parse_channel_map, run_connection, ChannelMap};
use agentbbs_bridge::{BridgeIdentity, SeenSet};
use agentbbs_core::Message;
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

fn usage() -> ! {
    eprintln!(
        "usage: agentbbs-irc-bridge --listen <addr:port> --base-url <url> --network <name> --channels <ch=board,...>\n\
         \n\
         Reads AGENTBBS_IRC_BRIDGE_SEED_HEX (64 hex chars) from the environment —\n\
         never pass it as an argument. Same seed = same per-network bridge\n\
         identity across restarts; a new seed rotates it."
    );
    std::process::exit(2);
}

struct Args {
    listen: String,
    base_url: String,
    network: String,
    channels: ChannelMap,
}

fn parse_args() -> Args {
    let mut listen = None;
    let mut base_url = None;
    let mut network = None;
    let mut channels = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--listen" => listen = args.next(),
            "--base-url" => base_url = args.next(),
            "--network" => network = args.next(),
            "--channels" => channels = args.next(),
            "-h" | "--help" => usage(),
            other => {
                eprintln!("agentbbs-irc-bridge: unknown argument: {other}");
                usage();
            }
        }
    }
    let (Some(listen), Some(base_url), Some(network), Some(channels)) =
        (listen, base_url, network, channels)
    else {
        usage();
    };
    Args {
        listen,
        base_url,
        network,
        channels: parse_channel_map(&channels),
    }
}

fn seed_from_env() -> BridgeIdentity {
    let hex = std::env::var("AGENTBBS_IRC_BRIDGE_SEED_HEX").unwrap_or_else(|_| {
        eprintln!("agentbbs-irc-bridge: missing AGENTBBS_IRC_BRIDGE_SEED_HEX (64 hex chars)");
        std::process::exit(2);
    });
    let bytes = hex_decode(&hex).unwrap_or_else(|| {
        eprintln!("agentbbs-irc-bridge: AGENTBBS_IRC_BRIDGE_SEED_HEX is not valid hex");
        std::process::exit(2);
    });
    let seed: [u8; 32] = bytes.try_into().unwrap_or_else(|_| {
        eprintln!("agentbbs-irc-bridge: AGENTBBS_IRC_BRIDGE_SEED_HEX must decode to 32 bytes");
        std::process::exit(2);
    });
    BridgeIdentity::from_seed(seed)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Deliver a bridge-signed message to the target node's `/api/boards/{slug}/signed`.
/// Every field is read straight off the already-signed `Message` — never
/// reconstructed — so the signature the node re-verifies always matches.
async fn deliver(client: &reqwest::Client, base_url: &str, msg: &Message) {
    let url = format!(
        "{}/api/boards/{}/signed",
        base_url.trim_end_matches('/'),
        msg.body.board
    );
    let payload = serde_json::json!({
        "subject": msg.body.subject,
        "body": msg.body.body,
        "author": msg.body.author.to_hex(),
        "handle": msg.body.handle,
        "created_at": msg.body.created_at.to_rfc3339(),
        "signature": msg.signature.to_hex(),
    });
    match client.post(&url).json(&payload).send().await {
        Ok(resp) if resp.status().is_success() => {
            eprintln!("agentbbs-irc-bridge: delivered → #{}", msg.body.board);
        }
        Ok(resp) => {
            eprintln!(
                "agentbbs-irc-bridge: delivery rejected ({}): #{}",
                resp.status(),
                msg.body.board
            );
        }
        Err(e) => eprintln!("agentbbs-irc-bridge: delivery failed: {e}"),
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = parse_args();
    let identity = seed_from_env();
    let listener = TcpListener::bind(&args.listen).await?;
    eprintln!(
        "agentbbs-irc-bridge: listening on {} · network={} · {} channel(s) mapped · target={}",
        args.listen,
        args.network,
        args.channels.len(),
        args.base_url
    );

    let seen = Arc::new(Mutex::new(SeenSet::new()));
    let channels = Arc::new(args.channels);
    let client = Arc::new(reqwest::Client::new());
    let mut conn_no: u64 = 0;

    loop {
        let (sock, peer) = listener.accept().await?;
        conn_no += 1;
        let identity = identity.clone();
        let seen = seen.clone();
        let channels = channels.clone();
        let client = client.clone();
        let network = args.network.clone();
        let base_url = args.base_url.clone();
        let conn_tag = format!("{peer}-{conn_no}");
        tokio::spawn(async move {
            let result = run_connection(
                sock,
                &identity,
                &seen,
                &channels,
                &network,
                &conn_tag,
                |msg| {
                    let client = client.clone();
                    let base_url = base_url.clone();
                    async move { deliver(&client, &base_url, &msg).await }
                },
            )
            .await;
            if let Err(e) = result {
                eprintln!("agentbbs-irc-bridge: connection {conn_tag} ended: {e}");
            }
        });
    }
}
