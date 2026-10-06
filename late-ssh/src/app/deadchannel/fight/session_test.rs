use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::time::{Duration, timeout};

use super::{FightSession, Scene};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::FOES;
use crate::app::deadchannel::fight::state::{Applied, Call, Outcome, Refusal};
use crate::app::deadchannel::fight::state::{Command, Pick};
use crate::app::deadchannel::fight::svc::{FightOutcome, FightService};
use crate::app::deadchannel::runner::state::Look;
use crate::app::games::chips::svc::ChipService;
use crate::test_helpers::new_test_db;

async fn session_with_runner(name: &str) -> (late_core::test_utils::TestDb, FightSession) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let look = Look::random(1, &mut rand::thread_rng());
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    let chat = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let svc = FightService::new(
        test_db.db.clone(),
        chat,
        ChipService::new(test_db.db.clone()),
    );
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

/// Walk up to the static and onto the first step: the road opens, the
/// reload lands, Enter fights the glyph every road opens on. The step's
/// answer is still out when this returns.
async fn into_the_first_fight(session: &mut FightSession) {
    session.step_up();
    answered(session).await;
    session.enter();
}

#[tokio::test]
async fn one_action_is_out_at_a_time() {
    let (_test_db, mut session) = session_with_runner("fight-session-guard").await;

    into_the_first_fight(&mut session).await;
    assert!(
        !session.request(Command::EndTurn),
        "a press while the step is out is dropped"
    );
    answered(&mut session).await;

    let scene = session.scene.as_ref().expect("the scene stays open");
    assert!(!scene.waiting);
    assert_eq!(scene.lines, vec![FOES[0].arrives.to_string()]);
    assert!(
        session.request(Command::EndTurn),
        "the answer clears the guard"
    );
    answered(&mut session).await;
}

#[tokio::test]
async fn walking_up_again_shows_the_fight_the_row_remembers() {
    let (_test_db, mut session) = session_with_runner("fight-session-resume").await;

    into_the_first_fight(&mut session).await;
    answered(&mut session).await;
    session.close();
    assert!(!session.scene_open());

    session.step_up();
    assert!(
        !session.picker_open(),
        "a fight on the row never gets a map over it"
    );
    answered(&mut session).await;
    let scene = session.scene.as_ref().expect("the scene reopened");
    assert_eq!(
        *scene,
        Scene {
            lines: vec![FOES[0].arrives.to_string()],
            latest: 1,
            over: false,
            waiting: false,
            old_signal: false,
            failed: false,
        }
    );
}

#[tokio::test]
async fn walking_up_opens_the_road_and_a_step_onto_a_glyph_opens_the_scene() {
    let (_test_db, mut session) = session_with_runner("fight-session-road").await;

    session.step_up();
    assert!(session.picker_open());
    assert!(!session.scene_open());
    answered(&mut session).await;
    let picker = session.picker.as_ref().expect("the road stays open");
    assert_eq!(picker.lane, 1, "the cursor starts on the middle lane");
    assert!(
        picker.fair.is_some(),
        "the reload's sheet reads the fair fight"
    );
    assert_eq!(picker.lower, None, "nothing below the flicker");
    assert_eq!(session.open_lanes(), vec![0, 1, 2]);

    // The cursor walks the open lanes and holds at the ends.
    session.pick_up();
    session.pick_up();
    assert_eq!(session.picker.as_ref().map(|picker| picker.lane), Some(0));
    session.pick_down();
    session.pick_down();
    session.pick_down();
    assert_eq!(session.picker.as_ref().map(|picker| picker.lane), Some(2));

    // A call the node does not answer is not sent: every road opens on
    // glyphs, and a glyph is not a cache.
    session.call(Call::Take);
    assert!(session.picker_open());
    assert!(session.request(Command::Patch), "nothing was in flight");
    answered(&mut session).await;

    session.enter();
    assert!(!session.picker_open());
    assert!(session.scene_open());
    answered(&mut session).await;
    let sheet = session.sheet.as_ref().expect("the mirror");
    assert_eq!(
        sheet.road.lane(),
        Some(2),
        "the step went where the cursor was"
    );

    // Enter on a finished scene goes back to the road, not the street.
    session.leave_scene();
    assert!(session.picker_open());
    assert!(!session.scene_open());
    assert_eq!(
        session.picker.as_ref().map(|picker| picker.lane),
        Some(2),
        "the cursor is where the runner stands"
    );
}

