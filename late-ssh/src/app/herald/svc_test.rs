use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::app_flag::AppFlag;
use late_core::models::artboard::Snapshot;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::models::herald::{HeraldKind, HeraldPost};
use late_core::models::work_profile::{WorkProfile, WorkProfileParams, WorkStatus, WorkType};
use late_core::test_utils::create_test_user;
use tokio::net::TcpListener;
use tokio_postgres::Client;
use uuid::Uuid;

use super::state::{BOARD_MIN_INK, board_key, shown_day};
use super::svc::{HERALD_MAX_ATTEMPTS, HeraldService};
use crate::config::{HeraldConfig, HeraldVisibility};
use crate::test_helpers::new_test_db;

const PNG_BYTES: &[u8] = b"\x89PNG fake picture";

/// What the fake instance and the fake web were sent.
#[derive(Debug, Default)]
struct Recorded {
    picture_paths: Vec<String>,
    /// Authorization header and the multipart body as text.
    media_uploads: Vec<(String, String)>,
    /// Authorization header, the idempotency key, the JSON body.
    statuses: Vec<(String, String, serde_json::Value)>,
    /// While set, the media endpoint answers 500.
    fail_media: bool,
}

type Log = Arc<Mutex<Recorded>>;

/// late-web's two PNG routes and the instance's two endpoints on one
/// local port.
async fn fake_servers() -> (String, Log) {
    let log: Log = Arc::default();
    let app = Router::new()
        .route("/gallery/{key}/image.png", get(picture))
        .route("/profiles/{slug}/poster.png", get(picture))
        .route("/api/v2/media", post(media))
        .route("/api/v1/statuses", post(status))
        .with_state(log.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (format!("http://{addr}"), log)
}

fn header_text(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

async fn picture(State(log): State<Log>, uri: Uri) -> impl IntoResponse {
    log.lock()
        .expect("log")
        .picture_paths
        .push(uri.path().to_string());
    (
        [(header::CONTENT_TYPE, "image/png")],
        Bytes::from_static(PNG_BYTES),
    )
}

async fn media(State(log): State<Log>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut log = log.lock().expect("log");
    if log.fail_media {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": "down"})),
        );
    }
    log.media_uploads.push((
        header_text(&headers, "authorization"),
        String::from_utf8_lossy(&body).into_owned(),
    ));
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({"id": "77", "type": "image"})),
    )
}

async fn status(
    State(log): State<Log>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut log = log.lock().expect("log");
    log.statuses.push((
        header_text(&headers, "authorization"),
        header_text(&headers, "idempotency-key"),
        body,
    ));
    let n = log.statuses.len();
    axum::Json(serde_json::json!({
        "id": n.to_string(),
        "url": format!("https://instance.test/@late/{n}"),
    }))
}

fn service(db: late_core::db::Db, base: &str) -> HeraldService {
    HeraldService::new(
        db,
        HeraldConfig {
            enabled: true,
            instance: format!("{base}/"),
            token: Some("secret-token".to_string()),
            web_internal_url: base.to_string(),
            visibility: HeraldVisibility::Unlisted,
        },
        "https://late.sh/".to_string(),
    )
}

/// A standing runner with a poster and a public profile under `slug`.
async fn runner_with_poster(
    client: &Client,
    db: &late_core::db::Db,
    name: &str,
    slug: &str,
) -> Uuid {
    let user = create_test_user(db, name).await;
    DeadchannelRunner::ensure_for_user(client, user.id, &serde_json::json!({}))
        .await
        .expect("runner");
    DeadchannelRunner::store_poster(client, user.id, &serde_json::json!({"cells": []}))
        .await
        .expect("poster");
    WorkProfile::create_by_user_id(
        client,
        user.id,
        WorkProfileParams {
            user_id: user.id,
            slug: slug.into(),
            headline: "Terminal artist".into(),
            status: WorkStatus::Open,
            work_type: WorkType::Any,
            location: String::new(),
            contact: String::new(),
            links: vec![],
            skills: vec![],
            skills_tags: vec![],
            summary: String::new(),
        },
    )
    .await
    .expect("profile");
    user.id
}

/// The board archived for `shown`, with `ink` drawn cells.
async fn daily_board(client: &Client, shown: NaiveDate, ink: usize) {
    let mut canvas = Canvas::with_size(80, 24);
    for index in 0..ink {
        canvas.set(
            Pos {
                x: index % 80,
                y: index / 80,
            },
            '#',
        );
    }
    Snapshot::upsert(
        client,
        &board_key(shown),
        serde_json::to_value(&canvas).expect("canvas json"),
        serde_json::json!({"cells": []}),
    )
    .await
    .expect("snapshot");
}

async fn row_key(client: &Client, day: NaiveDate) -> String {
    let row = client
        .query_one(
            "SELECT id::text AS id, status, attempts FROM herald_posts WHERE day = $1",
            &[&day],
        )
        .await
        .expect("row");
    row.get("id")
}

