use std::time::Duration;

use chrono::{TimeZone, Utc};
use late_core::models::chat_room::ChatRoom;
use late_core::models::chat_room_member::ChatRoomMember;
use late_core::models::paper::PaperRoomEdition;
use late_core::test_utils::create_test_user;

use super::svc::{
    PAPER_ROOM_LINES, PaperEvent, PaperOutcome, PaperService, PaperTrigger, PressOutcome, PrintJob,
    edition_for, edition_window, tidy_column,
};
use crate::app::ai::svc::AiService;
use crate::test_helpers::{
    assert_render_not_contains_for, chat_compose_app, make_app, new_test_db, render_plain,
    test_app_flags_rx, wait_for_render_contains, wait_for_render_not_contains,
};
use late_core::models::paper::PaperEdition;

#[test]
fn an_edition_covers_the_whole_utc_day_before_it() {
    let now = Utc.with_ymd_and_hms(2026, 9, 3, 15, 30, 0).unwrap();
    let edition = edition_for(now);
    assert_eq!(
        edition,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 3).unwrap()
    );
    let (floor, ceiling) = edition_window(edition);
    assert_eq!(floor, Utc.with_ymd_and_hms(2026, 9, 2, 0, 0, 0).unwrap());
    assert_eq!(ceiling, Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap());
    // A minute past midnight is already the next edition.
    assert_eq!(
        edition_for(Utc.with_ymd_and_hms(2026, 9, 4, 0, 1, 0).unwrap()),
        chrono::NaiveDate::from_ymd_opt(2026, 9, 4).unwrap()
    );
}

#[test]
fn a_column_is_tidied_to_plain_bullets_under_the_cap() {
    let raw = "Here is the column:\n```\n* **alice** shipped the thing\n\n- bob argued about tabs, again\n• carol asked a question\n-dave left\n- extra one\n- extra two\n```\n";
    assert_eq!(
        tidy_column(raw, PAPER_ROOM_LINES).as_deref(),
        Some(
            "- Here is the column:\n- alice shipped the thing\n- bob argued about tabs, again\n- carol asked a question\n- dave left"
        )
    );
    assert_eq!(tidy_column("   \n\n", 5), None);
    assert_eq!(tidy_column("- one\n- two", 1).as_deref(), Some("- one"));
}

async fn wait_event(rx: &mut tokio::sync::broadcast::Receiver<PaperEvent>) -> PaperEvent {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("paper event in time")
        .expect("paper event")
}

/// The next newsstand answer as `(user, trigger, outcome)`; a press event
/// here is a test bug.
async fn wait_open(
    rx: &mut tokio::sync::broadcast::Receiver<PaperEvent>,
) -> (uuid::Uuid, PaperTrigger, PaperOutcome) {
    match wait_event(rx).await {
        PaperEvent::Open {
            user_id,
            trigger,
            outcome,
        } => (user_id, trigger, outcome),
        PaperEvent::Press { .. } => panic!("expected a newsstand answer"),
    }
}

async fn wait_press(rx: &mut tokio::sync::broadcast::Receiver<PaperEvent>) -> PressOutcome {
    match wait_event(rx).await {
        PaperEvent::Press { outcome, .. } => outcome,
        PaperEvent::Open { .. } => panic!("expected a press answer"),
    }
}

async fn seed_lounge_page(db: &late_core::db::Db, text: &str) -> ChatRoom {
    let client = db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let edition = edition_for(Utc::now());
    assert!(
        PaperRoomEdition::claim_printing(&client, lounge.id, edition, 12, 4, Utc::now())
            .await
            .expect("claim")
    );
    PaperRoomEdition::finish(&client, lounge.id, edition, text)
        .await
        .expect("finish");
    lounge
}

