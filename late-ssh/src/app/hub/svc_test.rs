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

/// The trap that made every new session render empty leaderboard panels:
/// `watch::Sender::subscribe` records the *current* version as already seen, so
/// `has_changed()` is false against a snapshot that is sitting right there.
/// `tick.rs` only copies on `has_changed()`, which is why `App::new` has to seed
/// from `borrow()` instead of waiting for the next send.
#[test]
fn subscribing_does_not_report_the_published_snapshot_as_changed() {
    let service = inert_service();
    let mut session = service.subscribe();

    assert!(
        !session.has_changed().expect("sender is alive"),
        "a fresh subscriber must not be relied on to report existing data as changed"
    );
    assert!(
        session.borrow_and_update().today_champions.is_empty(),
        "borrow is the only way to reach the seeded snapshot"
    );
}

#[test]
fn timer_wake_refreshes_whenever_anyone_is_watching() {
    assert!(should_refresh(Wake::Timer, true, None));
    assert!(should_refresh(
        Wake::Timer,
        true,
        Some(Duration::from_secs(0))
    ));
}

#[test]
fn no_wake_refreshes_without_subscribers() {
    assert!(!should_refresh(Wake::Timer, false, None));
    assert!(!should_refresh(Wake::Connect, false, None));
}

#[test]
fn connecting_to_a_stale_snapshot_earns_a_refresh() {
    // The quiet-server case: the loop skipped every pass while nobody was on, so
    // the first session back seeds from whatever the last one left behind.
    assert!(
        should_refresh(Wake::Connect, true, None),
        "a process that has never refreshed must rebuild for its first session"
    );
    assert!(
        should_refresh(Wake::Connect, true, Some(REFRESH_INTERVAL)),
        "a snapshot exactly one interval old has aged out"
    );
}

#[test]
fn connecting_to_a_warm_snapshot_costs_no_queries() {
    // The busy-server case, and the reason the connect path is age-bounded: this
    // fires once per session, so an unbounded version would put the refresh back
    // on the hot path that #474 took it off.
    assert!(!should_refresh(
        Wake::Connect,
        true,
        Some(Duration::from_secs(0))
    ));
    assert!(!should_refresh(
        Wake::Connect,
        true,
        Some(REFRESH_INTERVAL - Duration::from_millis(1))
    ));
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
