//! App input integration tests against a real ephemeral DB.

mod helpers;

use helpers::{make_app, new_test_db, wait_for_render_contains};
use late_core::models::{chat_room::ChatRoom, chat_room_member::ChatRoomMember};
use late_core::test_utils::create_test_user;

#[tokio::test]
async fn dashboard_chat_compose_blocks_quit_shortcut() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "popup-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "popup-flow-it");

    app.handle_input(b"i");
    wait_for_render_contains(
        &mut app,
        "Message (Enter send, Alt+Enter newline, Esc cancel)",
    )
    .await;

    app.handle_input(b"q$$$");
    wait_for_render_contains(&mut app, "$$$").await;
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn screen_number_keys_switch_between_dashboard_games_and_chat() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms (h/l)").await;

    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn active_game_blocks_screen_number_hotkeys() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "games-hotkey-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "games-hotkey-flow-it");

    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"\n");
    wait_for_render_contains(&mut app, " 2048 ").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " 2048 ").await;
}

#[tokio::test]
async fn dashboard_chat_compose_treats_screen_hotkeys_as_text() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dash-chat-compose-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "dash-chat-compose-flow-it");

    app.handle_input(b"i3abc");

    wait_for_render_contains(&mut app, " Dashboard ").await;
    wait_for_render_contains(&mut app, "3abc").await;
}

#[tokio::test]
async fn chat_compose_treats_screen_hotkeys_as_text() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-compose-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-compose-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms (h/l)").await;

    app.handle_input(b"i2hey");
    wait_for_render_contains(&mut app, "2hey").await;
    wait_for_render_contains(
        &mut app,
        "Compose (Enter send, Alt+Enter newline, Esc cancel)",
    )
    .await;

    // Real terminals send CR (0x0D) for Enter in raw mode. Bare LF (0x0A) is
    // Ctrl+J and is aliased to "insert newline in chat composer", so we'd
    // end up composing "2hey\n" instead of submitting.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Compose (press i)").await;
}
