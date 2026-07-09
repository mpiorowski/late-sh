//! AgentBBS mobile web server.

use std::sync::Arc;

use agentbbs_core::store::{MemoryStore, RedbStore, Store};
use agentbbs_web::{router, AppState};

/// What `main` should do about store selection, decided from environment
/// alone so the decision itself is unit-testable without touching disk.
#[derive(Debug, PartialEq, Eq)]
enum StoreMode {
    /// Open a durable `RedbStore` at this path.
    Durable(String),
    /// Use the ephemeral in-memory store (only ever correct outside production).
    Ephemeral,
    /// Refuse to start. Carries the operator-facing reason.
    FailClosed(&'static str),
}

/// ADR-0054 Q4: with `AGENTBBS_DB_PATH` set, use the durable single-file
/// `RedbStore` so board state survives restarts — the persistence half of the
/// single-instance + persistent-volume Cloud Run recipe (pair with
/// `min-instances=1` and a mounted volume; redb is single-writer, so this is a
/// single-instance durability story, not multi-instance HA — see ADR-0054
/// backlog item 2(b) for the still-unbuilt shared/HA store).
///
/// `AGENTBBS_ENV=production` makes this fail closed: a production deployment
/// with no durable path configured, or one whose `RedbStore::open` fails,
/// refuses to boot rather than silently degrading to `MemoryStore` and losing
/// data on the next restart. Outside production (local dev, tests, demos),
/// the same failures fall back to `MemoryStore` with a loud warning.
fn store_mode(db_path: Option<&str>, production: bool) -> StoreMode {
    match db_path {
        Some(path) if !path.is_empty() => StoreMode::Durable(path.to_string()),
        _ if production => StoreMode::FailClosed(
            "AGENTBBS_ENV=production requires AGENTBBS_DB_PATH (a durable store) — \
             refusing to start with an ephemeral MemoryStore",
        ),
        _ => StoreMode::Ephemeral,
    }
}

fn is_production() -> bool {
    std::env::var("AGENTBBS_ENV").as_deref() == Ok("production")
}

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

    let db_path = std::env::var("AGENTBBS_DB_PATH").ok();
    let store: Arc<dyn Store> = match store_mode(db_path.as_deref(), is_production()) {
        StoreMode::Durable(path) => match RedbStore::open(&path) {
            Ok(s) => {
                tracing::info!("AgentBBS durable store: RedbStore at {path}");
                Arc::new(s) as Arc<dyn Store>
            }
            Err(e) if is_production() => {
                panic!(
                    "production requires a durable store; failed to open RedbStore at \
                     {path}: {e}"
                );
            }
            Err(e) => {
                tracing::error!(
                    "failed to open RedbStore at {path}: {e}; falling back to \
                     in-memory store (NOT durable — data is lost on restart)"
                );
                Arc::new(MemoryStore::new()) as Arc<dyn Store>
            }
        },
        StoreMode::Ephemeral => {
            tracing::info!(
                "AgentBBS in-memory store (ephemeral); set AGENTBBS_DB_PATH for persistence"
            );
            Arc::new(MemoryStore::new()) as Arc<dyn Store>
        }
        StoreMode::FailClosed(reason) => panic!("{reason}"),
    };
    let state = AppState::new(store);
    let app = router(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    tracing::info!("AgentBBS mobile web on http://{addr}");
    axum::serve(listener, app).await.expect("serve");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_path_selects_redb_regardless_of_environment() {
        assert_eq!(
            store_mode(Some("/data/bbs.redb"), false),
            StoreMode::Durable("/data/bbs.redb".to_string())
        );
        assert_eq!(
            store_mode(Some("/data/bbs.redb"), true),
            StoreMode::Durable("/data/bbs.redb".to_string())
        );
    }

    #[test]
    fn missing_path_is_ephemeral_outside_production() {
        assert_eq!(store_mode(None, false), StoreMode::Ephemeral);
        assert_eq!(store_mode(Some(""), false), StoreMode::Ephemeral);
    }

    #[test]
    fn missing_path_fails_closed_in_production() {
        assert!(matches!(store_mode(None, true), StoreMode::FailClosed(_)));
        assert!(matches!(
            store_mode(Some(""), true),
            StoreMode::FailClosed(_)
        ));
    }
}
