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

fn increment_for(
    batch: &PreparedOnlineTimeBatch,
    user_id: Uuid,
    month_start: NaiveDate,
) -> OnlineTimeIncrement {
    *batch
        .batch
        .increments
        .iter()
        .find(|value| value.user_id == user_id && value.month_start == month_start)
        .expect("user increment in batch")
}

fn at(day: u32, hour: u32, second: u32) -> DateTime<Utc> {
    use chrono::TimeZone;
    let (month, day) = match day {
        0 => (8, 31),
        day => (9, day),
    };
    Utc.with_ymd_and_hms(2026, month, day, hour, 0, second)
        .single()
        .expect("fixture time")
}

#[test]
fn online_time_checkpoints_one_continuous_presence_interval() {
    let tracker = OnlineTimeTracker::default();
    let user_id = Uuid::from_u128(10);
    // Connected on the last evening of August.
    let start = at(0, 23, 0);
    let august = month(2026, 8);
    let september = month(2026, 9);

    tracker.connected_at(user_id, start);
    // A defensive duplicate start cannot reset the first-online timestamp.
    tracker.connected_at(user_id, start + chrono::Duration::seconds(5));
    let first = tracker
        .begin_batch_at(start + chrono::Duration::seconds(10))
        .expect("first checkpoint");
    assert!(!first.was_retry);
    assert_eq!(
        increment_for(&first, user_id, august),
        OnlineTimeIncrement {
            user_id,
            month_start: august,
            span_start: start,
            span_end: start + chrono::Duration::seconds(10),
            milliseconds: 10_000,
        }
    );

    let retry = tracker
        .begin_batch_at(start + chrono::Duration::seconds(15))
        .expect("retained uncertain batch");
    assert!(retry.was_retry);
    assert_eq!(retry.batch.id, first.batch.id);
    assert_eq!(retry.batch.increments, first.batch.increments);

    tracker.acknowledge(retry.batch.id);
    let follow_up = tracker
        .begin_batch_at(start + chrono::Duration::seconds(15))
        .expect("time accrued while batch was uncertain");
    assert_eq!(
        increment_for(&follow_up, user_id, august).span_start,
        start + chrono::Duration::seconds(10),
        "the next span starts where the checkpoint left off"
    );
    assert_eq!(increment_for(&follow_up, user_id, august).milliseconds, 5_000);
    tracker.acknowledge(follow_up.batch.id);

    // Past midnight: the next checkpoint's span begins in September.
    let midnight = at(1, 0, 0);
    tracker.disconnected_at(user_id, midnight + chrono::Duration::seconds(20));
    let spill = tracker
        .begin_batch_at(midnight + chrono::Duration::seconds(30))
        .expect("the segment that crossed midnight");
    assert_eq!(
        increment_for(&spill, user_id, august).milliseconds,
        3_600_000 - 15_000 + 20_000,
        "a segment counts in the month it began"
    );
    tracker.acknowledge(spill.batch.id);

    tracker.connected_at(user_id, midnight + chrono::Duration::seconds(40));
    tracker.disconnected_at(user_id, midnight + chrono::Duration::seconds(45));
    let september_batch = tracker
        .begin_batch_at(midnight + chrono::Duration::seconds(50))
        .expect("a September segment");
    assert_eq!(
        increment_for(&september_batch, user_id, september).milliseconds,
        5_000
    );
    assert!(!tracker.is_active(user_id));
}

#[test]
fn online_time_folds_reconnections_into_one_span_with_the_gap_left_out() {
    let tracker = OnlineTimeTracker::default();
    let user_id = Uuid::from_u128(11);
    let start = at(2, 12, 0);
    let september = month(2026, 9);

    tracker.connected_at(user_id, start);
    tracker.disconnected_at(user_id, start + chrono::Duration::seconds(10));
    tracker.connected_at(user_id, start + chrono::Duration::seconds(20));
    tracker.disconnected_at(user_id, start + chrono::Duration::seconds(25));

    let batch = tracker
        .begin_batch_at(start + chrono::Duration::seconds(30))
        .expect("two completed intervals");
    assert_eq!(
        batch.batch.increments,
        vec![OnlineTimeIncrement {
            user_id,
            month_start: september,
            span_start: start,
            span_end: start + chrono::Duration::seconds(25),
            milliseconds: 15_000,
        }]
    );
}

fn placement(username: &str, category: &str, rank: i32, score_value: i64) -> AwardRollEntry {
    AwardRollEntry {
        username: username.to_string(),
        category: category.to_string(),
        rank,
        score_value,
    }
}

/// The whole #lounge roll from a fixed month: boards in badge order whatever
/// order the rows came in, medals in rank order, a shared first place shown
/// twice, the gallery's prizes, the single-holder awards (Late Time, Top
/// Drinkers) worded as such, and
/// a board nobody placed on (Snake) left out.
#[test]
fn award_roll_body_reads_as_one_message_per_month() {
    let roll = AwardRoll {
        period_month: month(2026, 9),
        entries: vec![
            placement("mira", "top_chips", 1, 120_500),
            placement("ada", "arcade_wins", 1, 340),
            placement("kai", "arcade_wins", 1, 340),
            placement("rex", "tetris", 1, 45_000),
            placement("ada", "artboard", 1, 12),
            placement("owl", "late_time", 1, 133_979_000),
            placement("barfly", "top_drinkers", 1, 12_400),
            placement("bo", "top_chips", 2, 98_000),
            placement("zed", "twenty_forty_eight", 2, 2_048),
            placement("kai", "artboard", 2, 9),
            placement("cy", "top_chips", 3, 50_000),
        ],
    };

    assert_eq!(
        award_roll_body(&roll),
        "\u{1F3C6} The September 2026 awards are in! Winners wear their badges all October.\n\
         Arcade Wins: \u{1F947} @ada 340 pts \u{00B7} \u{1F947} @kai 340 pts\n\
         Top Chips: \u{1F947} @mira 120,500 chips \u{00B7} \u{1F948} @bo 98,000 chips \u{00B7} \u{1F949} @cy 50,000 chips\n\
         Lateris: \u{1F947} @rex 45,000\n\
         2048: \u{1F948} @zed 2,048\n\
         Artboard Gallery: \u{1F947} @ada 12 applause (+40,000 chips) \u{00B7} \u{1F948} @kai 9 applause (+15,000 chips)\n\
         Late Time: \u{1F319} @owl with 37h 12m online\n\
         Top Drinkers: \u{1F37A} @barfly with 12,400 buzz\n\
         Congrats! Full standings on the Leaderboards page."
    );
}