#[tokio::test]
async fn a_failed_action_answers_where_it_was_asked_and_frees_the_guard() {
    let (_test_db, mut session) = session_with_runner("fight-session-failed").await;

    // On the scene, when one is open.
    into_the_first_fight(&mut session).await;
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
        Some("nobody at the counter is answering. try again.")
    );
    assert!(!session.action_in_flight);
}

#[tokio::test]
async fn a_step_key_on_a_road_that_is_over_closes_it() {
    let (_test_db, mut session) = session_with_runner("fight-session-spent").await;

    session.step_up();
    answered(&mut session).await;
    let mut spent = session.sheet.clone().expect("the mirror landed");
    spent.rations_left = 0;
    session.sheet = Some(spent);

    session.call(Call::Fight(Pick::Fair));

    assert!(!session.picker_open(), "the key closes the road");
    assert!(
        !session.scene_open(),
        "no scene opens only to carry the refusal the road already showed"
    );
}

/// A card the turn cannot pay for is said on the scene and the fight goes
/// on; any other refusal there is a step that never started, and ends it.
#[tokio::test]
async fn a_card_refused_keeps_the_scene_and_a_step_refused_ends_it() {
    let (_test_db, mut session) = session_with_runner("fight-session-refused").await;
    into_the_first_fight(&mut session).await;
    answered(&mut session).await;
    let sheet = session.sheet.clone().expect("the mirror");
    let refuse = |refusal, line: &str| FightOutcome::Acted {
        sheet: sheet.clone(),
        outcome: Outcome {
            applied: Applied::Refused(refusal),
            lines: vec![line.to_string()],
        },
    };

    session
        .outcome_tx
        .send(refuse(
            Refusal::NoEnergy,
            "not enough energy left this turn.",
        ))
        .expect("open");
    assert!(session.tick());
    let scene = session.scene.as_ref().expect("the scene");
    assert!(!scene.over, "the fight is still on");
    assert_eq!(
        scene.lines,
        vec![
            FOES[0].arrives.to_string(),
            "not enough energy left this turn.".to_string()
        ]
    );

    session
        .outcome_tx
        .send(refuse(Refusal::SignalDown, "your signal is down."))
        .expect("open");
    assert!(session.tick());
    let scene = session.scene.as_ref().expect("the scene");
    assert!(scene.over);
    assert_eq!(scene.lines, vec!["your signal is down.".to_string()]);
}

/// A draft owed, through the service and the row: the step keys do
/// nothing while the two cards are on the panel, `1` or `2` takes one,
/// and the pick is on the row the next reload reads.
#[tokio::test]
async fn a_draft_is_taken_on_the_road_and_kept_on_the_row() {
    let (test_db, mut session) = session_with_runner("fight-session-draft").await;
    session.step_up();
    answered(&mut session).await;
    // Level 3, the first draft's, written the way the fight loop writes.
    let mut sheet = session.sheet.clone().expect("the mirror landed");
    sheet.level = 3;
    sheet.peak_level = 3;
    sheet.signal = sheet.max_signal();
    let client = test_db.db.get().await.expect("db client");
    DeadchannelRunner::store_sheet(&client, sheet.to_write())
        .await
        .expect("the sheet stored");
    session.reload();
    answered(&mut session).await;
    let draft = session.draft().expect("a draft owed");

    session.enter();
    session.call(Call::Fight(Pick::Fair));
    assert!(session.picker_open(), "the road stays open on the draft");
    assert!(!session.scene_open(), "no step was sent");
    assert!(!session.take_card(2), "there is no third card");

    assert!(session.take_card(1));
    answered(&mut session).await;
    let sheet = session.sheet.as_ref().expect("the mirror");
    assert_eq!(sheet.cards, vec![draft.options[1]]);
    assert_eq!(session.draft(), None);
    assert_eq!(
        session.till.as_deref(),
        Some("siphon is in your deck now, in place of a strike."),
        "the pick is the road's word"
    );
    assert!(!session.take_card(0), "nothing is owed: the key is free");

    // The row kept it, and the road is open again.
    let row = DeadchannelRunner::lock_standing(&client, sheet.user_id)
        .await
        .expect("the row")
        .expect("a standing runner");
    assert_eq!(row.cards, Some(serde_json::json!(["siphon"])));
    session.enter();
    assert!(session.scene_open());
    answered(&mut session).await;
}
