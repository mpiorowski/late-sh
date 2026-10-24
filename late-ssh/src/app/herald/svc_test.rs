use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::artboard::Snapshot;
use late_core::models::herald::{
    HeraldDay, HeraldDayStatus, HeraldKind, HeraldNetwork, HeraldPost, HeraldSwitch,
};
use tokio::net::TcpListener;
use tokio_postgres::Client;
use uuid::Uuid;

use super::bluesky::tid_for;
use super::bluesky_test::png;
use super::state::{BOARD_MIN_INK, BoardLook, HeraldCommand, board_key, shown_day};
use super::svc::{CommandOutcome, DayWorked, HERALD_MAX_ATTEMPTS, HeraldService};
use crate::config::{BlueskyConfig, HeraldConfig, HeraldVisibility, MastodonConfig};
use crate::test_helpers::new_test_db;

/// What the fake networks and the fake web were sent.
#[derive(Debug, Default)]
struct Recorded {
    picture_paths: Vec<String>,
    /// Mastodon: the authorization header and the multipart body as text.
    media_uploads: Vec<(String, String)>,
    /// Mastodon: the authorization header, the idempotency key, the body.
    statuses: Vec<(String, String, serde_json::Value)>,
    /// Bluesky: the session request bodies.
    sessions: Vec<serde_json::Value>,
    /// Bluesky: the authorization header and the blob's size.
    blobs: Vec<(String, usize)>,
    /// Bluesky: the authorization header and the putRecord body.
    records: Vec<(String, serde_json::Value)>,
    /// While set, the Mastodon media endpoint answers 500.
    fail_mastodon: bool,
    /// While set, the Bluesky blob endpoint answers 500.
    fail_bluesky: bool,
}

type Log = Arc<Mutex<Recorded>>;

/// late-web's PNG route and both networks' endpoints on one local port.
async fn fake_servers() -> (String, Log) {
    let log: Log = Arc::default();
    let app = Router::new()
        .route("/gallery/{key}/image.png", get(picture))
        .route("/api/v2/media", post(media))
        .route("/api/v1/statuses", post(status))
        .route("/xrpc/com.atproto.server.createSession", post(session))
        .route("/xrpc/com.atproto.repo.uploadBlob", post(blob))
        .route("/xrpc/com.atproto.repo.putRecord", post(record))
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
        Bytes::from(png(640, 200, false)),
    )
}

async fn media(State(log): State<Log>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut log = log.lock().expect("log");
    if log.fail_mastodon {
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

async fn session(
    State(log): State<Log>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    log.lock().expect("log").sessions.push(body);
    axum::Json(serde_json::json!({
        "accessJwt": "jwt",
        "refreshJwt": "refresh",
        "did": "did:plc:test",
        "handle": "late.test",
    }))
}

async fn blob(State(log): State<Log>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let mut log = log.lock().expect("log");
    if log.fail_bluesky {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": "down"})),
        );
    }
    log.blobs
        .push((header_text(&headers, "authorization"), body.len()));
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "blob": {
                "$type": "blob",
                "ref": {"$link": "bafyfake"},
                "mimeType": "image/png",
                "size": body.len(),
            }
        })),
    )
}

async fn record(
    State(log): State<Log>,
    headers: HeaderMap,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let uri = format!(
        "at://{}/{}/{}",
        body["repo"].as_str().unwrap_or_default(),
        body["collection"].as_str().unwrap_or_default(),
        body["rkey"].as_str().unwrap_or_default()
    );
    log.lock()
        .expect("log")
        .records
        .push((header_text(&headers, "authorization"), body));
    axum::Json(serde_json::json!({"uri": uri, "cid": "bafycid"}))
}