#[tokio::test]
async fn the_newsstand_answers_unavailable_empty_and_ready_and_claims_the_login_pop_once() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paper-reader").await;

    // Presses stopped: the paper is unavailable whatever the rows say.
    let (_stopped_tx, stopped_rx) =
        tokio::sync::watch::channel(Some(late_core::models::app_flag::AppFlags {
            haunt_enabled: true,
            haunt_live: false,
            paper_enabled: false,
            paper_outside_enabled: false,
        }));
    let dark = PaperService::new(test_db.db.clone(), AiService::new(false, None), stopped_rx);
    let mut dark_rx = dark.subscribe();
    dark.request(user.id, PaperTrigger::Command);
    assert!(matches!(
        wait_open(&mut dark_rx).await.2,
        PaperOutcome::Unavailable
    ));
    dark.request_print(user.id, PrintJob::Today);
    assert!(matches!(
        wait_press(&mut dark_rx).await,
        PressOutcome::Unavailable
    ));

    // Presses running but nothing printed: empty, and no login claim
    // spent. Reading needs no AI: the rows are the paper.
    let service = PaperService::new(
        test_db.db.clone(),
        AiService::new(false, None),
        test_app_flags_rx(),
    );
    let mut rx = service.subscribe();
    service.request(user.id, PaperTrigger::Login);
    let (_, trigger, outcome) = wait_open(&mut rx).await;
    assert_eq!(trigger, PaperTrigger::Login);
    assert!(matches!(outcome, PaperOutcome::Empty));

    // A printed page: the login pop is won once per account per edition,
    // the command reopens it for free every time.
    let lounge = seed_lounge_page(&test_db.db, "- someone said something").await;
    service.request(user.id, PaperTrigger::Login);
    let (_, _, outcome) = wait_open(&mut rx).await;
    let PaperOutcome::Ready(edition) = outcome else {
        panic!("expected a ready paper, got {outcome:?}");
    };
    assert_eq!(edition.rooms.len(), 1);
    assert_eq!(edition.rooms[0].room_id, lounge.id);
    assert_eq!(
        edition.rooms[0].text.as_deref(),
        Some("- someone said something")
    );

    // Second device, same day: nothing arrives for the login trigger.
    service.request(user.id, PaperTrigger::Login);
    service.request(user.id, PaperTrigger::Command);
    let (_, trigger, outcome) = wait_open(&mut rx).await;
    assert_eq!(trigger, PaperTrigger::Command);
    assert!(matches!(outcome, PaperOutcome::Ready(_)));
    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "a lost login claim must send nothing"
    );

    // Another account is its own claim.
    let other = create_test_user(&test_db.db, "paper-other").await;
    service.request(other.id, PaperTrigger::Login);
    let (user_id, _, outcome) = wait_open(&mut rx).await;
    assert_eq!(user_id, other.id);
    assert!(matches!(outcome, PaperOutcome::Ready(_)));

    // `/paper reset` drops the rows and the caller's stamp: the edition
    // reads empty again and the login pop can be won again.
    service.request_reset(user.id);
    assert!(matches!(wait_press(&mut rx).await, PressOutcome::Reset));
    {
        let client = test_db.db.get().await.expect("db client");
        let edition = PaperEdition::load(&client, edition_for(Utc::now()))
            .await
            .expect("load");
        assert!(edition.rooms.is_empty() && edition.sections.is_empty());
    }
    seed_lounge_page(&test_db.db, "- printed again").await;
    service.request(user.id, PaperTrigger::Login);
    let (_, trigger, outcome) = wait_open(&mut rx).await;
    assert_eq!(trigger, PaperTrigger::Login);
    assert!(matches!(outcome, PaperOutcome::Ready(_)));

    // A preview (tomorrow's edition) is what `/paper` shows once it exists,
    // and the login pop never looks at it.
    let tomorrow = edition_for(Utc::now()) + chrono::Duration::days(1);
    {
        let client = test_db.db.get().await.expect("db client");
        let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
        assert!(
            PaperRoomEdition::claim_printing(&client, lounge.id, tomorrow, 3, 1, Utc::now())
                .await
                .expect("claim")
        );
        PaperRoomEdition::finish(&client, lounge.id, tomorrow, "- the preview")
            .await
            .expect("finish");
    }
    service.request(user.id, PaperTrigger::Command);
    let (_, _, outcome) = wait_open(&mut rx).await;
    let PaperOutcome::Ready(edition) = outcome else {
        panic!("expected the preview, got {outcome:?}");
    };
    assert_eq!(edition.edition, tomorrow);
    assert_eq!(edition.rooms[0].text.as_deref(), Some("- the preview"));
    service.request(other.id, PaperTrigger::Login);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "today's pop was already spent and the preview must not pop"
    );
}

#[tokio::test]
async fn the_login_pop_opens_once_after_the_splash_and_esc_closes_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paper-login").await;
    seed_lounge_page(&test_db.db, "- alice fixed the build, bob broke it again").await;

    {
        let client = test_db.db.get().await.expect("db client");
        let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
        ChatRoomMember::join(&client, lounge.id, user.id)
            .await
            .expect("join lounge");
    }
    let mut app = make_app(test_db.db.clone(), user.id, "paper-login-token");
    app.skip_splash_for_tests();
    // Rooms must be in before the paper lays out, or lounge would read as
    // a room the viewer is not in.
    wait_for_render_contains(&mut app, "lounge").await;
    // The test harness builds sessions with the tweak off; arm it by hand.
    app.paper.login_pop_pending = true;

    wait_for_render_contains(&mut app, "The Late Edition").await;
    let frame = render_plain(&mut app);
    assert!(frame.contains("YOUR ROOMS"), "{frame}");
    assert!(frame.contains("#lounge · 12 messages"), "{frame}");
    assert!(
        frame.contains("- alice fixed the build, bob broke it again"),
        "{frame}"
    );

    app.handle_input(b"\x1b");
    wait_for_render_not_contains(&mut app, "The Late Edition").await;

    // The account's pop for this edition is spent: arming it again on a
    // second device pops nothing.
    app.paper.login_pop_pending = true;
    assert_render_not_contains_for(&mut app, "The Late Edition", Duration::from_millis(400)).await;
}

#[tokio::test]
async fn slash_paper_reopens_the_edition_and_a_non_admin_cannot_stop_the_presses() {
    let (test_db, mut app) = chat_compose_app("paper-cmd").await;
    seed_lounge_page(&test_db.db, "- the lounge talked about lunch").await;

    app.handle_input(b"/paper\r");
    wait_for_render_contains(&mut app, "the lounge talked about lunch").await;
    app.handle_input(b"q");
    wait_for_render_not_contains(&mut app, "The Late Edition").await;

    app.handle_input(b"i");
    app.handle_input(b"/paper off\r");
    wait_for_render_contains(&mut app, "Only admins can touch the presses").await;
}
