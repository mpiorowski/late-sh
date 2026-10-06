// Standalone host for CodeKeep: The Pale. It runs the pinned upstream terminal
// game on one PTY per account and serves that PTY over SSH to late-ssh.

mod account;
mod config;
mod host;
mod identity;
#[cfg(test)]
mod identity_test;
mod server;

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use getrandom::SysRng;
use russh::keys::PrivateKey;
use russh::keys::signature::rand_core::UnwrapErr;

use crate::config::Config;
use crate::server::Server;

const SHUTDOWN_GRACE: Duration = Duration::from_secs(8);

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
        data_dir = %config.data_dir,
        listen = %config.listen_addr,
        port = config.port,
        "late-codekeep host starting"
    );

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
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut server = Server::new(&config, shutdown_rx);

    tracing::info!(%listen_addr, port, "ssh listener bound");
    tokio::select! {
        res = russh::server::Server::run_on_address(
            &mut server,
            ssh_config,
            (listen_addr.as_str(), port),
        ) => {
            res.context("ssh server run loop failed")?;
        }
        _ = wait_for_shutdown_signal() => {
            tracing::info!("shutdown signal received; saving live CodeKeep games");
            let _ = shutdown_tx.send(true);
            tokio::time::sleep(SHUTDOWN_GRACE).await;
            tracing::info!("shutdown grace elapsed; exiting");
        }
    }
    Ok(())
}

async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(signal) => signal,
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
