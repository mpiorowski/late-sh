//! The daily streak: one row per user in `user_daily_quest_streaks` (the
//! table keeps the name it was born with), kept by any daily board won on a
//! UTC day. A day counts once, yesterday continues the run, anything older
//! restarts it at 1. It pays nothing; the count is the whole reward.

use anyhow::Result;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use tokio_postgres::GenericClient;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct DailyStreak {
    pub user_id: Uuid,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
    pub last_completed_date: NaiveDate,
    pub consecutive_days: i32,
}

impl From<tokio_postgres::Row> for DailyStreak {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            user_id: row.get("user_id"),
            created: row.get("created"),
            updated: row.get("updated"),
            last_completed_date: row.get("last_completed_date"),
            consecutive_days: row.get("consecutive_days"),
        }
    }
}

/// A streak as seen on a day: zero once a whole day has gone by without a
/// win, otherwise the count and the day it was last kept.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DailyStreakSnapshot {
    pub consecutive_days: i32,
    pub last_completed_date: Option<NaiveDate>,
}

/// Which day a win kept: the streak's first, or one more on a run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreakDay {
    First,
    Kept { consecutive_days: i32 },
}

/// Keep the streak for `won_on`, the UTC date of a daily board win. `None`
/// when that day already counted (or the win is older than the last day
/// kept, which a late event can be), so a day with many wins is one day.
pub async fn record_daily_win(
    client: &impl GenericClient,
    user_id: Uuid,
    won_on: NaiveDate,
) -> Result<Option<StreakDay>> {
    let existing = client
        .query_opt(
            "SELECT last_completed_date, consecutive_days
             FROM user_daily_quest_streaks
             WHERE user_id = $1",
            &[&user_id],
        )
        .await?;
    let existing = existing.map(|row| {
        (
            row.get::<_, NaiveDate>("last_completed_date"),
            row.get::<_, i32>("consecutive_days"),
        )
    });
    let Some(day) = next_streak_day(existing, won_on) else {
        return Ok(None);
    };
    let consecutive_days = match day {
        StreakDay::First => 1,
        StreakDay::Kept { consecutive_days } => consecutive_days,
    };
    client
        .execute(
            "INSERT INTO user_daily_quest_streaks
                (user_id, last_completed_date, consecutive_days)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id) DO UPDATE SET
                last_completed_date = EXCLUDED.last_completed_date,
                consecutive_days = EXCLUDED.consecutive_days,
                updated = current_timestamp",
            &[&user_id, &won_on, &consecutive_days],
        )
        .await?;
    Ok(Some(day))
}

pub async fn get_daily_streak_snapshot(
    client: &impl GenericClient,
    user_id: Uuid,
    today: NaiveDate,
) -> Result<DailyStreakSnapshot> {
    let row = client
        .query_opt(
            "SELECT user_id, created, updated, last_completed_date, consecutive_days
             FROM user_daily_quest_streaks
             WHERE user_id = $1",
            &[&user_id],
        )
        .await?;
    let Some(row) = row else {
        return Ok(DailyStreakSnapshot::default());
    };
    Ok(snapshot_from_streak(&DailyStreak::from(row), today))
}

/// The rule: a day counts once, yesterday continues, anything older
/// restarts at 1. `existing` is the row's last kept day and count.
fn next_streak_day(existing: Option<(NaiveDate, i32)>, won_on: NaiveDate) -> Option<StreakDay> {
    let Some((last_completed_date, consecutive_days)) = existing else {
        return Some(StreakDay::First);
    };
    let yesterday = won_on - Duration::days(1);
    if last_completed_date >= won_on {
        None
    } else if last_completed_date == yesterday {
        Some(StreakDay::Kept {
            consecutive_days: consecutive_days.max(1).saturating_add(1),
        })
    } else {
        Some(StreakDay::First)
    }
}

fn snapshot_from_streak(streak: &DailyStreak, today: NaiveDate) -> DailyStreakSnapshot {
    let yesterday = today - Duration::days(1);
    if streak.last_completed_date != today && streak.last_completed_date != yesterday {
        return DailyStreakSnapshot::default();
    }
    DailyStreakSnapshot {
        consecutive_days: streak.consecutive_days,
        last_completed_date: Some(streak.last_completed_date),
    }
}

#[cfg(test)]
#[path = "daily_streak_test.rs"]
mod daily_streak_test;