/// Day one posts the waiting poster, day two the board, day three has a
/// bare board and nothing waiting and rests; a day is settled once.
#[tokio::test]
async fn the_herald_posts_a_poster_then_the_board_then_rests() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    AppFlag::HeraldEnabled
        .set(&client, true)
        .await
        .expect("switch on");

    let painter = runner_with_poster(&client, &db, "h_painter", "w_heraldsvc001").await;
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    daily_board(
        &client,
        shown_day(day + Duration::days(1)),
        BOARD_MIN_INK + 5,
    )
    .await;
    daily_board(
        &client,
        shown_day(day + Duration::days(2)),
        BOARD_MIN_INK - 1,
    )
    .await;

    let herald = service(db.clone(), &base);
    let mut finished = None;

    // Day one: the poster.
    assert!(herald.check(now, &mut finished).await);
    assert_eq!(finished, Some(day));
    let posted = HeraldPost::posted_on(&client, day)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Poster);
    assert_eq!(posted.subject, painter.to_string());
    assert_eq!(posted.status_url, "https://instance.test/@late/1");
    {
        let log = log.lock().expect("log");
        assert_eq!(
            log.picture_paths,
            vec!["/profiles/w_heraldsvc001/poster.png"]
        );
        let (auth, body) = &log.media_uploads[0];
        assert_eq!(auth, "Bearer secret-token");
        assert!(body.contains("name=\"file\""), "{body}");
        assert!(body.contains("PNG fake picture"), "{body}");
        assert!(body.contains("name=\"description\""), "{body}");
        assert!(body.contains("painted by ~h_painter"), "{body}");
        let (auth, key, status) = &log.statuses[0];
        assert_eq!(auth, "Bearer secret-token");
        assert_eq!(
            *key,
            row_key(&client, day).await,
            "the row id is the idempotency key"
        );
        assert_eq!(status["media_ids"], serde_json::json!(["77"]));
        assert_eq!(status["visibility"], "unlisted");
        assert_eq!(status["language"], "en");
        let text = status["status"].as_str().expect("text");
        assert!(text.contains("~h_painter painted this poster"), "{text}");
        assert!(
            text.contains("https://late.sh/profiles/w_heraldsvc001"),
            "{text}"
        );
    }

    // The same day again, memoized or not, is over.
    assert!(!herald.check(now + Duration::hours(1), &mut finished).await);
    let mut fresh = None;
    assert!(!herald.check(now + Duration::hours(2), &mut fresh).await);
    assert_eq!(fresh, Some(day), "a settled day is memoized on sight");
    assert_eq!(log.lock().expect("log").statuses.len(), 1);

    // Day two: the board, since the last post was a poster.
    let day_two = day + Duration::days(1);
    assert!(herald.check(now + Duration::days(1), &mut finished).await);
    let posted = HeraldPost::posted_on(&client, day_two)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Artboard);
    assert_eq!(posted.subject, "daily:2026-10-23");
    {
        let log = log.lock().expect("log");
        assert_eq!(log.picture_paths[1], "/gallery/daily:2026-10-23/image.png");
        assert!(log.media_uploads[1].1.contains("23 October 2026"));
        let text = log.statuses[1].2["status"].as_str().expect("text");
        assert!(
            text.contains("https://late.sh/gallery?key=daily:2026-10-23"),
            "{text}"
        );
        assert!(text.contains("#MastoArt"), "{text}");
    }

    // Day three: the one poster went out, the board is a smudge.
    let day_three = day + Duration::days(2);
    assert!(herald.check(now + Duration::days(2), &mut finished).await);
    assert_eq!(
        HeraldPost::posted_on(&client, day_three)
            .await
            .expect("read"),
        None
    );
    assert!(
        HeraldPost::is_finished(&client, day_three, HERALD_MAX_ATTEMPTS)
            .await
            .expect("read"),
        "a skipped day is settled"
    );
    assert_eq!(log.lock().expect("log").statuses.len(), 2);
}

/// With the row off nothing is claimed and nothing is sent, however the
/// process is configured.
#[tokio::test]
async fn the_switch_off_posts_nothing() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    runner_with_poster(&client, &db, "h_quiet", "w_heraldsvc002").await;

    let herald = service(db.clone(), &base);
    let mut finished = None;
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    assert!(!herald.check(now, &mut finished).await);
    assert_eq!(finished, None);
    let rows: i64 = client
        .query_one("SELECT count(*) FROM herald_posts", &[])
        .await
        .expect("count")
        .get(0);
    assert_eq!(rows, 0);
    let log = log.lock().expect("log");
    assert!(log.picture_paths.is_empty());
    assert!(log.statuses.is_empty());
}

/// An instance that fails the upload leaves the day failed with its key;
/// the next check claims it again and the retry posts under the same key.
#[tokio::test]
async fn a_failed_upload_is_retried_under_the_same_key() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    AppFlag::HeraldEnabled
        .set(&client, true)
        .await
        .expect("switch on");
    runner_with_poster(&client, &db, "h_retry", "w_heraldsvc003").await;
    log.lock().expect("log").fail_media = true;

    let herald = service(db.clone(), &base);
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    let day = now.date_naive();
    let mut finished = None;
    assert!(
        herald.check(now, &mut finished).await,
        "the day was claimed and tried"
    );
    assert_eq!(finished, None, "a failed day is not finished");
    let row = client
        .query_one(
            "SELECT status, attempts FROM herald_posts WHERE day = $1",
            &[&day],
        )
        .await
        .expect("row");
    assert_eq!(row.get::<_, String>("status"), "failed");
    assert_eq!(row.get::<_, i32>("attempts"), 1);
    let key = row_key(&client, day).await;
    assert!(
        log.lock().expect("log").statuses.is_empty(),
        "no status without media"
    );

    log.lock().expect("log").fail_media = false;
    assert!(
        herald
            .check(now + Duration::minutes(5), &mut finished)
            .await
    );
    assert_eq!(finished, Some(day));
    let posted = HeraldPost::posted_on(&client, day)
        .await
        .expect("read")
        .expect("posted on the retry");
    assert_eq!(posted.kind, HeraldKind::Poster);
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 1);
    assert_eq!(
        log.statuses[0].1, key,
        "the retry carries the first claim's key"
    );
}
