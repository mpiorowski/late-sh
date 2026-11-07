use super::*;
use crate::test_utils::{create_test_user, test_db};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
}

#[test]
fn a_day_counts_once_yesterday_continues_and_older_restarts() {
    assert_eq!(next_streak_day(None, day(5)), Some(StreakDay::First));
    assert_eq!(
        next_streak_day(Some((day(4), 3)), day(5)),
        Some(StreakDay::Kept {
            consecutive_days: 4
        })
    );
    assert_eq!(next_streak_day(Some((day(5), 4)), day(5)), None);
    assert_eq!(
        next_streak_day(Some((day(5), 4)), day(4)),
        None,
        "a late event for an earlier day never moves the run"
    );
    assert_eq!(
        next_streak_day(Some((day(2), 9)), day(5)),
        Some(StreakDay::First)
    );
}

#[test]
fn the_snapshot_is_empty_once_a_whole_day_went_by() {
    let kept = DailyStreakSnapshot {
        consecutive_days: 7,
        last_completed_date: Some(day(4)),
    };
    assert_eq!(snapshot_from_streak(day(4), 7, day(4)), kept);
    assert_eq!(snapshot_from_streak(day(4), 7, day(5)), kept);
    assert_eq!(
        snapshot_from_streak(day(4), 7, day(6)),
        DailyStreakSnapshot::default()
    );
}

/// The moon fills slowly: nothing under three days, then one glyph per
/// milestone, so the mark is never a one-day badge.
#[test]
fn the_moon_fills_at_three_seven_thirty_and_a_hundred() {
    assert_eq!(StreakMark::for_days(0), None);
    assert_eq!(StreakMark::for_days(2), None);
    assert_eq!(StreakMark::for_days(3), Some(StreakMark::Crescent));
    assert_eq!(StreakMark::for_days(6), Some(StreakMark::Crescent));
    assert_eq!(StreakMark::for_days(7), Some(StreakMark::Half));
    assert_eq!(StreakMark::for_days(30), Some(StreakMark::Full));
    assert_eq!(StreakMark::for_days(100), Some(StreakMark::Core));
    assert_eq!(StreakMark::for_days(1000).map(StreakMark::glyph), Some('◉'));
}

/// Three wins on two days make a two-day streak, and the snapshot reads
/// it back on the days it is alive.
#[tokio::test]
async fn wins_keep_the_day_and_the_snapshot_reads_it_back() {
    let test_db = test_db().await;
    let user = create_test_user(&test_db.db, "streaker").await;
    let client = test_db.db.get().await.expect("db client");

    assert_eq!(
        record_daily_win(&**client, user.id, day(5)).await.unwrap(),
        Some(StreakDay::First)
    );
    assert_eq!(
        record_daily_win(&**client, user.id, day(5)).await.unwrap(),
        None,
        "a second board the same day is the same day"
    );
    assert_eq!(
        record_daily_win(&**client, user.id, day(6)).await.unwrap(),
        Some(StreakDay::Kept {
            consecutive_days: 2
        })
    );
    assert_eq!(
        get_daily_streak_snapshot(&**client, user.id, day(7))
            .await
            .unwrap(),
        DailyStreakSnapshot {
            consecutive_days: 2,
            last_completed_date: Some(day(6)),
        }
    );
    assert_eq!(
        get_daily_streak_snapshot(&**client, user.id, day(8))
            .await
            .unwrap(),
        DailyStreakSnapshot::default()
    );
    // The batched read for the chat marks lists the live runs only.
    let stranger = create_test_user(&test_db.db, "streak-stranger").await;
    let ids = [user.id, stranger.id];
    let listed = |today: NaiveDate| list_daily_streak_snapshots(&**client, &ids, today);
    assert_eq!(
        listed(day(7)).await.unwrap(),
        HashMap::from([(
            user.id,
            DailyStreakSnapshot {
                consecutive_days: 2,
                last_completed_date: Some(day(6)),
            }
        )])
    );
    assert_eq!(listed(day(8)).await.unwrap(), HashMap::new());
    assert_eq!(
        record_daily_win(&**client, user.id, day(9)).await.unwrap(),
        Some(StreakDay::First),
        "a missed day restarts the run"
    );
}
