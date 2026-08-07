// The stats-session SSH client: connects to a door host as the reserved
// `late_stats` username, pushes the per-file cursors as one env request, and
// turns the host's framed line stream into typed [`StatsFrame`]s. The
// username, env var, and frame shape mirror the host side
// (`late-dcss/src/stats.rs`), the same cross-crate contract style as the rc
// push and the identity derivation; keep the copies in sync.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use russh::client::{self, Config, Handler};
use russh::keys::PublicKey;
use russh::{ChannelMsg, Disconnect};
use tokio::sync::mpsc;
use tokio::time::timeout;

/// The reserved SSH username that opens a stats session instead of a game.
pub const STATS_USERNAME: &str = "late_stats";

/// Env request carrying the per-file byte offsets (`logfile:123,milestones:456`).
pub const CURSORS_ENV_VAR: &str = "LATE_DOOR_STATS_CURSORS";

const SETUP_TIMEOUT: Duration = Duration::from_secs(15);

/// One complete log line from a host file. `next_offset` is the byte offset
/// after the line in the source file: persist it as the file's cursor once
/// the line's effect is committed, and resuming from it never replays the
/// line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatsFrame {
    pub file: String,
    pub next_offset: i64,
    pub line: String,
}

pub struct StreamConfig {
    pub host: String,
    pub port: u16,
    /// The shared-secret-derived door client key (each door has its own
    /// blake3 domain; the caller derives it with that door's `identity`).
    pub key: russh::keys::PrivateKey,
    /// Resume offsets per file id; missing files start at 0 (full backfill).
    pub cursors: HashMap<String, i64>,
}

/// The door hosts are trusted late.sh-owned services on the internal
/// network; auth is the derived client key (same policy as the game doors).
struct AcceptAnyHostKey;

impl Handler for AcceptAnyHostKey {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// Parse one `<file>\t<offset>\t<line>` frame. `None` for anything malformed
/// (logged and skipped by the caller; the host only ever writes this shape).
pub(crate) fn parse_frame(raw: &str) -> Option<StatsFrame> {
    let mut parts = raw.splitn(3, '\t');
    let file = parts.next()?;
    let next_offset: i64 = parts.next()?.parse().ok()?;
    let line = parts.next()?;
    if file.is_empty() {
        return None;
    }
    Some(StatsFrame {
        file: file.to_string(),
        next_offset,
        line: line.to_string(),
    })
}

fn cursors_env_value(cursors: &HashMap<String, i64>) -> String {
    cursors
        .iter()
        .map(|(file, offset)| format!("{file}:{offset}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Connect and stream frames into `tx` until the connection ends (host
/// restart, network drop) or `tx` closes (the consumer bailed). The caller
/// owns the retry policy.
pub async fn run_stats_stream(cfg: StreamConfig, tx: mpsc::Sender<StatsFrame>) -> Result<()> {
    let config = Arc::new(Config {
        // The stream idles between games; keepalives hold it open.
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    });

    let mut session = timeout(
        SETUP_TIMEOUT,
        client::connect(config, (cfg.host.as_str(), cfg.port), AcceptAnyHostKey),
    )
    .await
    .context("stats connect timed out")?
    .with_context(|| format!("connecting to {}:{}", cfg.host, cfg.port))?;

    let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(cfg.key), None);
    let auth = timeout(
        SETUP_TIMEOUT,
        session.authenticate_publickey(STATS_USERNAME, key),
    )
    .await
    .context("stats authenticate_publickey timed out")?
    .context("stats authenticate_publickey failed")?;
    if !auth.success() {
        anyhow::bail!("door host rejected derived credentials for stats session");
    }

    let mut channel = timeout(SETUP_TIMEOUT, session.channel_open_session())
        .await
        .context("stats channel_open_session timed out")?
        .context("stats channel_open_session failed")?;
    timeout(
        SETUP_TIMEOUT,
        channel.set_env(false, CURSORS_ENV_VAR, cursors_env_value(&cfg.cursors)),
    )
    .await
    .context("stats set_env timed out")?
    .context("stats set_env failed")?;
    timeout(SETUP_TIMEOUT, channel.request_shell(true))
        .await
        .context("stats request_shell timed out")?
        .context("stats request_shell failed")?;

    // Reassemble frames across chunk boundaries: data arrives in arbitrary
    // pieces, one frame per newline.
    let mut buf: Vec<u8> = Vec::new();
    let result = loop {
        let Some(msg) = channel.wait().await else {
            break Ok(());
        };
        match msg {
            ChannelMsg::Data { data } | ChannelMsg::ExtendedData { data, .. } => {
                buf.extend_from_slice(&data);
                while let Some(nl) = buf.iter().position(|&b| b == b'\n') {
                    let raw = String::from_utf8_lossy(&buf[..nl]).into_owned();
                    buf.drain(..=nl);
                    match parse_frame(&raw) {
                        Some(frame) => {
                            if tx.send(frame).await.is_err() {
                                // Consumer gone (ingest error path); close and
                                // let the retry loop reconnect from the last
                                // committed cursor.
                                break;
                            }
                        }
                        None => tracing::warn!(raw, "malformed stats frame; skipping"),
                    }
                }
                if tx.is_closed() {
                    break Ok(());
                }
            }
            ChannelMsg::Eof | ChannelMsg::Close | ChannelMsg::ExitStatus { .. } => break Ok(()),
            _ => {}
        }
    };

    let _ = channel.close().await;
    let _ = session
        .disconnect(Disconnect::ByApplication, "", "en")
        .await;
    result
}

#[cfg(test)]
mod tests {
    use super::{StatsFrame, parse_frame};

    #[test]
    fn parses_a_frame() {
        assert_eq!(
            parse_frame("logfile\t123\tname=X:sc=1"),
            Some(StatsFrame {
                file: "logfile".to_string(),
                next_offset: 123,
                line: "name=X:sc=1".to_string(),
            })
        );
    }

    #[test]
    fn line_keeps_embedded_tabs() {
        let frame = parse_frame("milestones\t9\ta\tb").expect("parses");
        assert_eq!(frame.line, "a\tb");
    }

    #[test]
    fn rejects_malformed_frames() {
        assert!(parse_frame("").is_none());
        assert!(parse_frame("logfile").is_none());
        assert!(parse_frame("logfile\tnot-a-number\tline").is_none());
        assert!(parse_frame("\t12\tline").is_none());
    }
}
