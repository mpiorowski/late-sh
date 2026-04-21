//! App-level dashboard input integration tests against a real ephemeral DB.

mod helpers;

use helpers::{
    make_app, make_app_with_paired_client, new_test_db, wait_for_render_contains, wait_until,
};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    profile::{Profile, ProfileParams},
    vote::Vote,
};
use late_core::test_utils::create_test_user;
use late_ssh::session::PairControlMessage;
use tokio::time::{Duration, Instant, sleep};

async fn make_app_harness() -> (late_core::test_utils::TestDb, late_ssh::app::state::App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "todo-it").await;
    let app = make_app(test_db.db.clone(), user.id, "todo-flow-it");
    (test_db, app)
}

#[tokio::test]
async fn enter_on_dashboard_shows_url_copied_banner() {
    let (_test_db, mut app) = make_app_harness().await;

    app.handle_input(b"\n");
    wait_for_render_contains(&mut app, "CLI install command copied!").await;
}

#[tokio::test]
async fn r_refresh_on_dashboard_keeps_dashboard_visible() {
    let (_test_db, mut app) = make_app_harness().await;

    wait_for_render_contains(&mut app, " Dashboard ").await;
    app.handle_input(b"r");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn m_on_dashboard_sends_toggle_to_paired_client() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paired-browser-it").await;
    let (mut app, mut rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "paired-browser-flow-it");

    app.handle_input(b"m");

    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::ToggleMute);
    wait_for_render_contains(&mut app, "Sent mute toggle to paired client").await;
}

#[tokio::test]
async fn plus_and_minus_send_volume_controls_to_paired_client() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "paired-volume-it").await;
    let (mut app, mut rx) =
        make_app_with_paired_client(test_db.db.clone(), user.id, "paired-volume-flow-it");

    app.handle_input(b"+");
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::VolumeUp);
    wait_for_render_contains(&mut app, "Sent volume up to paired client").await;

    app.handle_input(b"-");
    assert_eq!(rx.try_recv().unwrap(), PairControlMessage::VolumeDown);
    wait_for_render_contains(&mut app, "Sent volume down to paired client").await;
}

#[tokio::test]
async fn c_on_dashboard_copies_selected_message_before_voting_classic() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dashboard-copy-priority-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: user.id,
            body: "copy me from dashboard".to_string(),
        },
    )
    .await
    .expect("create dashboard message");

    let mut app = make_app(
        test_db.db.clone(),
        user.id,
        "dashboard-copy-priority-flow-it",
    );
    wait_for_render_contains(&mut app, "copy me from dashboard").await;

    app.handle_input(b"j");
    app.handle_input(b"c");
    wait_for_render_contains(&mut app, "Message copied to clipboard!").await;

    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        let vote = Vote::find_by_user(&client, user.id)
            .await
            .expect("load vote after dashboard copy");
        assert!(
            vote.is_none(),
            "expected no vote to be recorded when copying a selected dashboard message"
        );
        sleep(Duration::from_millis(30)).await;
    }
}

#[tokio::test]
async fn c_on_dashboard_still_votes_classic_when_no_message_is_selected() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dashboard-classic-vote-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");

    let mut app = make_app(
        test_db.db.clone(),
        user.id,
        "dashboard-classic-vote-flow-it",
    );
    wait_for_render_contains(&mut app, " Dashboard ").await;

    app.handle_input(b"c");

    wait_until(
        || async {
            Vote::find_by_user(&client, user.id)
                .await
                .expect("load dashboard classic vote")
                .is_some_and(|vote| vote.genre == "classic")
        },
        "dashboard c to cast classic vote without a selected message",
    )
    .await;
}

#[tokio::test]
async fn dashboard_lazy_primes_favorite_histories_without_opening_chat() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dashboard-prime-user-it").await;
    let author = create_test_user(&test_db.db, "dashboard-prime-author-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    let alpha = ChatRoom::get_or_create_public_room(&client, "alpha-prime")
        .await
        .expect("create alpha room");
    let beta = ChatRoom::get_or_create_public_room(&client, "beta-prime")
        .await
        .expect("create beta room");

    for room in [general.id, alpha.id, beta.id] {
        ChatRoomMember::join(&client, room, user.id)
            .await
            .expect("join viewer");
        ChatRoomMember::join(&client, room, author.id)
            .await
            .expect("join author");
    }

    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: author.id,
            body: "general seed".to_string(),
        },
    )
    .await
    .expect("create general message");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: alpha.id,
            user_id: author.id,
            body: "alpha backlog".to_string(),
        },
    )
    .await
    .expect("create alpha message");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: beta.id,
            user_id: author.id,
            body: "beta backlog".to_string(),
        },
    )
    .await
    .expect("create beta message");

    Profile::update(
        &client,
        user.id,
        ProfileParams {
            username: "dashboard-prime-user-it".to_string(),
            bio: String::new(),
            country: None,
            timezone: None,
            notify_kinds: Vec::new(),
            notify_bell: false,
            notify_cooldown_mins: 0,
            notify_format: None,
            theme_id: Some("late".to_string()),
            enable_background_color: false,
            show_dashboard_header: true,
            show_right_sidebar: true,
            show_games_sidebar: true,
            favorite_room_ids: vec![alpha.id, beta.id],
        },
    )
    .await
    .expect("update favorites");

    let mut app = make_app(test_db.db.clone(), user.id, "dashboard-prime-flow-it");

    wait_for_render_contains(&mut app, "alpha backlog").await;

    app.handle_input(b"]");
    wait_for_render_contains(&mut app, "beta backlog").await;
}
