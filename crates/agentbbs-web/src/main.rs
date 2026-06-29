//! AgentBBS mobile web server.

use std::sync::Arc;

use agentbbs_core::store::MemoryStore;
use agentbbs_web::{router, AppState};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8088);

    // A single shared in-memory store; swap for RedbStore for persistence.
    let state = AppState::new(Arc::new(MemoryStore::new()));
    let app = router(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    tracing::info!("AgentBBS mobile web on http://{addr}");
    axum::serve(listener, app).await.expect("serve");
}
