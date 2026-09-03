use std::time::Duration;

use chrono::{TimeZone, Utc};
use late_core::models::chat_room::ChatRoom;
use late_core::models::chat_room_member::ChatRoomMember;
use late_core::models::paper::PaperRoomEdition;
use late_core::test_utils::create_test_user;

use super::svc::{
    PAPER_MAX_ATTEMPTS, PAPER_ROOM_LINES, PaperEvent, PaperOutcome, PaperService, PaperTrigger,
    PressOutcome, PrintJob, edition_for, edition_window, outside_prompt, tidy_column,
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
    seed_lounge_page_for(db, edition_for(Utc::now()), text).await
}

async fn seed_lounge_page_for(
    db: &late_core::db::Db,
    edition: chrono::NaiveDate,
    text: &str,
) -> ChatRoom {
    let client = db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    assert!(
        PaperRoomEdition::claim_printing(
            &client,
            lounge.id,
            edition,
            12,
            4,
            Utc::now(),
            PAPER_MAX_ATTEMPTS
        )
        .await
        .expect("claim")
    );
    PaperRoomEdition::finish(&client, lounge.id, edition, Some(text))
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

    // The newsstand sells today's edition and nothing else: rows dated
    // tomorrow (there should be none; a preview never writes any) are
    // neither shown by `/paper` nor popped at login.
    let today = edition_for(Utc::now());
    seed_lounge_page_for(&test_db.db, today + chrono::Duration::days(1), "- tomorrow").await;
    service.request(user.id, PaperTrigger::Command);
    let (_, _, outcome) = wait_open(&mut rx).await;
    let PaperOutcome::Ready(edition) = outcome else {
        panic!("expected today's paper, got {outcome:?}");
    };
    assert_eq!(edition.edition, today);
    assert_eq!(edition.rooms[0].text.as_deref(), Some("- printed again"));
    service.request(other.id, PaperTrigger::Login);
    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "today's pop was already spent and tomorrow's rows must not pop"
    );

    // A preview needs the press (a configured AI service), which the test
    // harness does not have, so it is unavailable here rather than a
    // model call; the in-memory layout itself is `preview_edition`.
    service.request_print(user.id, PrintJob::Preview);
    assert!(matches!(
        wait_press(&mut rx).await,
        PressOutcome::Unavailable
    ));
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

#[tokio::test]
async fn a_newcomers_paper_waits_until_the_tour_is_walked() {
    use crate::app::clubhouse::state::Tutorial;
    use crate::app::common::primitives::Screen;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paper-newcomer").await;
    seed_lounge_page(&test_db.db, "- the regulars argued about editors").await;
    {
        let client = test_db.db.get().await.expect("db client");
        let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
        ChatRoomMember::join(&client, lounge.id, user.id)
            .await
            .expect("join lounge");
    }

    let mut app = make_app(test_db.db.clone(), user.id, "paper-newcomer-token");
    app.skip_splash_for_tests();
    wait_for_render_contains(&mut app, "lounge").await;
    // A first-ever session: land in the tavern with the walkthrough pending,
    // and the paper armed like any other login.
    app.set_screen(Screen::Clubhouse);
    app.clubhouse.tutorial = Tutorial::Pending;
    app.paper.login_pop_pending = true;
    assert_render_not_contains_for(&mut app, "The Late Edition", Duration::from_millis(300)).await;
    app.clubhouse.enter_screen();
    assert_eq!(app.clubhouse.tutorial, Tutorial::Welcome);

    // Nothing pops while the tour holds the keys, however long it takes.
    for bytes in [&b"1"[..], b"\r", b"2", b"\r", b"3", b"4", b"5", b"6"] {
        app.handle_input(bytes);
        let frame = render_plain(&mut app);
        assert!(!frame.contains("The Late Edition"), "{frame}");
    }
    app.handle_input(b"0");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Homecoming);
    assert_render_not_contains_for(&mut app, "The Late Edition", Duration::from_millis(300)).await;

    // Settling in is the last step of the opening; the paper is next.
    app.handle_input(b"\r");
    assert_eq!(app.clubhouse.tutorial, Tutorial::Done);
    wait_for_render_contains(&mut app, "The Late Edition").await;
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("- the regulars argued about editors"),
        "{frame}"
    );
}

#[test]
fn the_outside_prompt_carries_the_date_and_the_earlier_editions() {
    let edition = chrono::NaiveDate::from_ymd_opt(2026, 9, 4).unwrap();
    let bare = outside_prompt(edition, &[]);
    assert!(
        bare.starts_with("Today is Friday, September 4 2026."),
        "{bare}"
    );
    assert!(!bare.contains("Already covered"), "{bare}");
    assert!(bare.ends_with("Output ONLY the lines."), "{bare}");

    let covered = vec![
        (
            edition.pred_opt().unwrap(),
            "- a kernel shipped".to_string(),
        ),
        (
            edition - chrono::Duration::days(3),
            "- an outage\n- a ruling".to_string(),
        ),
    ];
    let remembered = outside_prompt(edition, &covered);
    assert!(
        remembered.contains(
            "Already covered in earlier editions, do not repeat:\n[2026-09-03]\n- a kernel shipped\n[2026-09-01]\n- an outage\n- a ruling\n"
        ),
        "{remembered}"
    );
}
