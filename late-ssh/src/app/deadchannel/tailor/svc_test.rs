use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use rand::{SeedableRng, rngs::StdRng};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{TailorOutcome, TailorService};
use crate::app::deadchannel::runner::state::Look;
use crate::test_helpers::new_test_db;

async fn answer(rx: &mut mpsc::UnboundedReceiver<TailorOutcome>) -> TailorOutcome {
    timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("an answer in time")
        .expect("the task answers")
}

#[tokio::test]
async fn wearing_a_look_puts_it_on_the_row_while_the_runner_stands() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "tailor-svc").await;
    let client = test_db.db.get().await.expect("db client");
    let mut rng = StdRng::seed_from_u64(1);
    let born = Look::random(1, &mut rng);
    DeadchannelRunner::ensure_for_user(&client, user.id, &born.to_json())
        .await
        .expect("a runner");
    let svc = TailorService::new(test_db.db.clone());
    let (tx, mut rx) = mpsc::unbounded_channel();

    let mut rng = StdRng::seed_from_u64(2);
    let chosen = Look::random(1, &mut rng);
    assert_ne!(chosen, born);
    svc.wear_task(user.id, chosen, tx.clone());
    assert_eq!(answer(&mut rx).await, TailorOutcome::Worn(chosen));
    let row = DeadchannelRunner::find_by_user(&client, user.id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(Look::parse(&row.look).expect("a look"), chosen);

    // Gone dark: the mirror has nobody in it.
    DeadchannelRunner::mark_left(&client, user.id)
        .await
        .expect("leave");
    svc.wear_task(user.id, born, tx);
    assert_eq!(answer(&mut rx).await, TailorOutcome::NoRunner);
}
