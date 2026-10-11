use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{GuideOutcome, GuideService};
use crate::app::deadchannel::runner::state::Look;
use crate::test_helpers::new_test_db;

async fn answer(rx: &mut mpsc::UnboundedReceiver<GuideOutcome>) -> GuideOutcome {
    timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("an answer in time")
        .expect("the task answers")
}

#[tokio::test]
async fn the_first_descent_opens_the_guide_and_the_second_does_not() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "guide-svc").await;
    let client = test_db.db.get().await.expect("db client");
    let look = Look::random(1, &mut rand::thread_rng());
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    let svc = GuideService::new(test_db.db.clone());
    let (tx, mut rx) = mpsc::unbounded_channel();

    svc.claim_first_descent_task(user.id, tx.clone());
    assert_eq!(answer(&mut rx).await, GuideOutcome::FirstDescent);

    svc.claim_first_descent_task(user.id, tx.clone());
    assert_eq!(answer(&mut rx).await, GuideOutcome::SeenBefore);

    // No runner at all: seen before, as far as the street is concerned.
    let civilian = create_test_user(&test_db.db, "guide-svc-civilian").await;
    svc.claim_first_descent_task(civilian.id, tx);
    assert_eq!(answer(&mut rx).await, GuideOutcome::SeenBefore);
}
