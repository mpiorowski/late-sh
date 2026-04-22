//! App input integration tests against a real ephemeral DB.

mod helpers;

use helpers::{
    chat_compose_app, make_app, make_app_with_chat_service, new_test_db, render_plain,
    wait_for_render_contains, wait_until,
};
use late_core::models::{
    chat_message::{ChatMessage, ChatMessageParams},
    chat_message_reaction::ChatMessageReaction,
    chat_room::ChatRoom,
    chat_room_member::ChatRoomMember,
    user::User,
};
use late_core::test_utils::create_test_user;
use rstest::rstest;
use tokio::time::Duration;
use uuid::Uuid;

#[tokio::test]
async fn dashboard_chat_compose_blocks_quit_shortcut() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "popup-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "popup-flow-it");

    // Hop through the chat screen first so the async room snapshot has
    // definitely landed: `> general` only renders once `drain_snapshot`
    // populates `general_room_id`, which the dashboard `i` handler needs.
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "> general").await;
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;

    app.handle_input(b"i");
    wait_for_render_contains(
        &mut app,
        "Compose (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel)",
    )
    .await;

    app.handle_input(b"q$$$");
    wait_for_render_contains(&mut app, "$$$").await;
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn q_opens_quit_confirm_and_escape_dismisses_it() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "quit-confirm-it").await;
    let mut app = make_app(test_db.db.clone(), user.id, "quit-confirm-flow-it");

    app.handle_input(b"q");
    wait_for_render_contains(&mut app, " Quit? ").await;
    wait_for_render_contains(&mut app, "Clicked by mistake, right?").await;
    wait_for_render_contains(&mut app, "bye, I'll be back").await;
    wait_for_render_contains(&mut app, "yeah, my bad, stay").await;

    app.handle_input(b"\x1b");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let frame = render_plain(&mut app);
    assert!(
        !frame.contains("Clicked by mistake, right?"),
        "expected quit confirm to dismiss after Esc; frame={frame:?}"
    );
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
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"3");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;
}

#[tokio::test]
async fn shift_tab_cycles_screens_backwards() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "screen-backtab-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "screen-backtab-flow-it");

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " The Arcade ").await;

    app.handle_input(b"\x1b[Z");
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"\x1b[Z");
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
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "dash-chat-compose-flow-it");

    // See `dashboard_chat_compose_blocks_quit_shortcut` — hop through chat
    // once to guarantee the room snapshot has populated `general_room_id`.
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, "> general").await;
    app.handle_input(b"1");
    wait_for_render_contains(&mut app, " Dashboard ").await;

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
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"i2hey");
    wait_for_render_contains(&mut app, "2hey").await;
    wait_for_render_contains(
        &mut app,
        "Compose (Enter send, Alt+S stay, Alt+Enter newline, Esc cancel)",
    )
    .await;

    // Real terminals send CR (0x0D) for Enter in raw mode. Bare LF (0x0A) is
    // Ctrl+J and is aliased to "insert newline in chat composer", so we'd
    // end up composing "2hey\n" instead of submitting.
    app.handle_input(b"\r");
    wait_for_render_contains(&mut app, "Compose (press i)").await;
}

#[rstest]
#[case::cyrillic("cyrillic", "тест")]
#[case::han("han", "漢字")]
#[case::latin_diacritic("accented", "café")]
#[case::greek("greek", "αβγ")]
#[tokio::test]
async fn chat_compose_accepts_non_ascii_typing(#[case] label: &str, #[case] input: &str) {
    let (_db, mut app) = chat_compose_app(&format!("utf8-{label}")).await;
    app.handle_input(input.as_bytes());
    wait_for_render_contains(&mut app, input).await;
}

#[tokio::test]
async fn split_read_alt_backspace_deletes_word_without_wedging_parser() {
    let (_db, mut app) = chat_compose_app("alt-backspace-split").await;

    app.handle_input(b"one two");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("one") && frame.contains("two"),
        "expected compose render to show the initial text; frame={frame:?}"
    );

    // Simulate a terminal splitting Alt+Backspace across reads: lone ESC
    // first, then DEL on the next input chunk.
    app.handle_input(b"\x1b");
    app.handle_input(b"\x7f");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("│one │"),
        "expected split Alt+Backspace to leave the composer in the explicit intermediate state `one `; frame={frame:?}"
    );
    assert!(
        !frame.contains("two"),
        "expected split Alt+Backspace to delete the previous word; frame={frame:?}"
    );

    // Plain Backspace must still work after the word-delete chord. Insert a
    // fresh sentinel byte first so we can verify backspace removed it without
    // depending on whether delete-word keeps the separating space.
    app.handle_input(b"x\x7f!");
    let frame = render_plain(&mut app);
    assert!(
        frame.contains("one!") && !frame.contains("x"),
        "expected composer to keep accepting backspace and text after Alt+Backspace split from the intermediate `one ` state; frame={frame:?}"
    );
    assert!(
        !frame.contains("two"),
        "expected Alt+Backspace split read to delete the previous word; frame={frame:?}"
    );
}

