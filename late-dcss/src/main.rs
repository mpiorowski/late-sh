// Standalone host for the Dungeon Crawl Stone Soup door game. Runs the real
// upstream crawl console binary on a PTY and serves it over SSH; late-ssh
// connects as a client and proxies the terminal into its DCSS launcher (the
// same transport as the nethack host).
//
// See late-ssh/src/app/door/dcss/CONTEXT.md.

mod config;
mod host;
mod identity;
#[cfg(test)]
mod identity_test;
mod playname;
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
        data_dir = %config.data_dir,
        listen = %config.listen_addr,
        port = config.port,
        "late-dcss host starting"
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

    // Broadcast a graceful-shutdown signal to every live PtyHost so a pod SIGTERM
    // SIGHUP-saves in-flight games (crawl saves-and-exits on hangup, same as the
    // dgamelaunch public servers rely on) instead of losing them to SIGKILL.
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
            tracing::info!("shutdown signal received; SIGHUP-saving live crawl children");
            let _ = shutdown_tx.send(true);
            // Hold the process open long enough for the bridges to run crawl's
            // hangup-save. Exceeds host.rs's per-child HANGUP_SAVE_GRACE and must
            // stay under the pod's terminationGracePeriodSeconds (service-dcss.tf).
            tokio::time::sleep(SHUTDOWN_GRACE).await;
            tracing::info!("shutdown grace elapsed; exiting");
        }
    }
    Ok(())
}

/// Total time to let in-flight games hangup-save on shutdown before the process
/// exits. Larger than host.rs's per-child `HANGUP_SAVE_GRACE`, smaller than the
/// pod's `terminationGracePeriodSeconds`.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(8);

/// Resolve when the process receives SIGTERM (k8s pod stop) or SIGINT (Ctrl-C).
/// Mirrors `late_core::shutdown::wait_for_shutdown_signal`; late-dcss has no
/// late-core dependency, so it is duplicated here.
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = ?e, "failed to install SIGTERM handler; shutdown-save disabled");
            // Never resolve, so we keep serving and fall back to per-session
            // teardown saves rather than spuriously triggering a shutdown.
            return std::future::pending().await;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
