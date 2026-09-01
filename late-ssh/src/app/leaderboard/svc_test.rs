use super::*;
use late_core::db::{Db, DbConfig};
use late_core::models::leaderboard::RankedEntry;
use uuid::Uuid;

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
fn initial_snapshot_is_retained_without_subscribers() {
    let service = inert_service();
    let expected = RankedEntry {
        username: "first-player".to_string(),
        user_id: Uuid::from_u128(1),
        value: 42,
        rank: 1,
        note: None,
    };

    service.publish(LeaderboardData {
        monthly_chip_earners: vec![expected.clone()],
        ..LeaderboardData::default()
    });
    let session = service.subscribe();
    let snapshot = session.borrow();
    let actual = snapshot
        .monthly_chip_earners
        .first()
        .expect("published entry retained");

    assert_eq!(snapshot.monthly_chip_earners.len(), 1);
    assert_eq!(actual.username, expected.username);
    assert_eq!(actual.user_id, expected.user_id);
    assert_eq!(actual.value, expected.value);
    assert_eq!(actual.rank, expected.rank);
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

fn month(year: i32, month: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, 1).expect("valid month")
}

/// The award snapshot gate: the hourly tick must query the DB only at startup,
/// on a UTC month rollover, or on the daily fallback.
#[test]
fn award_snapshot_runs_at_startup() {
    assert!(should_snapshot_awards(month(2026, 8), None));
}

#[test]
fn award_snapshot_runs_when_the_month_rolls_over() {
    // Last pass wrote July, the clock now says August is the previous month:
    // run regardless of how recently the last pass ran.
    assert!(should_snapshot_awards(
        month(2026, 8),
        Some((month(2026, 7), Duration::from_secs(60)))
    ));
}

#[test]
fn award_snapshot_skips_a_warm_same_month_pass() {
    assert!(!should_snapshot_awards(
        month(2026, 8),
        Some((
            month(2026, 8),
            AWARD_SNAPSHOT_FALLBACK - Duration::from_secs(1)
        ))
    ));
}

#[test]
fn award_snapshot_falls_back_to_daily() {
    assert!(should_snapshot_awards(
        month(2026, 8),
        Some((month(2026, 8), AWARD_SNAPSHOT_FALLBACK))
    ));
}

#[test]
fn previous_utc_month_steps_back_within_a_year() {
    let today = NaiveDate::from_ymd_opt(2026, 9, 1).expect("valid date");
    assert_eq!(previous_utc_month(today), month(2026, 8));
}

#[test]
fn previous_utc_month_crosses_the_year_boundary() {
    let today = NaiveDate::from_ymd_opt(2026, 1, 15).expect("valid date");
    assert_eq!(previous_utc_month(today), month(2025, 12));
}

fn increment_for(batch: &PreparedOnlineTimeBatch, user_id: Uuid, month_start: NaiveDate) -> i64 {
    batch
        .batch
        .increments
        .iter()
        .find_map(|value| {
            (value.user_id == user_id && value.month_start == month_start)
                .then_some(value.milliseconds)
        })
        .expect("user increment in batch")
}

#[test]
fn online_time_checkpoints_one_continuous_presence_interval() {
    let tracker = OnlineTimeTracker::default();
    let user_id = Uuid::from_u128(10);
    let start = Instant::now();
    let august = month(2026, 8);
    let september = month(2026, 9);

    tracker.connected_at(user_id, start, august);
    // A defensive duplicate start cannot reset the first-online timestamp or
    // move its elapsed time into another UTC month.
    tracker.connected_at(user_id, start + Duration::from_secs(5), september);
    let first = tracker
        .begin_batch_at(start + Duration::from_secs(10), september)
        .expect("first checkpoint");
    assert!(!first.was_retry);
    assert_eq!(increment_for(&first, user_id, august), 10_000);

    let retry = tracker
        .begin_batch_at(start + Duration::from_secs(15), september)
        .expect("retained uncertain batch");
    assert!(retry.was_retry);
    assert_eq!(retry.batch.id, first.batch.id);
    assert_eq!(increment_for(&retry, user_id, august), 10_000);

    tracker.acknowledge(retry.batch.id);
    let follow_up = tracker
        .begin_batch_at(start + Duration::from_secs(15), september)
        .expect("time accrued while batch was uncertain");
    assert_eq!(increment_for(&follow_up, user_id, september), 5_000);
    tracker.acknowledge(follow_up.batch.id);

    tracker.disconnected_at(user_id, start + Duration::from_secs(20));
    let final_batch = tracker
        .begin_batch_at(start + Duration::from_secs(30), september)
        .expect("completed interval");
    assert_eq!(increment_for(&final_batch, user_id, september), 5_000);
    assert!(!tracker.is_active(user_id));
}

#[test]
fn online_time_adds_disjoint_reconnections() {
    let tracker = OnlineTimeTracker::default();
    let user_id = Uuid::from_u128(11);
    let start = Instant::now();
    let august = month(2026, 8);

    tracker.connected_at(user_id, start, august);
    tracker.disconnected_at(user_id, start + Duration::from_secs(10));
    tracker.connected_at(user_id, start + Duration::from_secs(20), august);
    tracker.disconnected_at(user_id, start + Duration::from_secs(25));

    let batch = tracker
        .begin_batch_at(start + Duration::from_secs(30), august)
        .expect("two completed intervals");
    assert_eq!(increment_for(&batch, user_id, august), 15_000);
}
