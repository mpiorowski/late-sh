use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use late_core::models::{chat_room::ChatRoom, chat_room_member::ChatRoomMember, user::LandingPage};
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;

use super::state::{HANDOFF_IDLE, ResumePage, ResumePoint};
use crate::app::audio::client_state::{ClientAudioState, ClientKind};
use crate::app::common::primitives::Screen;
use crate::app::state::App;
use crate::paired_clients::{PAIR_CONTROL_QUEUE_CAP, PairControlMessage};
use crate::test_helpers::{
    SessionWorld, make_app_in_world, make_app_with_paired_client, new_test_db, wait_for_app,
};

/// Pair a native CLI that advertises `reconnect` on the app's token, the way
/// a current `late` does in native mode.
fn pair_reconnect_cli(app: &App) -> mpsc::Receiver<PairControlMessage> {
    let registry = app
        .paired_client_registry
        .clone()
        .expect("paired client registry");
    let (tx, rx) = mpsc::channel(PAIR_CONTROL_QUEUE_CAP);
    let registration_id = registry
        .register(
            app.session_token.clone(),
            tx,
            app.user_id,
            late_core::models::user::AudioSource::default(),
        )
        .expect("register reconnect cli");
    registry.update_state_and_enforce_mute_policy(
        &app.session_token,
        registration_id,
        ClientAudioState {
            client_kind: ClientKind::Cli,
            capabilities: vec!["reconnect".to_string()],
            ..ClientAudioState::default()
        },
    );
    rx
}

/// The `reconnect` resume points queued on `rx`, skipping the other controls
/// a tick may send.
fn reconnects(rx: &mut mpsc::Receiver<PairControlMessage>) -> Vec<String> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|msg| match msg {
            PairControlMessage::Reconnect { resume } => Some(resume),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_quiet_session_on_a_draining_pod_asks_its_cli_once_to_move_with_its_page() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "handoff-send-it").await;
    let (mut app, mut plain_rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "handoff-send-flow-it");
    let mut cli_rx = pair_reconnect_cli(&app);
    app.set_screen(Screen::Arcade);

    // Quiet, but the pod is not draining.
    app.last_active_at = Instant::now()
        .checked_sub(HANDOFF_IDLE + Duration::from_secs(1))
        .expect("a machine booted more than five minutes ago");
    app.tick();
    assert_eq!(
        reconnects(&mut cli_rx),
        Vec::<String>::new(),
        "nothing moves off a live pod"
    );

    // Draining, but someone just typed.
    app.is_draining.store(true, Ordering::Relaxed);
    app.last_active_at = Instant::now();
    app.tick();
    assert_eq!(
        reconnects(&mut cli_rx),
        Vec::<String>::new(),
        "an active session keeps the banner"
    );

    app.last_active_at = Instant::now()
        .checked_sub(HANDOFF_IDLE + Duration::from_secs(1))
        .expect("a machine booted more than five minutes ago");
    app.tick();
    let sent = reconnects(&mut cli_rx);
    let [resume] = sent.as_slice() else {
        panic!("the quiet session's cli is asked to move once, got {sent:?}");
    };
    let point = ResumePoint::parse(resume).expect("a resume point this server reads");
    assert_eq!(point.page, ResumePage::Arcade);
    assert_eq!(
        reconnects(&mut plain_rx),
        Vec::<String>::new(),
        "a client without the capability is never asked"
    );

    app.tick();
    assert_eq!(
        reconnects(&mut cli_rx),
        Vec::<String>::new(),
        "one handoff per session"
    );
}

#[tokio::test]
async fn a_moved_session_lands_on_its_page_unless_the_tour_still_has_to_run() {
    let test_db = new_test_db().await;
    for (idx, (page, landing, tour_pending, expected)) in [
        (
            ResumePage::Arcade,
            LandingPage::Clubhouse,
            false,
            Screen::Arcade,
        ),
        (
            ResumePage::Calendars,
            LandingPage::Zen,
            false,
            Screen::Calendars,
        ),
        (ResumePage::Zen, LandingPage::Home, false, Screen::Zen),
        (
            ResumePage::Clubhouse,
            LandingPage::Home,
            false,
            Screen::Clubhouse,
        ),
        // The tour only runs in the tavern, so it wins over the resume.
        (
            ResumePage::Arcade,
            LandingPage::Home,
            true,
            Screen::Clubhouse,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let user = create_test_user(&test_db.db, &format!("handoff-land-it-{idx}")).await;
        let app = make_app_in_world(
            test_db.db.clone(),
            user.id,
            &format!("handoff-land-flow-it-{idx}"),
            SessionWorld {
                landing_page: Some(landing),
                tour_pending,
                resume: Some(ResumePoint {
                    page,
                    room_id: None,
                }),
                ..SessionWorld::default()
            },
        );
        assert_eq!(
            app.screen, expected,
            "resume {page:?}, landing {landing:?}, tour pending {tour_pending}"
        );
    }
}

#[tokio::test]
async fn a_moved_session_selects_its_room_only_if_it_is_one_of_the_users_rooms() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "handoff-room-it").await;
    let client = test_db.db.get().await.expect("db client");
    let first = ChatRoom::get_or_create_public_room(&client, "handoff-room-first")
        .await
        .expect("first room");
    let second = ChatRoom::get_or_create_public_room(&client, "handoff-room-second")
        .await
        .expect("second room");
    let stranger = ChatRoom::get_or_create_public_room(&client, "handoff-room-stranger")
        .await
        .expect("room the user never joined");
    for room in [&first, &second] {
        ChatRoomMember::join(&client, room.id, user.id)
            .await
            .expect("join room");
    }

    for (idx, (resume_room, expected)) in [
        (first.id, Some(first.id)),
        (second.id, Some(second.id)),
        (stranger.id, None),
    ]
    .into_iter()
    .enumerate()
    {
        let mut app = make_app_in_world(
            test_db.db.clone(),
            user.id,
            &format!("handoff-room-flow-it-{idx}"),
            SessionWorld {
                resume: Some(ResumePoint {
                    page: ResumePage::Dashboard,
                    room_id: Some(resume_room),
                }),
                ..SessionWorld::default()
            },
        );
        wait_for_app(&mut app, "the room list arrives", |app| {
            app.chat.selected_room_id.is_some()
        })
        .await;
        let selected = app.chat.selected_room_id.expect("a selected room");
        match expected {
            Some(room_id) => assert_eq!(selected, room_id),
            None => assert_ne!(selected, stranger.id, "a room the user cannot see"),
        }
    }
}
