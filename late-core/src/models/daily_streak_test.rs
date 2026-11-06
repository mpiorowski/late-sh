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
    let streak = DailyStreak {
        user_id: Uuid::now_v7(),
        created: DateTime::<Utc>::UNIX_EPOCH,
        updated: DateTime::<Utc>::UNIX_EPOCH,
        last_completed_date: day(4),
        consecutive_days: 7,
    };
    let kept = DailyStreakSnapshot {
        consecutive_days: 7,
        last_completed_date: Some(day(4)),
    };
    assert_eq!(snapshot_from_streak(&streak, day(4)), kept);
    assert_eq!(snapshot_from_streak(&streak, day(5)), kept);
    assert_eq!(
        snapshot_from_streak(&streak, day(6)),
        DailyStreakSnapshot::default()
    );
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
    assert_eq!(
        record_daily_win(&**client, user.id, day(9)).await.unwrap(),
        Some(StreakDay::First),
        "a missed day restarts the run"
    );
}
