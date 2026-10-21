use dartboard_core::{Pos, RgbColor};
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{PosterOutcome, PosterService};
use crate::app::deadchannel::poster::state::blank;
use crate::test_helpers::new_test_db;

async fn answer(rx: &mut mpsc::UnboundedReceiver<PosterOutcome>) -> PosterOutcome {
    timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("an answer in time")
        .expect("the task answers")
}

#[tokio::test]
async fn prints_hangs_replaces_and_burns_while_the_runner_stands() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "poster-svc").await;
    let client = test_db.db.get().await.expect("db client");
    DeadchannelRunner::ensure_for_user(&client, user.id, &serde_json::json!({}))
        .await
        .expect("a runner");
    let svc = PosterService::new(test_db.db.clone());
    let (tx, mut rx) = mpsc::unbounded_channel();
    let read = async |rx: &mut mpsc::UnboundedReceiver<PosterOutcome>| {
        svc.read_task(user.id, "poster-svc".to_string(), tx.clone());
        answer(rx).await
    };

    assert_eq!(read(&mut rx).await, PosterOutcome::Hung(None));

    let mut first = blank();
    first.put_glyph_colored(Pos { x: 0, y: 0 }, '@', RgbColor::new(1, 2, 3));
    let mut second = blank();
    second.put_glyph_colored(Pos { x: 29, y: 11 }, '界', RgbColor::new(4, 5, 6));
    second.put_glyph_colored(Pos { x: 10, y: 4 }, '▓', RgbColor::new(7, 8, 9));
    for canvas in [first, second] {
        svc.save_task(
            user.id,
            "poster-svc".to_string(),
            canvas.clone(),
            tx.clone(),
        );
        assert_eq!(answer(&mut rx).await, PosterOutcome::Saved(canvas.clone()));
        assert_eq!(read(&mut rx).await, PosterOutcome::Hung(Some(canvas)));
    }

    svc.burn_task(user.id, "poster-svc".to_string(), tx.clone());
    assert_eq!(answer(&mut rx).await, PosterOutcome::Burned);
    assert_eq!(read(&mut rx).await, PosterOutcome::Hung(None));

    // Gone dark: nothing to hang a poster on, nothing to burn.
    DeadchannelRunner::mark_left(&client, user.id)
        .await
        .expect("leave");
    svc.save_task(user.id, "poster-svc".to_string(), blank(), tx.clone());
    assert_eq!(answer(&mut rx).await, PosterOutcome::NoRunner);
    svc.burn_task(user.id, "poster-svc".to_string(), tx.clone());
    assert_eq!(answer(&mut rx).await, PosterOutcome::NoRunner);
}

#[tokio::test]
async fn a_poster_that_does_not_decode_reads_as_a_bare_wall() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "poster-bad").await;
    let client = test_db.db.get().await.expect("db client");
    DeadchannelRunner::ensure_for_user(&client, user.id, &serde_json::json!({}))
        .await
        .expect("a runner");
    DeadchannelRunner::store_poster(&client, user.id, &serde_json::json!({"not": "a canvas"}))
        .await
        .expect("store");
    let svc = PosterService::new(test_db.db.clone());
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.read_task(user.id, "poster-bad".to_string(), tx);
    assert_eq!(answer(&mut rx).await, PosterOutcome::Hung(None));
}
