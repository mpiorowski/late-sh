// Standalone host for the dopewars door game. Runs the real upstream dopewars
// curses client on a PTY and serves it over SSH; late-ssh connects as a client
// and proxies the terminal into its dopewars launcher (the same transport as the
// nethack host).
//
// See late-ssh/src/app/door/dopewars/CONTEXT.md.

mod config;
mod host;
mod identity;
mod server;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use getrandom::SysRng;
use russh::keys::PrivateKey;
use russh::keys::signature::rand_core::UnwrapErr;

use crate::config::Config;
use crate::server::Server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().context("loading config from environment")?;
    tracing::info!(
        bin = %config.bin,
        score_file = %config.score_file,
        listen = %config.listen_addr,
        port = config.port,
        "late-dopewars host starting"
    );

    // Ephemeral SSH host key, generated fresh on each start. late-ssh is the only
    // client and accepts any host key (auth is the shared-secret-derived client
    // key carried by the connection), so there is nothing to gain from persisting
    // it across restarts.
    let key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)?;
    let ssh_config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(config.idle_timeout)),
        auth_rejection_time: Duration::from_secs(3),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![key],
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });

    let listen_addr = config.listen_addr.clone();
    let port = config.port;
    let mut server = Server::new(&config);

    tracing::info!(%listen_addr, port, "ssh listener bound");
    // dopewars has no mid-game save, so a pod SIGTERM has nothing to drain: the
    // detached per-session bridges' children are `kill_on_drop` and die with the
    // process. We just exit promptly on the signal.
    tokio::select! {
        res = russh::server::Server::run_on_address(
            &mut server,
            ssh_config,
            (listen_addr.as_str(), port),
        ) => {
            res.context("ssh server run loop failed")?;
        }
        _ = wait_for_shutdown_signal() => {
            tracing::info!("shutdown signal received; exiting");
        }
    }
    Ok(())
}

/// Resolve when the process receives SIGTERM (k8s pod stop) or SIGINT (Ctrl-C).
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = ?e, "failed to install SIGTERM handler");
            return std::future::pending().await;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
