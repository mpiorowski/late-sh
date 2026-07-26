use super::*;
use late_core::db::{Db, DbConfig};

/// The refresh loop's gate. These assert the observable contract: a service
/// nobody is watching does no leaderboard work, and one live session is enough
/// to turn it back on. No DB access is needed to exercise the gate itself.
fn inert_service() -> LeaderboardService {
    let db = Db::new(&DbConfig::default()).expect("inert pool");
    LeaderboardService::new(db)
}

#[test]
fn skips_refresh_when_nobody_is_watching() {
    let service = inert_service();

    assert!(
        !service.has_subscribers(),
        "a service with no sessions attached must not refresh"
    );
}

#[test]
fn refreshes_while_a_session_is_watching() {
    let service = inert_service();
    let session = service.subscribe();

    assert!(
        service.has_subscribers(),
        "one subscribed session must be enough to keep the loop refreshing"
    );

    drop(session);

    assert!(
        !service.has_subscribers(),
        "the loop must go quiet again once the last session disconnects"
    );
}

#[test]
fn refresh_interval_stays_coarse() {
    // Guards the 2026-07-26 fix: this loop was 13% of all DB execution time at
    // 30s. If a future change wants it hot again, the chip-balance path in
    // app/tick.rs and the SCALE.md ranking both need revisiting first.
    assert!(
        REFRESH_INTERVAL >= Duration::from_secs(60),
        "leaderboard refresh is a background timer, not a live feed"
    );
}
