use super::*;
use chrono::TimeZone;
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;

use crate::app::activity::event::ActivityEvent;
use crate::test_helpers::new_test_db;

#[test]
fn mood_flips_on_the_utc_day() {
    let today = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
    let this_morning = Utc.with_ymd_and_hms(2026, 5, 20, 0, 5, 0).unwrap();
    let last_night = Utc.with_ymd_and_hms(2026, 5, 19, 23, 55, 0).unwrap();

    assert_eq!(mood_for(Some(this_morning), today), PetMood::Happy);
    // Fed five minutes before midnight UTC: a new day, a new meal owed.
    assert_eq!(mood_for(Some(last_night), today), PetMood::Sad);
    assert_eq!(mood_for(None, today), PetMood::Sad);
}

async fn fresh_state(handle: &str) -> PetState {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, handle).await;
    let (tx, _) = broadcast::channel::<ActivityEvent>(16);
    let svc = super::super::svc::PetService::new(test_db.db.clone(), tx);
    let cat = svc.ensure_cat(user.id).await.expect("ensure cat");
    PetState::new(user.id, svc, cat)
}

#[tokio::test]
async fn one_free_meal_a_day_turns_the_mood_around() {
    let mut state = fresh_state("cat-daily-meal").await;
    assert_eq!(state.mood(), PetMood::Sad);
    assert!(!state.fed_today());

    assert_eq!(state.feed(), FeedOutcome::Fed);
    assert_eq!(state.mood(), PetMood::Happy);
    assert!(state.fed_today());
    assert_eq!(state.action_feedback.as_deref(), Some("fed!"));

    assert_eq!(state.feed(), FeedOutcome::AlreadyFedToday);
    assert_eq!(state.action_feedback.as_deref(), Some("already fed today"));

    state.claim_fed_chips(100);
    assert_eq!(state.action_feedback.as_deref(), Some("fed! +100 chips"));
}

#[tokio::test]
async fn sparse_ticks_expire_feedback_on_the_wall_clock() {
    let mut state = fresh_state("cat-wall-tick").await;

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
