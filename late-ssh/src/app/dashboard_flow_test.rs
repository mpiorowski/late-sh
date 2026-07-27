//! App-level dashboard input integration tests against a real ephemeral DB.

use crate::paired_clients::PairControlMessage;
use crate::test_helpers::{
    make_app, make_app_with_paired_client, new_test_db, render_plain, wait_for_render_contains,
};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
};
use late_core::test_utils::create_test_user;

async fn make_app_harness() -> (late_core::test_utils::TestDb, crate::app::state::App) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "todo-it").await;
    let app = make_app(test_db.db.clone(), user.id, "todo-flow-it");
    (test_db, app)
}

#[tokio::test]
async fn guide_routing_preserves_dashboard_content_input_and_lateania_context() {
    let (_test_db, mut app) = make_app_harness().await;

    wait_for_render_contains(&mut app, " Home ").await;

    app.handle_input(b"\x12");
    let frame = render_plain(&mut app);
    assert!(!frame.contains("Install `late` / Listen Anywhere"));
    assert!(!frame.contains("Browser pairing"));

    // The old terminal FAQ byte no longer opens a standalone modal; those
    // topics now live in the guide.
    app.handle_input(b"\x0c");
    assert!(
        !render_plain(&mut app).contains("Why copy sometimes silently fails"),
        "the legacy help byte must remain inert"
    );

    app.handle_input(b"b");
    assert!(
        !render_plain(&mut app).contains("Install `late` / Listen Anywhere"),
        "lowercase b should not open the guide"
    );

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Install `late` / Listen Anywhere").await;
    wait_for_render_contains(&mut app, "https://cli.late.sh/install.sh | bash").await;
    wait_for_render_contains(&mut app, "https://cli.late.sh/install.ps1 | iex").await;
    wait_for_render_contains(&mut app, "What `late` unlocks").await;
    wait_for_render_contains(&mut app, "CLI YouTube").await;
    wait_for_render_contains(&mut app, "?/Esc/q close").await;

    app.handle_input(b"\x1b[<35;20;5M");
    wait_for_render_contains(&mut app, "Install `late` / Listen Anywhere").await;

    app.handle_input(b"?");
    assert!(
        !render_plain(&mut app).contains("Install `late` / Listen Anywhere"),
        "? should close the guide"
    );

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Install `late` / Listen Anywhere").await;
    for _ in 0..30 {
        app.handle_input(b"j");
    }
    wait_for_render_contains(&mut app, "Listen without the CLI").await;
    wait_for_render_contains(&mut app, "█▀▀▀▀▀█").await;

    app.handle_input(b"q");
    assert!(
        !render_plain(&mut app).contains("Listen without the CLI"),
        "q should close the guide"
    );

    // Lateania has no top-level key: open Games and launch its default card,
    // then verify that ? selects the screen-specific guide section.
    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " Games ").await;
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, " Lateania ").await;

    app.handle_input(b"?");
    wait_for_render_contains(&mut app, "Lateania is the persistent BBS-style world").await;
    assert!(
        !render_plain(&mut app).contains("Install `late` / Listen Anywhere"),
        "Lateania should select its own guide section"
    );
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
async fn c_on_dashboard_copies_selected_message() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dashboard-copy-priority-it").await;
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge room");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
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
}
