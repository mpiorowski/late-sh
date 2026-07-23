use super::*;
use chrono::TimeZone;

#[test]
fn food_is_due_every_two_days_while_water_is_daily() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
    let yesterday = Utc.with_ymd_and_hms(2026, 5, 19, 12, 0, 0).unwrap();
    let two_days = Utc.with_ymd_and_hms(2026, 5, 18, 12, 0, 0).unwrap();
    let three_days = Utc.with_ymd_and_hms(2026, 5, 17, 12, 0, 0).unwrap();

    assert_eq!(
        need_after(Some(yesterday), today, FOOD_DUE_AFTER_DAYS),
        PetNeedStatus::Done
    );
    assert_eq!(
        need_after(Some(two_days), today, FOOD_DUE_AFTER_DAYS),
        PetNeedStatus::Due
    );
    assert_eq!(
        need_after(Some(three_days), today, FOOD_DUE_AFTER_DAYS),
        PetNeedStatus::Overdue
    );
    assert_eq!(
        need_after(Some(yesterday), today, DAILY_DUE_AFTER_DAYS),
        PetNeedStatus::Due
    );
    assert_eq!(
        need_after(Some(two_days), today, DAILY_DUE_AFTER_DAYS),
        PetNeedStatus::Overdue
    );
}

#[test]
fn weighted_needs_drive_mood() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
    let cared = PetNeeds {
        food: PetNeedStatus::Done,
        water: PetNeedStatus::Done,
    };
    assert_eq!(
        mood_for_state(cared, HAPPY_CARE_STREAK_DAYS, Some(today), today),
        PetMood::Happy
    );
    assert_eq!(
        mood_for_state(cared, HAPPY_CARE_STREAK_DAYS - 1, Some(today), today),
        PetMood::Content
    );
    assert_eq!(
        mood_for_state(
            cared,
            HAPPY_CARE_STREAK_DAYS,
            Some(today.pred_opt().unwrap()),
            today
        ),
        PetMood::Content
    );

    // A due water bowl reads thirsty, matching the amber bowl beside it.
    assert_eq!(
        mood_for_state(
            PetNeeds {
                water: PetNeedStatus::Due,
                ..cared
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Thirsty
    );
    assert_eq!(
        mood_for_state(
            PetNeeds {
                water: PetNeedStatus::Overdue,
                ..cared
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Thirsty
    );
    assert_eq!(
        mood_for_state(
            PetNeeds {
                food: PetNeedStatus::Due,
                ..cared
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Hungry
    );
    // Score 50 sits exactly on the sad bar, so food still leads.
    assert_eq!(
        mood_for_state(
            PetNeeds {
                food: PetNeedStatus::Due,
                water: PetNeedStatus::Overdue,
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Hungry
    );
    // Overdue food alone (45) is sad on the score, with water fully done.
    assert_eq!(
        mood_for_state(
            PetNeeds {
                food: PetNeedStatus::Overdue,
                ..cared
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Sad
    );
    assert_eq!(
        mood_for_state(
            PetNeeds {
                food: PetNeedStatus::Overdue,
                water: PetNeedStatus::Due,
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Sad
    );
    assert_eq!(
        mood_for_state(
            PetNeeds {
                food: PetNeedStatus::Overdue,
                water: PetNeedStatus::Overdue,
            },
            HAPPY_CARE_STREAK_DAYS,
            Some(today),
            today,
        ),
        PetMood::Sad
    );
}

#[test]
fn completed_care_streak_advances_by_calendar_day() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
    let yesterday = today.pred_opt().unwrap();
    let two_days_ago = yesterday.pred_opt().unwrap();

    assert_eq!(next_care_streak_days(0, None, today), 1);
    assert_eq!(next_care_streak_days(1, Some(today), today), 1);
    assert_eq!(next_care_streak_days(2, Some(yesterday), today), 3);
    assert_eq!(next_care_streak_days(8, Some(two_days_ago), today), 1);
}

#[test]
fn care_score_weights_food_more_than_water() {
    let cared = PetNeeds {
        food: PetNeedStatus::Done,
        water: PetNeedStatus::Done,
    };
    assert_eq!(
        PetNeeds {
            water: PetNeedStatus::Due,
            ..cared
        }
        .care_score(),
        90
    );
    assert_eq!(
        PetNeeds {
            food: PetNeedStatus::Due,
            ..cared
        }
        .care_score(),
        75
    );
    assert_eq!(
        PetNeeds {
            food: PetNeedStatus::Overdue,
            ..cared
        }
        .care_score(),
        45
    );
}

#[tokio::test]
async fn sparse_ticks_expire_feedback_on_the_wall_clock() {
    use crate::test_helpers::new_test_db;
    use late_core::test_utils::create_test_user;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "cat-wall-tick").await;
    let svc = super::super::svc::PetService::new(test_db.db.clone());
    let cat = svc.ensure_cat(user.id).await.expect("ensure cat");
    let mut state = PetState::new(user.id, svc, cat);

    state.tick(10);
    state.set_feedback("fed");

    // The adaptive loop ticks sparsely: one call covering the whole
    // feedback window must expire it, same wall time as dense ticking.
    assert!(
        state.tick(10 + FEEDBACK_TICKS),
        "feedback expiry must report changed"
    );
    assert!(state.action_feedback.is_none());
    assert_eq!(
        state.animation_ticks(),
        10 + FEEDBACK_TICKS,
        "animation clock syncs to the wall tick, not the call count"
    );
}
