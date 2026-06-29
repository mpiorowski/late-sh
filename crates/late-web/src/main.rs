use std::net::SocketAddr;

use anyhow::Context;
use late_core::db::Db;
use late_web::{AppState, app, config::Config};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = late_core::telemetry::init_telemetry("late-web")
        .context("failed to initialize telemetry")?;

    let config = Config::from_env().context("failed to load configuration")?;
    config.log_startup();

    let http_client = reqwest::Client::builder()
        .tcp_keepalive(std::time::Duration::from_secs(60))
        .pool_idle_timeout(std::time::Duration::from_secs(90))
        .no_proxy()
        .build()
        .context("failed to build HTTP client")?;
    let db = Db::from_env().context("failed to initialize database pool")?;

    let port = config.port;

    let state = AppState {
        config,
        db,
        http_client,
    };

    let app = app(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!(addr = %addr, "starting server");
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(late_core::shutdown::wait_for_shutdown_signal())
        .await;

    result?;
    Ok(())
}
