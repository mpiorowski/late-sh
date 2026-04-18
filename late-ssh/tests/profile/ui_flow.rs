//! End-to-end profile screen flows: input → state → save → render.
//! Exercises the real `App`, `ProfileService`, and DB round-trips; no test
//! accessors or mocks.

use late_core::models::profile::Profile;
use tokio::time::{Duration, Instant, sleep};

use super::helpers::{make_app, new_test_db, render_plain, wait_for_render_contains, wait_until};
use late_core::test_utils::create_test_user;

/// The profile screen renders one checkbox row per notification kind. Assert
/// that `needle` (e.g. "Direct messages") appears on a line that also contains
/// `marker` (e.g. "[x]").
fn row_has_marker(plain: &str, needle: &str, marker: &str) -> bool {
    plain
        .lines()
        .any(|line| line.contains(needle) && line.contains(marker))
}

#[tokio::test]
async fn profile_notification_checkbox_toggle_persists_across_reconnect() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "notify-toggle-it").await;

    // First connection: toggle "Direct messages" on.
    {
        let mut app = make_app(test_db.db.clone(), user.id, "notify-toggle-flow-it");

        app.handle_input(b"4");
        // Wait for the profile snapshot to land — the username row is populated
        // from the async `find_profile` task. Without this, `profile.id` is
        // still `Uuid::nil()` when we press Space, and `save_profile` silently
        // updates zero rows.
        wait_for_render_contains(&mut app, "notify-toggle-it").await;
        wait_for_render_contains(&mut app, "Direct messages").await;

        // All kinds start off → three unchecked boxes.
        let initial = render_plain(&mut app);
        assert!(
            row_has_marker(&initial, "Direct messages", "[ ]"),
            "Direct messages row should start unchecked:\n{initial}"
        );
        assert!(
            row_has_marker(&initial, "@mentions", "[ ]"),
            "@mentions row should start unchecked:\n{initial}"
        );
        assert!(
            row_has_marker(&initial, "Game events", "[ ]"),
            "Game events row should start unchecked:\n{initial}"
        );

        // settings_row defaults to 0 (Theme). Row 1 is the background-color
        // toggle, so it takes two Down presses to reach the first notification
        // row ("Direct messages"), then toggle it.
        app.handle_input(b"jj");
        app.handle_input(b" ");

        // Wait for the toggled frame. The in-memory flip is immediate but
        // rendering happens on the next tick.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut toggled = false;
        while Instant::now() < deadline {
            let plain = render_plain(&mut app);
            if row_has_marker(&plain, "Direct messages", "[x]") {
                toggled = true;
                break;
            }
            sleep(Duration::from_millis(30)).await;
        }
        assert!(
            toggled,
            "Direct messages row should flip to [x] after Space"
        );

        // The other two rows must remain unchecked — only the selected one toggled.
        let after = render_plain(&mut app);
        assert!(
            row_has_marker(&after, "@mentions", "[ ]"),
            "@mentions should remain unchecked:\n{after}"
        );
        assert!(
            row_has_marker(&after, "Game events", "[ ]"),
            "Game events should remain unchecked:\n{after}"
        );

        // The bell setting should be independent of the other settings.
        assert!(
            row_has_marker(&after, "Bell", "[ ]"),
            "Bell should remain unchecked:\n{after}"
        );

        // Each Space press spawns an independent save task; wait for the DMs
        // save to land before toggling Bell so the two writes serialize in DB
        // order (otherwise save-2 can race ahead and get overwritten by save-1,
        // clobbering notify_bell back to false).
        let db = test_db.db.clone();
        wait_until(
            || {
                let db = db.clone();
                async move {
                    let client = db.get().await.expect("db client");
                    let profile = Profile::load(&client, user.id).await.expect("profile");
                    profile.notify_kinds == vec!["dms".to_string()]
                }
            },
            "profile.notify_kinds to persist as [dms] before toggling Bell",
        )
        .await;

        app.handle_input(b"jjj");
        app.handle_input(b" ");

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut toggled = false;
        while Instant::now() < deadline {
            let plain = render_plain(&mut app);
            if row_has_marker(&plain, "Bell", "[x]") {
                toggled = true;
                break;
            }
            sleep(Duration::from_millis(30)).await;
        }
        assert!(toggled, "Bell row should flip to [x] after Space");

        // Wait for the save task to reach the DB before tearing down the app.
        let db = test_db.db.clone();
        wait_until(
            || {
                let db = db.clone();
                async move {
                    let client = db.get().await.expect("db client");
                    let profile = Profile::load(&client, user.id).await.expect("profile");
                    profile.notify_kinds == vec!["dms".to_string()] && profile.notify_bell
                }
            },
            "profile.notify_kinds to persist as [dms] and notify_bell as true",
        )
        .await;
    }

    // Reconnect as the same user and confirm the profile snapshot restores
    // the checkbox. The background refresh task reads the freshly-updated row
    // on startup; poll render output until it reflects the persisted state.
    let mut reconnected = make_app(
        test_db.db.clone(),
        user.id,
        "notify-toggle-flow-reconnect-it",
    );
    reconnected.handle_input(b"4");
    wait_for_render_contains(&mut reconnected, "Direct messages").await;

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut restored = false;
    while Instant::now() < deadline {
        let plain = render_plain(&mut reconnected);
        if row_has_marker(&plain, "Direct messages", "[x]") && row_has_marker(&plain, "Bell", "[x]")
        {
            restored = true;
            break;
        }
        sleep(Duration::from_millis(30)).await;
    }
    assert!(
        restored,
        "Direct messages and Bell rows should remain [x] after reconnect"
    );
}

