// Standalone host for the NetHack door game. Runs the real upstream NetHack
// binary on a PTY and serves it over SSH; late-ssh connects as a client and
// proxies the terminal into its NetHack launcher (the rebels-camp transport).
//
// See late-ssh/src/app/door/nethack/CONTEXT.md.

mod config;
mod host;
mod identity;
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

fn load_or_generate_key(path: &std::path::Path) -> anyhow::Result<PrivateKey> {
    use russh::keys::ssh_key::LineEnding;

    if path.exists() {
        let key = russh::keys::load_secret_key(path, None)
            .with_context(|| format!("loading server key {}", path.display()))?;
        tracing::info!(path = %path.display(), "loaded existing server key");
        Ok(key)
    } else {
        let key = PrivateKey::random(&mut UnwrapErr(SysRng), russh::keys::Algorithm::Ed25519)?;
        let pem = key.to_openssh(LineEnding::LF)?;
        std::fs::write(path, pem.as_bytes())
            .with_context(|| format!("writing server key {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        tracing::info!(path = %path.display(), "generated new server key");
        Ok(key)
    }
}

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
        "late-nethack host starting"
    );

    let key = load_or_generate_key(&config.server_key_path)?;
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
    russh::server::Server::run_on_address(&mut server, ssh_config, (listen_addr.as_str(), port))
        .await
        .context("ssh server run loop failed")?;
    Ok(())
}
