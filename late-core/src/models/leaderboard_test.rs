use chrono::{Datelike, Duration, NaiveDate, Utc};
use uuid::Uuid;

use crate::{
    models::{
        le_word::DailyWin,
        leaderboard::{RankedEntry, fetch_leaderboard_data},
    },
    test_utils::{create_test_user, test_db},
};

fn entry_for(entries: &[RankedEntry], user_id: Uuid) -> &RankedEntry {
    entries
        .iter()
        .find(|entry| entry.user_id == user_id)
        .expect("user on board")
}

/// Le Word feeds three boards off one win table: two counts and a
/// gaps-and-islands longest-run. The run is the part worth pinning down, so the
/// fixture gives the leader a broken streak that is longer in total than the
/// rival's unbroken one.
#[tokio::test]
async fn le_word_boards_rank_wins_and_longest_streak() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let solver = create_test_user(&test_db.db, "le_word_solver").await;
    let rival = create_test_user(&test_db.db, "le_word_rival").await;

    let today = Utc::now().date_naive();
    let month_start =
        NaiveDate::from_ymd_opt(today.year(), today.month(), 1).expect("first of month");
    let day = |offset: i64| month_start + Duration::days(offset);

    // Three in a row, a two-day gap, then two more: five wins this month, best
    // run of three.
    for offset in [0, 1, 2, 5, 6] {
        DailyWin::record_win(&client, solver.id, day(offset), 4)
            .await
            .expect("record solver win");
    }
    // An older win outside the month, far enough back to island on its own.
    DailyWin::record_win(&client, solver.id, month_start - Duration::days(10), 3)
        .await
        .expect("record solver history");

    for offset in [1, 2] {
        DailyWin::record_win(&client, rival.id, day(offset), 5)
            .await
            .expect("record rival win");
    }

    let data = fetch_leaderboard_data(&client)
        .await
        .expect("fetch leaderboard");

    let monthly = entry_for(&data.monthly_le_word_wins, solver.id);
    assert_eq!(monthly.value, 5, "last month's win must not count");
    assert_eq!(monthly.rank, 1);
    assert_eq!(entry_for(&data.monthly_le_word_wins, rival.id).value, 2);

    let all_time = entry_for(&data.all_time_le_word_wins, solver.id);
    assert_eq!(all_time.value, 6, "all-time counts the older win too");
    assert_eq!(all_time.rank, 1);

    let streak = entry_for(&data.le_word_win_streaks, solver.id);
    assert_eq!(streak.value, 3, "the gap breaks the run at three");
    assert_eq!(streak.rank, 1);

    let rival_streak = entry_for(&data.le_word_win_streaks, rival.id);
    assert_eq!(rival_streak.value, 2);
    assert_eq!(rival_streak.rank, 2);
}
