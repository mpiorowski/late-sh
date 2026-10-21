use dartboard_core::{Pos, RgbColor};
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::time::{Duration, timeout};

use super::PosterSession;
use crate::app::deadchannel::poster::state::{Edit, blank};
use crate::app::deadchannel::poster::svc::{PosterOutcome, PosterService};
use crate::app::deadchannel::runner::state::Look;
use crate::app::deadchannel::runner::svc::RunnerEntry;
use crate::test_helpers::new_test_db;

/// Ticks `session` until `done` holds.
async fn settle(session: &mut PosterSession, done: impl Fn(&PosterSession) -> bool) {
    timeout(Duration::from_secs(5), async {
        loop {
            session.tick();
            if done(session) {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the session settles in time");
}

/// Saving, stepping out and straight back in while the save is still out:
/// the panel opens on the poster the save hung, clean, even when the
/// database answers a wall read with the old poster first (the read
/// running before the save's update commits).
#[tokio::test]
async fn a_panel_reopened_while_a_save_is_out_opens_on_the_saved_poster() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "poster-session").await;
    let client = test_db.db.get().await.expect("db client");
    DeadchannelRunner::ensure_for_user(&client, user.id, &serde_json::json!({}))
        .await
        .expect("a runner");
    let mut old = blank();
    old.put_glyph_colored(Pos { x: 0, y: 0 }, 'O', RgbColor::new(1, 2, 3));
    DeadchannelRunner::store_poster(
        &client,
        user.id,
        &serde_json::to_value(&old).expect("a canvas serializes"),
    )
    .await
    .expect("the old poster hangs");

    let entry = RunnerEntry {
        look: Look::parse(&serde_json::json!({
            "hood": {"piece": "hood.cross", "tint": "amber"},
            "eyes": {"piece": "eyes.gem", "tint": "white"},
            "coat": {"piece": "coat.heavy", "tint": "static"},
            "mark": {"glyph": "▚"}
        }))
        .expect("a look"),
        level: 1,
        peak_level: 1,
        marks: 0,
    };
    let mut session = PosterSession::new(
        user.id,
        "poster-session".to_string(),
        PosterService::new(test_db.db.clone()),
    );
    session.open(Some(entry));
    settle(&mut session, |session| session.draft().is_some()).await;
    session
        .draft_mut()
        .expect("an open draft")
        .edit(Edit::Type('N'));
    let new = session.draft().expect("an open draft").canvas.clone();

    session.save();
    session.close();
    session.open(Some(entry));
    session
        .outcome_tx
        .send(PosterOutcome::Hung(Some(old)))
        .expect("the session listens");
    settle(&mut session, |session| {
        session.writing.is_none() && session.draft().is_some()
    })
    .await;

    let draft = session.draft().expect("an open draft");
    assert_eq!(draft.canvas, new);
    assert!(!draft.dirty());
    assert!(session.hung);
}