#[tokio::test]
async fn profile_username_edit_trims_whitespace_and_persists() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "trim-orig").await;
    let mut app = make_app(test_db.db.clone(), user.id, "trim-flow-it");

    // Profile screen, wait until initial snapshot arrives.
    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "trim-orig").await;

    // Enter username edit mode.
    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Username (Enter save, Esc cancel)").await;

    // The composer pre-fills with the current name; Ctrl+U (0x15) clears it.
    // Then type a whitespace-padded name and press Enter.
    app.handle_input(b"\x15");
    app.handle_input(b"  alice  \r");

    // Render should show the trimmed name, and the edit box title returns to
    // the read-only "(i edit)" hint — proving submit_username ran.
    wait_for_render_contains(&mut app, "Username (i edit)").await;

    let rendered = render_plain(&mut app);
    assert!(
        rendered.contains("alice"),
        "rendered profile should show 'alice':\n{rendered}"
    );
    assert!(
        !rendered.contains("  alice  "),
        "rendered profile must not contain padded username:\n{rendered}"
    );

    // DB round-trip: the async save should land as the trimmed string.
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let profile = Profile::load(&client, user.id).await.expect("profile");
                profile.username == "alice"
            }
        },
        "profile.username to persist as trimmed value",
    )
    .await;
}

#[tokio::test]
async fn profile_username_edit_replaces_spaces_and_invalid_chars() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "normalize-orig").await;
    let mut app = make_app(test_db.db.clone(), user.id, "normalize-flow-it");

    app.handle_input(b"4");
    wait_for_render_contains(&mut app, "normalize-orig").await;

    app.handle_input(b"i");
    wait_for_render_contains(&mut app, "Username (Enter save, Esc cancel)").await;

    app.handle_input(b"\x15");
    app.handle_input(b"late night!!!\r");

    wait_for_render_contains(&mut app, "Username (i edit)").await;
    wait_for_render_contains(&mut app, "late_night").await;

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let profile = Profile::load(&client, user.id).await.expect("profile");
                profile.username == "late_night"
            }
        },
        "profile.username to persist as normalized value",
    )
    .await;
}