#[tokio::test]
async fn chat_room_switch_ctrl_keys_wrap() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "chat-room-switch-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "chat-room-switch-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;

    app.handle_input(b"\x10");
    wait_for_render_contains(&mut app, "> discover").await;

    app.handle_input(b"\x0e");
    wait_for_render_contains(&mut app, "> general").await;
}

#[tokio::test]
async fn chat_reaction_leader_uses_digits_without_switching_screens() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "f-react-viewer").await;
    let author = create_test_user(&test_db.db, "f-react-author").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: author.id,
            body: "reaction target".to_string(),
        },
    )
    .await
    .expect("create message");

    let mut app = make_app(test_db.db.clone(), viewer.id, "f-react-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "reaction target").await;

    app.handle_input(b"j");
    app.handle_input(b"f1");

    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_until(
        || async {
            ChatMessageReaction::get_by_user_and_message(&client, message.id, viewer.id)
                .await
                .expect("load reaction")
                .is_some_and(|reaction| reaction.kind == 1)
        },
        "f leader reaction to persist",
    )
    .await;
}

#[tokio::test]
async fn help_command_renders_chat_feedback_without_persisting_message() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "help-notice-it").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, user.id)
        .await
        .expect("join general room");
    let mut app = make_app(test_db.db.clone(), user.id, "help-notice-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;

    app.handle_input(b"i/binds\r");
    wait_for_render_contains(&mut app, " Guide ").await;
    wait_for_render_contains(&mut app, " Chat ").await;
    wait_for_render_contains(&mut app, "/ignore [@user]").await;

    let messages = ChatMessage::list_recent(&client, general.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /binds to stay client-side");
}

#[tokio::test]
async fn members_command_shows_room_members_without_persisting_message() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "list-flow-viewer").await;
    let target = create_test_user(&test_db.db, "list-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer to general");

    let private_room = ChatRoom::create_private_room(&client, "side")
        .await
        .expect("create room");
    ChatRoomMember::join(&client, private_room.id, viewer.id)
        .await
        .expect("join viewer to side");
    ChatRoomMember::join(&client, private_room.id, target.id)
        .await
        .expect("join target to side");

    let mut app = make_app(test_db.db.clone(), viewer.id, "list-room-members-flow-it");

    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "> general").await;
    wait_for_render_contains(&mut app, " Private ").await;
    wait_for_render_contains(&mut app, " side").await;

    app.handle_input(b" ");
    wait_for_render_contains(&mut app, "[g] side").await;
    app.handle_input(b"g");
    wait_for_render_contains(&mut app, "> side").await;

    app.handle_input(b"i/members\r");
    wait_for_render_contains(&mut app, "#side Members").await;
    wait_for_render_contains(&mut app, "@list-flow-viewer").await;
    wait_for_render_contains(&mut app, "@list-flow-target").await;

    let messages = ChatMessage::list_recent(&client, private_room.id, 20)
        .await
        .expect("list recent messages");
    assert!(messages.is_empty(), "expected /members to stay client-side");
}

#[tokio::test]
async fn ignore_command_hides_messages_and_persists_across_refresh() {
    let test_db = new_test_db().await;
    let viewer = create_test_user(&test_db.db, "ignore-flow-viewer").await;
    let target = create_test_user(&test_db.db, "ignore-flow-target").await;
    let client = test_db.db.get().await.expect("db client");
    let general = ChatRoom::ensure_general(&client)
        .await
        .expect("ensure general room");
    ChatRoomMember::join(&client, general.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, general.id, target.id)
        .await
        .expect("join target");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: general.id,
            user_id: target.id,
            body: "message from ignored user".to_string(),
        },
    )
    .await
    .expect("create message");

    let (mut app, chat_service) =
        make_app_with_chat_service(test_db.db.clone(), viewer.id, "ignore-command-flow-it");
    app.handle_input(b"2");
    wait_for_render_contains(&mut app, " Rooms ").await;
    wait_for_render_contains(&mut app, "message from ignored user").await;

    app.handle_input(b"i");
    app.handle_input(b"/ignore ignore-flow-target\r");
    wait_for_render_contains(&mut app, "Ignored @ignore-flow-target").await;

    let ignored = User::ignored_user_ids(&client, viewer.id)
        .await
        .expect("load ignore list");
    assert_eq!(ignored, vec![target.id]);

    let post_ignore_body = "fresh message from ignored user";
    chat_service.send_message_task(
        target.id,
        general.id,
        Some("general".to_string()),
        post_ignore_body.to_string(),
        Uuid::now_v7(),
        false,
    );
    wait_until(
        || async {
            ChatMessage::list_recent(&client, general.id, 20)
                .await
                .expect("list recent messages")
                .iter()
                .any(|message| message.body == post_ignore_body)
        },
        "post-ignore message to persist",
    )
    .await;

    helpers::assert_render_not_contains_for(&mut app, post_ignore_body, Duration::from_millis(300))
        .await;

    let mut refreshed_app = make_app(test_db.db.clone(), viewer.id, "ignore-command-refresh-it");
    refreshed_app.handle_input(b"2");
    wait_for_render_contains(&mut refreshed_app, " Rooms ").await;
    helpers::assert_render_not_contains_for(
        &mut refreshed_app,
        post_ignore_body,
        Duration::from_millis(300),
    )
    .await;
}
