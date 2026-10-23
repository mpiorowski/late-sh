use crate::{
    app::{
        audio::{booth::state::BoothFocus, youtube::YoutubeVideo},
        state::App,
    },
    test_helpers::{make_app, wait_until},
};
use late_core::{
    db::Db,
    models::media_favorite::MediaFavorite,
    test_utils::{create_test_user, test_db},
};
use uuid::Uuid;

fn video(video_id: &str, title: &str) -> YoutubeVideo {
    YoutubeVideo {
        video_id: video_id.to_string(),
        title: Some(title.to_string()),
        channel: Some("Music channel".to_string()),
        duration_ms: Some(60_000),
        is_stream: false,
    }
}

#[tokio::test]
async fn ctrl_y_copies_the_link_of_the_track_the_focus_points_at() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "booth-copy").await;
    let mut app = make_app(test.db.clone(), user.id, "booth-copy");
    let service = app.audio.service();
    service
        .submit_validated_video(user.id, video("aaaaaaaaaaa", "Playing"))
        .await
        .expect("start YouTube track");
    service
        .submit_validated_video(user.id, video("bbbbbbbbbbb", "Up next"))
        .await
        .expect("queue YouTube track");
    let playing = "https://www.youtube.com/watch?v=aaaaaaaaaaa";
    let up_next = "https://www.youtube.com/watch?v=bbbbbbbbbbb";
    app.booth_modal_state.open(true);

    // Submit focus copies the playing track and leaves the URL field alone.
    app.handle_input(b"i\x19");
    assert_eq!(app.booth_modal_state.submit_input(), "i");
    assert_eq!(app.pending_clipboard.as_deref(), Some(playing));

    // The lists copy their selected row; History keeps the playing track on
    // top, and the `/` filter does not swallow the copy.
    for (focus, keys, expected) in [
        ("queue", &b"\t"[..], up_next),
        ("history", &b"\t"[..], playing),
        ("history filter", &b"/"[..], playing),
    ] {
        app.handle_input(keys);
        app.pending_clipboard = None;
        app.handle_input(b"\x19");
        assert_eq!(app.pending_clipboard.as_deref(), Some(expected), "{focus}");
        assert!(app.booth_modal_state.is_open(), "{focus}");
    }
    assert!(app.booth_modal_state.history_filter_active());
}

#[tokio::test]
async fn ctrl_y_on_an_empty_queue_preserves_the_clipboard() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "booth-copy-empty").await;
    let mut app = make_app(test.db.clone(), user.id, "booth-copy-empty");
    app.pending_clipboard = Some("Earlier copy".to_string());
    app.booth_modal_state.open(false);

    app.handle_input(b"\x19");

    assert_eq!(app.pending_clipboard.as_deref(), Some("Earlier copy"));
    assert_eq!(
        app.banner.as_ref().map(|banner| banner.message.as_str()),
        Some("No track to copy")
    );
}

fn selected_history_track(app: &App) -> Option<String> {
    let snapshot = app.audio.queue_snapshot();
    app.booth_modal_state
        .selected_history_row(app.audio.booth_favorites(), &snapshot.history)
        .map(|row| row.video_id().to_string())
}

/// Wait until the favorites table holds `expected`, newest first, and the
/// session shows those very rows, ids included.
async fn settle_favorites(app: &mut App, db: &Db, user_id: Uuid, expected: &[&str]) {
    wait_until(
        || {
            app.audio.tick();
            let shown: Vec<(Uuid, String)> = app
                .audio
                .booth_favorites()
                .iter()
                .map(|favorite| (favorite.id, favorite.video_id.clone()))
                .collect();
            let expected: Vec<String> = expected.iter().map(|id| id.to_string()).collect();
            let db = db.clone();
            async move {
                let client = db.get().await.expect("db client");
                let kept: Vec<(Uuid, String)> = MediaFavorite::list_for_user(&client, user_id)
                    .await
                    .expect("favorites")
                    .into_iter()
                    .map(|row| (row.id, row.external_id))
                    .collect();
                let kept_video_ids: Vec<String> =
                    kept.iter().map(|(_, video_id)| video_id.clone()).collect();
                kept == shown && kept_video_ids == expected
            }
        },
        "favorites settled",
    )
    .await;
}

/// `f` keeps the row that came next under the cursor, the way News's `s`
/// does, so the cursor never lands on a track the listener did not pick:
/// before, it kept its index while the track jumped to the top, and sat on
/// the row above. The row moves at once, and the table keeps the rows the
/// session showed, under the same ids.
#[tokio::test]
async fn f_keeps_the_next_row_under_the_cursor() {
    let test = test_db().await;
    let user = create_test_user(&test.db, "booth-favorite").await;
    let mut app = make_app(test.db.clone(), user.id, "booth-favorite");
    let service = app.audio.service().clone();
    for (video_id, title) in [
        ("aaaaaaaaaaa", "First"),
        ("bbbbbbbbbbb", "Second"),
        ("ccccccccccc", "Third"),
    ] {
        service
            .submit_validated_video(user.id, video(video_id, title))
            .await
            .expect("queue YouTube track");
    }
    service.force_skip().await.expect("play the second track");
    service.force_skip().await.expect("play the third track");
    app.booth_modal_state.open(true);
    app.booth_modal_state.set_focus(BoothFocus::History);

    // History is [ccc, bbb, aaa]; keep bbb.
    app.handle_input(b"\x1b[B");
    app.handle_input(b"f");
    settle_favorites(&mut app, &test.db, user.id, &["bbbbbbbbbbb"]).await;
    assert_eq!(selected_history_track(&app).as_deref(), Some("aaaaaaaaaaa"));

    // [bbb*, ccc, aaa]: nothing comes after aaa, so the cursor follows it
    // to the top, before any write lands.
    app.handle_input(b"f");
    let shown: Vec<&str> = app
        .audio
        .booth_favorites()
        .iter()
        .map(|favorite| favorite.video_id.as_str())
        .collect();
    assert_eq!(shown, vec!["aaaaaaaaaaa", "bbbbbbbbbbb"]);
    assert_eq!(selected_history_track(&app).as_deref(), Some("aaaaaaaaaaa"));

    // Letting aaa go keeps bbb, the row after it, under the cursor.
    app.handle_input(b"f");
    assert_eq!(selected_history_track(&app).as_deref(), Some("bbbbbbbbbbb"));
    settle_favorites(&mut app, &test.db, user.id, &["bbbbbbbbbbb"]).await;
    assert_eq!(selected_history_track(&app).as_deref(), Some("bbbbbbbbbbb"));
}
