//! App-level dashboard input integration tests against a real ephemeral DB.

mod helpers;

use helpers::{
    make_app, make_app_with_paired_client, new_test_db, render_plain, wait_for_render_contains,
    wait_until,
};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
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
async fn uppercase_p_on_dashboard_opens_install_pair_modal() {
    let (_test_db, mut app) = make_app_harness().await;

    app.handle_input(b"b");
    assert!(
        !render_plain(&mut app).contains("build from source"),
        "lowercase b should not open the install/pair modal"
    );

    app.handle_input(b"P");
    wait_for_render_contains(&mut app, "build from source").await;
    wait_for_render_contains(&mut app, "curl -fsSL https://cli.late.sh/install.sh | bash").await;
    wait_for_render_contains(&mut app, "irm https://cli.late.sh/install.ps1 | iex").await;
    wait_for_render_contains(&mut app, "alternatively pair browser").await;
}

#[tokio::test]
async fn mouse_move_does_not_close_cli_install_modal() {
    let (_test_db, mut app) = make_app_harness().await;

    app.handle_input(b"P");
    wait_for_render_contains(&mut app, "build from source").await;

    app.handle_input(b"\x1b[<35;20;5M");
    wait_for_render_contains(&mut app, "build from source").await;

    app.handle_input(b"x");
    assert!(!render_plain(&mut app).contains("build from source"));
}

#[tokio::test]
async fn uppercase_p_opens_pairing_qr_on_home() {
    let (_test_db, mut app) = make_app_harness().await;

    app.handle_input(b"P");
    wait_for_render_contains(&mut app, "Install CLI & Pair Browser").await;
    wait_for_render_contains(&mut app, "alternatively pair browser").await;
}

#[tokio::test]
async fn mouse_move_does_not_close_pairing_qr() {
    let (_test_db, mut app) = make_app_harness().await;

    app.handle_input(b"P");
    wait_for_render_contains(&mut app, "Install CLI & Pair Browser").await;

    app.handle_input(b"\x1b[<35;20;5M");
    wait_for_render_contains(&mut app, "Install CLI & Pair Browser").await;

    app.handle_input(b"x");
    assert!(!render_plain(&mut app).contains("Install CLI & Pair Browser"));
}

#[tokio::test]
async fn r_refresh_on_dashboard_keeps_dashboard_visible() {
    let (_test_db, mut app) = make_app_harness().await;

    wait_for_render_contains(&mut app, " Home ").await;
    app.handle_input(b"r");
    wait_for_render_contains(&mut app, " Home ").await;
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
async fn v_c_on_dashboard_votes_classic_when_no_message_is_selected() {
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
    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"v");
    app.handle_input(b"c");

    wait_until(
        || async {
            Vote::find_by_user(&client, user.id)
                .await
                .expect("load dashboard classic vote")
                .is_some_and(|vote| vote.genre == "classic")
        },
        "dashboard v+c to cast classic vote without a selected message",
    )
    .await;
}
