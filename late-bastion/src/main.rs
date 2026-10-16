use std::sync::Arc;

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use late_bastion::{config::Config, ssh};
use late_core::shutdown::{CancellationToken, wait_for_shutdown_signal};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(Config::from_env()?);
    config.log_startup();

    let shutdown = CancellationToken::new();

    let ssh_task = {
        let config = Arc::clone(&config);
        let shutdown = shutdown.clone();
        tokio::spawn(async move { ssh::run(config, shutdown).await })
    };

    wait_for_shutdown_signal().await;
    tracing::info!("shutdown signal received");
    shutdown.cancel();

    match ssh_task.await {
        Ok(Ok(())) => tracing::info!("bastion exited cleanly"),
        Ok(Err(e)) => {
            tracing::error!(error = ?e, "bastion ssh task failed");
            return Err(e);
        }
        Err(e) => {
            tracing::error!(error = ?e, "bastion ssh task panicked");
            return Err(anyhow::Error::new(e));
        }
    }

    Ok(())
}
