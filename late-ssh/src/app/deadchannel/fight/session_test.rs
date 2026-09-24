use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::time::{Duration, timeout};

use super::{FightSession, Scene};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::FOES;
use crate::app::deadchannel::fight::state::Command;
use crate::app::deadchannel::fight::svc::{FightOutcome, FightService};
use crate::app::deadchannel::runner::state::Look;
use crate::test_helpers::new_test_db;

async fn session_with_runner(name: &str) -> (late_core::test_utils::TestDb, FightSession) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let look = Look::random(&mut rand::thread_rng());
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    let chat = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let svc = FightService::new(test_db.db.clone(), chat);
    let session = FightSession::new(user.id, "mira".to_string(), svc);
    (test_db, session)
}

/// Tick until an answer lands, as the app's tick loop would.
async fn answered(session: &mut FightSession) {
    timeout(Duration::from_secs(5), async {
        while !session.tick() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("an answer in time");
}

#[tokio::test]
async fn one_action_is_out_at_a_time() {
    let (_test_db, mut session) = session_with_runner("fight-session-guard").await;

    session.open();
    assert!(
        !session.request(Command::Attack),
        "a press while the start is out is dropped"
    );
    answered(&mut session).await;

    let scene = session.scene.as_ref().expect("the scene stays open");
    assert!(!scene.waiting);
    assert_eq!(scene.lines, vec![FOES[0].arrives.to_string()]);
    assert!(
        session.request(Command::Attack),
        "the answer clears the guard"
    );
    answered(&mut session).await;
}

#[tokio::test]
async fn a_second_step_in_shows_the_fight_the_row_remembers() {
    let (_test_db, mut session) = session_with_runner("fight-session-resume").await;

    session.open();
    answered(&mut session).await;
    session.close();
    assert!(!session.scene_open());

    session.open();
    answered(&mut session).await;
    let scene = session.scene.as_ref().expect("the scene reopened");
    assert_eq!(
        *scene,
        Scene {
            lines: vec![FOES[0].arrives.to_string()],
            latest: 1,
            over: false,
            waiting: false,
        }
    );
}

#[tokio::test]
async fn a_failed_action_answers_where_it_was_asked_and_frees_the_guard() {
    let (_test_db, mut session) = session_with_runner("fight-session-failed").await;

    // On the scene, when one is open.
    session.open();
    answered(&mut session).await;
    session.action_in_flight = true;
    session
        .outcome_tx
        .send(FightOutcome::ActionFailed)
        .expect("open");
    assert!(session.tick());
    let scene = session.scene.as_ref().expect("the scene stays open");
    assert!(!scene.waiting);
    assert_eq!(
        scene.lines.last().map(String::as_str),
        Some("the static is not answering. try again.")
    );
    assert!(!session.action_in_flight);

    // At the till, when no scene is open.
    session.close();
    session.action_in_flight = true;
    session
        .outcome_tx
        .send(FightOutcome::ActionFailed)
        .expect("open");
    assert!(session.tick());
    assert_eq!(
        session.till.as_deref(),
        Some("the armorer is not answering. try again.")
    );
    assert!(!session.action_in_flight);
}