fn service(db: late_core::db::Db, base: &str) -> HeraldService {
    HeraldService::new(
        db,
        HeraldConfig {
            mastodon: Some(MastodonConfig {
                instance: base.to_string(),
                token: "secret-token".to_string(),
                visibility: HeraldVisibility::Unlisted,
            }),
            bluesky: Some(BlueskyConfig {
                pds: base.to_string(),
                identifier: "late.test".to_string(),
                app_password: "app-pass".to_string(),
            }),
            web_internal_url: format!("{base}/"),
        },
        "https://late.sh/".to_string(),
    )
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

async fn row(client: &Client, day: NaiveDate) -> (Uuid, String, i32) {
    let row = client
        .query_one(
            "SELECT id, status, attempts FROM herald_posts WHERE day = $1",
            &[&day],
        )
        .await
        .expect("row");
    (row.get("id"), row.get("status"), row.get("attempts"))
}

/// Day one posts yesterday's board to both networks, day two the next
/// board, day three has a bare board and rests; a day is settled once.
#[tokio::test]
async fn the_herald_posts_the_board_and_rests_on_a_bare_one() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");

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

    // Day one: the board of the 22nd, on both networks.
    assert!(herald.check(now, &mut finished).await);
    assert_eq!(finished, Some(day));
    let (key, status, attempts) = row(&client, day).await;
    assert_eq!((status.as_str(), attempts), ("posted", 1));
    let posted = HeraldPost::posted_on(&client, day)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Artboard);
    assert_eq!(posted.subject, "daily:2026-10-22");
    let rkey = tid_for(key);
    assert_eq!(posted.deliveries.len(), 2);
    assert_eq!(posted.deliveries[0].network, HeraldNetwork::Bluesky);
    assert_eq!(
        posted.deliveries[0].url,
        format!("https://bsky.app/profile/late.test/post/{rkey}")
    );
    assert_eq!(posted.deliveries[1].network, HeraldNetwork::Mastodon);
    assert_eq!(posted.deliveries[1].url, "https://instance.test/@late/1");
    {
        let log = log.lock().expect("log");
        assert_eq!(
            log.picture_paths,
            vec!["/gallery/daily:2026-10-22/image.png"],
            "fetched once for both networks"
        );

        let (auth, body) = &log.media_uploads[0];
        assert_eq!(auth, "Bearer secret-token");
        assert!(body.contains("name=\"file\""), "{body}");
        assert!(body.contains("image/png"), "{body}");
        assert!(body.contains("name=\"description\""), "{body}");
        assert!(body.contains("22 October 2026"), "{body}");
        let (auth, idempotency, status) = &log.statuses[0];
        assert_eq!(auth, "Bearer secret-token");
        assert_eq!(
            *idempotency,
            key.to_string(),
            "the row id is the idempotency key"
        );
        assert_eq!(status["media_ids"], serde_json::json!(["77"]));
        assert_eq!(status["visibility"], "unlisted");
        assert_eq!(status["language"], "en");
        let text = status["status"].as_str().expect("text");
        assert!(text.contains("The Artboard yesterday"), "{text}");
        assert!(
            text.contains("https://late.sh/gallery?key=daily:2026-10-22"),
            "{text}"
        );
        assert!(text.contains("#MastoArt"), "{text}");

        assert_eq!(log.sessions[0]["identifier"], "late.test");
        assert_eq!(log.sessions[0]["password"], "app-pass");
        assert_eq!(log.blobs[0].0, "Bearer jwt");
        assert_eq!(log.blobs[0].1, png(640, 200, false).len());
        let (auth, put) = &log.records[0];
        assert_eq!(auth, "Bearer jwt");
        assert_eq!(put["repo"], "did:plc:test");
        assert_eq!(put["collection"], "app.bsky.feed.post");
        assert_eq!(put["rkey"], rkey, "the record key comes from the row id");
        let record = &put["record"];
        assert_eq!(record["text"], text, "the same words on both networks");
        assert_eq!(record["langs"], serde_json::json!(["en"]));
        assert!(
            record["createdAt"]
                .as_str()
                .unwrap()
                .starts_with("2026-10-23T16:30:00")
        );
        assert!(!record["facets"].as_array().unwrap().is_empty());
        let image = &record["embed"]["images"][0];
        assert!(image["alt"].as_str().unwrap().contains("22 October 2026"));
        assert_eq!(image["image"]["ref"]["$link"], "bafyfake");
        assert_eq!(
            image["aspectRatio"],
            serde_json::json!({"width": 640, "height": 200})
        );
    }

    // The same day again, memoized or not, is over.
    assert!(!herald.check(now + Duration::hours(1), &mut finished).await);
    let mut fresh = None;
    assert!(!herald.check(now + Duration::hours(2), &mut fresh).await);
    assert_eq!(fresh, Some(day), "a settled day is memoized on sight");
    assert_eq!(log.lock().expect("log").statuses.len(), 1);

    // Day two: the next day's board.
    let day_two = day + Duration::days(1);
    assert!(herald.check(now + Duration::days(1), &mut finished).await);
    let posted = HeraldPost::posted_on(&client, day_two)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Artboard);
    assert_eq!(posted.subject, "daily:2026-10-23");
    assert_eq!(posted.deliveries.len(), 2);
    {
        let log = log.lock().expect("log");
        assert_eq!(log.picture_paths[1], "/gallery/daily:2026-10-23/image.png");
        assert!(log.media_uploads[1].1.contains("23 October 2026"));
        let text = log.statuses[1].2["status"].as_str().expect("text");
        assert!(
            text.contains("https://late.sh/gallery?key=daily:2026-10-23"),
            "{text}"
        );
        assert_eq!(log.records[1].1["record"]["text"], text);
    }

    // Day three: the board is a smudge.
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
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 2);
    assert_eq!(log.records.len(), 2);
}

/// With the row off nothing is claimed and nothing is sent, however the
/// process is configured.
#[tokio::test]
async fn the_switch_off_posts_nothing() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    daily_board(&client, shown_day(now.date_naive()), BOARD_MIN_INK).await;

    let herald = service(db.clone(), &base);
    let mut finished = None;
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
    assert!(log.records.is_empty());
}

