//! ircd TCP listener. Config-gated; spawned from main alongside SSH/API.
//!
//! Shutdown policy is fast-disconnect, no drain (FRD §10 L2): on shutdown we
//! send every connection `ERROR :Server restarting` and rely on client
//! auto-reconnect against the replacement pod.

use std::net::IpAddr;

use anyhow::Result;
use late_core::{rate_limit::IpRateLimiter, shutdown::CancellationToken};
use tokio::{io::AsyncWriteExt, net::TcpListener};

use super::conn;
use crate::state::State;

pub async fn run(state: State, shutdown: Option<CancellationToken>) -> Result<()> {
    let config = state.config.irc.clone();
    let listener = TcpListener::bind(("0.0.0.0", config.port)).await?;
    tracing::info!(port = config.port, "ircd listening");
    let auth_limiter = IpRateLimiter::new(
        config.max_auth_failures_per_ip,
        config.auth_failure_window_secs,
    );

    loop {
        tokio::select! {
            _ = cancelled(&shutdown) => break,
            accepted = listener.accept() => {
                let (stream, addr) = match accepted {
                    Ok(accepted) => accepted,
                    Err(err) => {
                        tracing::warn!(error = %err, "ircd: accept failed");
                        continue;
                    }
                };
                let peer_ip: IpAddr = addr.ip();
                if state.is_draining.load(std::sync::atomic::Ordering::Relaxed) {
                    reject(stream, "Server restarting").await;
                    continue;
                }
                if state.irc_registry.connection_count() >= config.max_conns_global {
                    reject(stream, "Too many connections").await;
                    continue;
                }
                let conn_state = state.clone();
                let conn_limiter = auth_limiter.clone();
                tokio::spawn(async move {
                    if let Err(err) =
                        conn::handle(conn_state, stream, peer_ip, conn_limiter).await
                    {
                        tracing::debug!(error = %err, %peer_ip, "ircd: connection ended with error");
                    }
                });
            }
        }
    }

    let disconnected = state.irc_registry.disconnect_all("Server restarting");
    tracing::info!(disconnected, "ircd: shutdown, disconnected clients");
    Ok(())
}

async fn cancelled(shutdown: &Option<CancellationToken>) {
    match shutdown {
        Some(token) => token.cancelled().await,
        None => std::future::pending().await,
    }
}

/// Best-effort ERROR line for connections refused before registration.
async fn reject(mut stream: tokio::net::TcpStream, reason: &str) {
    let line = format!("ERROR :{reason}\r\n");
    let _ = stream.write_all(line.as_bytes()).await;
    let _ = stream.shutdown().await;
}
