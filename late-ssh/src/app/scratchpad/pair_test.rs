use std::sync::{Arc, Mutex};
use std::time::Instant;

use uuid::Uuid;

use super::*;
use crate::state::ActiveUser;

fn active_users_with(entries: &[(Uuid, &str)]) -> ActiveUsers {
    let mut users = std::collections::HashMap::new();
    for (id, username) in entries {
        users.insert(
            *id,
            ActiveUser {
                username: username.to_string(),
                fingerprint: None,
                audio_source: late_core::models::user::AudioSource::Icecast,
                sessions: Vec::new(),
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
    }
    Arc::new(Mutex::new(users))
}

#[test]
fn finds_by_case_insensitive_username() {
    let alice = Uuid::from_u128(1);
    let active = active_users_with(&[(alice, "Alice")]);
    assert_eq!(
        find_active_user_by_username(Some(&active), "alice"),
        Some(alice)
    );
    assert_eq!(
        find_active_user_by_username(Some(&active), "ALICE"),
        Some(alice)
    );
}

#[test]
fn returns_none_for_unknown_username() {
    let active = active_users_with(&[(Uuid::from_u128(1), "Alice")]);
    assert_eq!(find_active_user_by_username(Some(&active), "bob"), None);
}

#[test]
fn returns_none_without_an_active_users_map() {
    assert_eq!(find_active_user_by_username(None, "alice"), None);
}

/// Two real apps sharing one registry and one active-users map, the way two
/// SSH sessions on one replica see each other. Returns them in the order the
/// usernames were given.
async fn two_sessions(
    test_db: &late_core::test_utils::TestDb,
    names: [&str; 2],
) -> [crate::app::state::App; 2] {
    use crate::test_helpers::{SessionWorld, make_app_in_world, wait_for_render_contains};

    use late_core::models::{chat_room::ChatRoom, chat_room_member::ChatRoomMember};

    let registry = SharedScratchpadRegistry::new();
    let client = test_db.db.get().await.expect("db client");
    let lounge = ChatRoom::ensure_lounge(&client)
        .await
        .expect("ensure lounge");
    let mut users = Vec::new();
    for name in names {
        let user = late_core::test_utils::create_test_user(&test_db.db, name).await;
        ChatRoomMember::join(&client, lounge.id, user.id)
            .await
            .expect("join lounge");
        users.push(user.id);
    }
    drop(client);
    let active = active_users_with(&[(users[0], names[0]), (users[1], names[1])]);

    let mut apps = Vec::new();
    for (idx, name) in names.iter().enumerate() {
        let world = SessionWorld {
            username: Some((*name).to_string()),
            active_users: Some(active.clone()),
            scratchpad_registry: Some(registry.clone()),
            ..SessionWorld::default()
        };
        let mut app = make_app_in_world(
            test_db.db.clone(),
            users[idx],
            &format!("sess-{name}"),
            world,
        );
        // The composer only takes commands once the room snapshot has landed,
        // so settle both sessions before any test types into them.
        wait_for_render_contains(&mut app, "lounge").await;
        apps.push(app);
    }
    let [b, a] = [
        apps.pop().expect("second app"),
        apps.pop().expect("first app"),
    ];
    [a, b]
}

/// Type a chat command into the composer and submit it. The trailing space is
/// load-bearing: a command ending in `@name` leaves the mention autocomplete
/// open, and Enter there confirms the completion instead of submitting.
async fn run_command(app: &mut crate::app::state::App, command: &str) {
    use crate::test_helpers::wait_for_render_contains;

    app.handle_input(b"i");
    wait_for_render_contains(app, "Compose (Enter send").await;
    app.handle_input(format!("{command} ").as_bytes());
    assert!(
        !app.chat.is_autocomplete_active(),
        "the trailing space should have closed the autocomplete, or Enter \
         will confirm a completion instead of submitting {command:?}"
    );
    app.handle_input(b"\r");
}

#[tokio::test]
async fn pairing_lifecycle_preserves_invite_input_sync_and_departure() {
    use crate::test_helpers::{render_plain, wait_for_render_contains};

    let test_db = crate::test_helpers::new_test_db().await;
    let [mut alice, mut bob] = two_sessions(&test_db, ["alice-pair", "bob-pair"]).await;

    run_command(&mut alice, "/pair @bob-pair").await;
    let frame = render_plain(&mut alice);
    assert!(
        frame.contains("Asked @bob-pair to pair"),
        "asking should report waiting, not open the editor; frame={frame:?}"
    );
    assert!(
        !frame.contains("paired with"),
        "one ask is not a pairing; frame={frame:?}"
    );

    // Regression test for the invite prompt this replaced: a `/pair` from
    // someone else must not change the target's screen or eat their keys.
    let frame = render_plain(&mut bob);
    assert!(
        frame.contains("alice-pair wants to pair"),
        "the target is told, by banner only; frame={frame:?}"
    );
    assert!(
        !frame.contains("paired with"),
        "the target is not dragged into the editor; frame={frame:?}"
    );

    // The banner owns nothing: bob's very next keystroke still opens the
    // composer instead of being swallowed by a prompt.
    bob.handle_input(b"i");
    let frame = render_plain(&mut bob);
    assert!(
        frame.contains("Compose (Enter send"),
        "input still reaches the screen underneath; frame={frame:?}"
    );

    // Bob's composer is already open from the input-ownership assertion, so
    // finish the reciprocal command in place instead of resetting the app.
    bob.handle_input(b"/pair @alice-pair ");
    assert!(
        !bob.chat.is_autocomplete_active(),
        "the trailing space should close pair-command autocomplete"
    );
    bob.handle_input(b"\r");
    let frame = render_plain(&mut bob);
    assert!(
        frame.contains("paired with @alice-pair"),
        "the second ask completes the handshake; frame={frame:?}"
    );

    // Alice never accepted anything: her session picks the pairing up on its
    // next tick, because she already asked for it.
    wait_for_render_contains(&mut alice, "paired with @bob-pair").await;

    alice.handle_input(b"fn main() {}");

    wait_for_render_contains(&mut bob, "fn main() {}").await;

    // A lone Esc is held by the parser until it can rule out a longer escape
    // sequence, so wait for Home rather than rendering one frame.
    alice.handle_input(b"\x1b");
    wait_for_render_contains(&mut alice, " Home ").await;

    let frame = render_plain(&mut alice);
    assert!(
        !frame.contains("paired with"),
        "the leaver is back on Home; frame={frame:?}"
    );
    wait_for_render_contains(&mut bob, "alice-pair left the pairing").await;
}

#[tokio::test]
async fn question_mark_types_into_the_scratchpad_instead_of_opening_the_guide() {
    // Regression test: the daily board / house table let '?' escape to the
    // global help guide, since neither is a place where you'd ever want to
    // type a literal '?'. The scratchpad's dedicated-screen dispatch copied
    // that shape verbatim, so '?' silently opened the guide instead of being
    // inserted -- wrong for a free-typing text editor.
    use crate::test_helpers::{render_plain, wait_for_render_contains};

    let test_db = crate::test_helpers::new_test_db().await;
    let [mut alice, mut bob] = two_sessions(&test_db, ["alice-qmark", "bob-qmark"]).await;
    run_command(&mut alice, "/pair @bob-qmark").await;
    run_command(&mut bob, "/pair @alice-qmark").await;
    wait_for_render_contains(&mut alice, "paired with @bob-qmark").await;

    alice.handle_input(b"what does this do?");

    wait_for_render_contains(&mut bob, "what does this do?").await;
    let frame = render_plain(&mut alice);
    assert!(
        !frame.contains("Install `late` / Listen Anywhere"),
        "the global guide must not have opened; frame={frame:?}"
    );
}
