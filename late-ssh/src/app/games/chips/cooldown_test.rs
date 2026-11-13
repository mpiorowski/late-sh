use chrono::{DateTime, Duration, TimeZone, Utc};
use late_core::models::game_payout::LastCooldownClaim;
use late_core::models::reward::RewardTemplate;
use serde_json::json;
use uuid::Uuid;

use super::cooldown::{CooldownReward, CooldownTimers, PayoutWindow};

const MONTH_SECS: i32 = 30 * 24 * 60 * 60;

/// One active `cooldown` template per reward, keyed `game` / `kind-<n>` so
/// every reward has its own milestone, like the seeds.
fn templates() -> Vec<RewardTemplate> {
    CooldownReward::ALL
        .into_iter()
        .enumerate()
        .map(|(index, reward)| RewardTemplate {
            id: Uuid::now_v7(),
            key: reward.reward_key().to_string(),
            kind: "game_win".to_string(),
            params: json!({ "game": "game", "payout_kind": format!("kind-{index}") }),
            reward_chips: 10_000,
            claim_policy: "cooldown".to_string(),
            cooldown_seconds: Some(MONTH_SECS),
        })
        .collect()
}

fn claim(reward: CooldownReward, paid_at: DateTime<Utc>) -> LastCooldownClaim {
    let index = CooldownReward::ALL
        .iter()
        .position(|candidate| *candidate == reward)
        .expect("a listed reward");
    LastCooldownClaim {
        game: "game".to_string(),
        payout_kind: format!("kind-{index}"),
        paid_at,
    }
}

fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 12, 12, 0, 0).unwrap()
}

#[test]
fn a_milestone_opens_a_window_after_its_last_payout_and_only_its_own() {
    let paid_at = noon() - Duration::days(3);
    let timers = CooldownTimers::from_rows(
        &templates(),
        &[claim(CooldownReward::DarkroomEscape, paid_at)],
    )
    .expect("timers");

    // Paid three days ago: 27 days to go. Its sibling ending never paid.
    assert_eq!(
        timers.window(CooldownReward::DarkroomEscape, noon()),
        PayoutWindow::Closed {
            remaining: Duration::days(27)
        }
    );
    assert_eq!(
        timers.window(CooldownReward::DarkroomBeacon, noon()),
        PayoutWindow::Open
    );

    // The moment the window ends, it pays again.
    let opens = paid_at + Duration::seconds(i64::from(MONTH_SECS));
    assert_eq!(
        timers.window(CooldownReward::DarkroomEscape, opens),
        PayoutWindow::Open
    );
}

#[test]
fn a_missing_template_is_an_error_not_an_open_window() {
    let mut seeded = templates();
    seeded.retain(|template| template.key != CooldownReward::OldSignal.reward_key());
    let error = CooldownTimers::from_rows(&seeded, &[]).expect_err("a seed bug");
    assert!(
        error.to_string().contains("deadchannel_old_signal_slain"),
        "{error}"
    );
}

#[test]
fn the_label_counts_down_to_the_minute_and_never_reads_zero() {
    let label = |remaining| PayoutWindow::Closed { remaining }.label();
    assert_eq!(PayoutWindow::Open.label(), "pays now");
    assert_eq!(
        label(Duration::days(12) + Duration::hours(4) + Duration::minutes(30)),
        "pays again in 12d 4h"
    );
    assert_eq!(
        label(Duration::hours(5) + Duration::minutes(12)),
        "pays again in 5h 12m"
    );
    assert_eq!(label(Duration::seconds(1)), "pays again in 1m");
}
