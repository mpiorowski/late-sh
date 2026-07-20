// Standalone host for the Usurper door game. Runs the real upstream Usurper
// binary (Rick Parrish's Free Pascal port of the classic 1993 BBS door) on a
// PTY and serves it over SSH; late-ssh connects as a client and proxies the
// terminal into its Usurper launcher (the same transport as the nethack and
// dcss hosts). The host also owns the door-specific plumbing the roguelikes
// didn't need: per-session DOOR32.SYS dropfiles (identity), node-slot
// allocation (the game's multinode model), and CP437 -> UTF-8 output
// transcoding (the game predates Unicode).
//
// See late-ssh/src/app/door/usurper/CONTEXT.md.

mod config;
mod cp437;
#[cfg(test)]
mod cp437_test;
mod dropfile;
#[cfg(test)]
mod dropfile_test;
mod host;
mod identity;
#[cfg(test)]
mod identity_test;
mod nodes;
#[cfg(test)]
mod nodes_test;
mod playname;
#[cfg(test)]
mod playname_test;
mod seed;
#[cfg(test)]
mod seed_test;
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
        game_dir = %config.game_dir,
        seed_dir = %config.seed_dir,
        listen = %config.listen_addr,
        port = config.port,
        max_nodes = config.max_nodes,
        "late-usurper host starting"
    );

    // Seed the writable game tree from the image's read-only template (only
    // files that don't exist yet, so the shared world is never overwritten) and
    // sweep the two stale-lock artifacts a hard kill can leave behind. Both are
    // safe exactly because this runs before any session exists.
    seed::prepare_game_dir(&config.seed_dir, &config.game_dir)
        .context("preparing usurper game dir")?;

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

    // Broadcast a graceful-shutdown signal to every live PtyHost so a pod
    // SIGTERM tears each child down through the SIGHUP-then-SIGKILL path
    // instead of losing them to an abrupt exit.
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
            tracing::info!("shutdown signal received; tearing down live usurper children");
            let _ = shutdown_tx.send(true);
            // Hold the process open long enough for the bridges to finish their
            // teardown. Exceeds host.rs's per-child HANGUP_GRACE and must stay
            // under the pod's terminationGracePeriodSeconds (service-usurper.tf).
            tokio::time::sleep(SHUTDOWN_GRACE).await;
            tracing::info!("shutdown grace elapsed; exiting");
        }
    }
    Ok(())
}

/// Total time to let in-flight sessions tear down on shutdown before the
/// process exits. Larger than host.rs's per-child `HANGUP_GRACE`, smaller than
/// the pod's `terminationGracePeriodSeconds`.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(8);

/// Resolve when the process receives SIGTERM (k8s pod stop) or SIGINT (Ctrl-C).
/// Mirrors `late_core::shutdown::wait_for_shutdown_signal`; late-usurper has no
/// late-core dependency, so it is duplicated here.
async fn wait_for_shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = ?e, "failed to install SIGTERM handler; shutdown teardown disabled");
            // Never resolve, so we keep serving and fall back to per-session
            // teardown rather than spuriously triggering a shutdown.
            return std::future::pending().await;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}