/// One network down leaves the day failed with the other's delivery
/// kept; the next check sends the same picture to the missing network
/// only, under the same key.
#[tokio::test]
async fn a_failed_network_is_retried_alone_under_the_same_key() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    log.lock().expect("log").fail_bluesky = true;

    let herald = service(db.clone(), &base);
    let mut finished = None;
    assert!(
        herald.check(now, &mut finished).await,
        "the day was claimed and tried"
    );
    assert_eq!(finished, None, "a failed day is not finished");
    let (key, status, attempts) = row(&client, day).await;
    assert_eq!((status.as_str(), attempts), ("failed", 1));
    let landed = HeraldPost::deliveries(&client, day).await.expect("read");
    assert_eq!(landed.len(), 1);
    assert_eq!(landed[0].network, HeraldNetwork::Mastodon);
    assert_eq!(
        HeraldPost::posted_on(&client, day).await.expect("read"),
        None
    );
    {
        let log = log.lock().expect("log");
        assert_eq!(log.statuses.len(), 1);
        assert_eq!(log.blobs.len(), 0);
        assert_eq!(log.records.len(), 0);
    }

    log.lock().expect("log").fail_bluesky = false;
    assert!(
        herald
            .check(now + Duration::minutes(5), &mut finished)
            .await
    );
    assert_eq!(finished, Some(day));
    let (retry_key, status, attempts) = row(&client, day).await;
    assert_eq!((status.as_str(), attempts), ("posted", 2));
    assert_eq!(retry_key, key, "the retry carries the first claim's key");
    let posted = HeraldPost::posted_on(&client, day)
        .await
        .expect("read")
        .expect("posted on the retry");
    assert_eq!(posted.subject, "daily:2026-10-22", "the stored pick");
    assert_eq!(posted.deliveries.len(), 2);
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 1, "mastodon was not sent again");
    assert_eq!(log.records.len(), 1);
    assert_eq!(log.records[0].1["rkey"], tid_for(key));
}

/// An admin's `/herald`: the row flips, status and preview read without
/// posting, and `post` works today before 16:00 with the row off; the
/// same day again is settled, and the checker then leaves it alone.
#[tokio::test]
async fn the_commands_flip_read_and_post_today_by_hand() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    let herald = service(db.clone(), &base);
    // Mid-morning: the checker's due day is yesterday, the admin's is today.
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 9, 0, 0).unwrap();
    let today = now.date_naive();

    assert_eq!(
        herald.run_command(HeraldCommand::On, now).await,
        CommandOutcome::SwitchedOn
    );
    assert!(HeraldSwitch::is_on(&client).await.expect("read"));
    assert_eq!(
        herald.run_command(HeraldCommand::Off, now).await,
        CommandOutcome::SwitchedOff
    );
    assert!(!HeraldSwitch::is_on(&client).await.expect("read"));

    assert_eq!(
        herald.run_command(HeraldCommand::Status, now).await,
        CommandOutcome::Status {
            on: false,
            day: today,
            today: None,
        }
    );
    assert_eq!(
        herald.run_command(HeraldCommand::Preview, now).await,
        CommandOutcome::Preview {
            day: today,
            look: BoardLook {
                key: "daily:2026-10-22".to_string(),
                shown: shown_day(today),
                ink: None,
            },
            networks: vec![HeraldNetwork::Mastodon, HeraldNetwork::Bluesky],
        }
    );

    daily_board(&client, shown_day(today), BOARD_MIN_INK + 3).await;
    match herald.run_command(HeraldCommand::Preview, now).await {
        CommandOutcome::Preview { look, .. } => {
            assert_eq!(look.ink, Some(BOARD_MIN_INK + 3));
            assert!(look.picture().is_some());
        }
        other => panic!("a preview, not {other:?}"),
    }
    assert!(
        log.lock().expect("log").statuses.is_empty(),
        "a preview sends nothing"
    );

    // Posting by hand, with the row off and before the post time.
    let worked = herald.run_command(HeraldCommand::Post, now).await;
    let CommandOutcome::Worked(DayWorked::Posted(posted)) = worked else {
        panic!("posted, not {worked:?}");
    };
    assert_eq!(posted.subject, "daily:2026-10-22");
    assert_eq!(posted.deliveries.len(), 2);
    assert_eq!(
        herald.run_command(HeraldCommand::Status, now).await,
        CommandOutcome::Status {
            on: false,
            day: today,
            today: Some(HeraldDay {
                status: HeraldDayStatus::Posted,
                attempts: 1,
                deliveries: posted.deliveries.clone(),
            }),
        }
    );
    assert_eq!(
        herald.run_command(HeraldCommand::Post, now).await,
        CommandOutcome::Worked(DayWorked::Finished),
        "a day goes out once"
    );

    // The checker at 16:00 with the row on finds today settled.
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let mut finished = None;
    assert!(!herald.check(now + Duration::hours(8), &mut finished).await);
    assert_eq!(finished, Some(today));
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 1);
    assert_eq!(log.records.len(), 1);
}
