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
use super::state::{
    BOARD_MIN_INK, BoardFound, BoardInk, Crop, Found, HeraldCommand, Look, post_due_day, schedule,
    shown_day,
};
use super::svc::{
    CommandOutcome, DayWorked, HERALD_CLAIM_WINDOW, HERALD_MAX_ATTEMPTS, HeraldService, NextPost,
};
use crate::app::ai::svc::AiService;
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
    /// While set, late-web's picture routes answer what an older late-web
    /// answers for a route it lacks: a page, not a picture.
    web_answers_html: bool,
    /// Gemini: how many times the model was asked, and how many images
    /// the last call carried.
    gemini_calls: usize,
    gemini_images: usize,
    /// Gemini: the text the model answers.
    gemini_reply: String,
}

type Log = Arc<Mutex<Recorded>>;

/// late-web's PNG route, both networks' endpoints and the model on one
/// local port.
async fn fake_servers() -> (String, Log) {
    let log: Log = Arc::default();
    let app = Router::new()
        .route("/gallery/{key}/image.png", get(picture))
        .route("/gallery/pieces/{id}/image.png", get(picture))
        .route("/games/week/{last}/image.png", get(picture))
        .route("/paper/{edition}/image.png", get(picture))
        .route("/api/v2/media", post(media))
        .route("/api/v1/statuses", post(status))
        .route("/xrpc/com.atproto.server.createSession", post(session))
        .route("/xrpc/com.atproto.repo.uploadBlob", post(blob))
        .route("/xrpc/com.atproto.repo.putRecord", post(record))
        .route("/v1beta/models/{call}", post(gemini))
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
    let mut log = log.lock().expect("log");
    log.picture_paths.push(uri.to_string());
    match log.web_answers_html {
        true => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            Bytes::from_static(b"<html>late.sh</html>"),
        ),
        false => (
            [(header::CONTENT_TYPE, "image/png")],
            Bytes::from(png(640, 200, false)),
        ),
    }
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

async fn gemini(
    State(log): State<Log>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut log = log.lock().expect("log");
    log.gemini_calls += 1;
    log.gemini_images = body["contents"][0]["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter(|part| part.get("inlineData").is_some())
        .count();
    axum::Json(serde_json::json!({
        "candidates": [{"content": {"parts": [{"text": log.gemini_reply}]}}]
    }))
}

/// The model switched off, as in dev without a key.
fn ai_off() -> AiService {
    AiService::new(false, None)
}

fn service(db: late_core::db::Db, base: &str, ai: AiService) -> HeraldService {
    HeraldService::new(
        db,
        ai,
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
        &Snapshot::daily_key(shown),
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
/// The model is off, so every post goes out marked not reviewed: Bluesky
/// gets the picture with the mark, Mastodon a note in its place.
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

    let herald = service(db.clone(), &base, ai_off());
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
            vec!["/gallery/daily:2026-10-22/image.png?crop=0,0,80,5"],
            "fetched once for both networks, cropped to the drawn rows"
        );

        assert!(
            log.media_uploads.is_empty(),
            "an unseen picture stays off Mastodon"
        );
        let (auth, idempotency, status) = &log.statuses[0];
        assert_eq!(auth, "Bearer secret-token");
        assert_eq!(
            *idempotency,
            key.to_string(),
            "the row id is the idempotency key"
        );
        assert_eq!(status["media_ids"], serde_json::json!([]));
        assert_eq!(status["visibility"], "unlisted");
        assert_eq!(status["language"], "en");
        assert_eq!(status["sensitive"], true);
        assert_eq!(status["spoiler_text"], "cw: not reviewed");
        assert_eq!(
            status["status"],
            "Yesterday's Artboard, as the room left it on 22 October 2026, is behind a content warning, so this server gets a note and not the picture. It is on late.sh, and on Bluesky with a label.\n\nhttps://late.sh\n\n#tildeverse #terminal",
            "the note links the front page, never the gallery's card"
        );

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
        assert_eq!(
            record["text"],
            "cw: not reviewed\n\nThe Artboard yesterday: late.sh's shared ASCII canvas, as the room left it on 22 October 2026. Anyone in the clubhouse can pick up a brush.\n\nhttps://late.sh/gallery\n\n#ASCIIart #MastoArt #tildeverse #nsfw",
        );
        assert!(record.get("labels").is_none(), "no label for not reviewed");
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
        assert_eq!(
            log.picture_paths[1],
            "/gallery/daily:2026-10-23/image.png?crop=0,0,80,5"
        );
        let text = log.statuses[1].2["status"].as_str().expect("text");
        assert!(text.contains("23 October 2026"), "{text}");
        assert!(
            log.records[1].1["record"]["text"]
                .as_str()
                .expect("text")
                .starts_with("cw: not reviewed\n\nThe Artboard yesterday")
        );
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
        HeraldPost::is_finished(
            &client,
            day_three,
            Utc::now() - HERALD_CLAIM_WINDOW,
            HERALD_MAX_ATTEMPTS
        )
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

    let herald = service(db.clone(), &base, ai_off());
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

    let herald = service(db.clone(), &base, ai_off());
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

/// With the model on, its look writes the alt text and the mark, and is
/// stored with the pick: a warned board is a note on Mastodon and the
/// labelled picture on Bluesky, and a retry after one network failed sends
/// the same mark without asking the model again.
#[tokio::test]
async fn a_seen_day_carries_the_mark_and_a_retry_keeps_it() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    {
        let mut log = log.lock().expect("log");
        log.gemini_reply = serde_json::json!({
            "alt": "A cyborg stands on the left.",
            "warning": "nudity",
            "reason": "bare breasts on the cyborg",
        })
        .to_string();
        log.fail_bluesky = true;
    }
    let ai = AiService::at_endpoint(true, Some("test-key".to_string()), base.clone());
    let herald = service(db.clone(), &base, ai);
    let mut finished = None;

    assert!(herald.check(now, &mut finished).await);
    assert_eq!(row(&client, day).await.1, "failed");
    {
        let log = log.lock().expect("log");
        assert_eq!(log.gemini_calls, 1);
        assert_eq!(log.gemini_images, 5, "the board whole and its quarters");
        assert!(log.media_uploads.is_empty(), "no picture on Mastodon");
        let status = &log.statuses[0].2;
        assert_eq!(status["media_ids"], serde_json::json!([]));
        assert_eq!(status["sensitive"], true);
        assert_eq!(status["spoiler_text"], "nsfw: nudity");
        let text = status["status"].as_str().expect("text");
        assert!(text.starts_with("Yesterday's Artboard"), "{text}");
    }

    log.lock().expect("log").fail_bluesky = false;
    assert!(
        herald
            .check(now + Duration::minutes(5), &mut finished)
            .await
    );
    assert_eq!(row(&client, day).await.1, "posted");
    let log = log.lock().expect("log");
    assert_eq!(log.gemini_calls, 1, "the stored look, not a second call");
    let record = &log.records[0].1["record"];
    assert_eq!(
        record["labels"],
        serde_json::json!({
            "$type": "com.atproto.label.defs#selfLabels",
            "values": [{"val": "nudity"}],
        })
    );
    let text = record["text"].as_str().expect("text");
    assert!(text.starts_with("The Artboard yesterday"), "{text}");
    assert!(text.ends_with("#tildeverse #nsfw"), "{text}");
    assert_eq!(
        record["embed"]["images"][0]["alt"],
        "ASCII art: A cyborg stands on the left."
    );
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
    let herald = service(db.clone(), &base, ai_off());
    // Mid-morning: the checker's due day is yesterday, the admin's is today.
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 9, 0, 0).unwrap();
    let today = now.date_naive();

    assert_eq!(
        herald.run_command(HeraldCommand::On, now).await,
        CommandOutcome::SwitchedOn(NextPost::AtHourAfterSkipping(post_due_day(now))),
        "yesterday was due while off and stays unposted"
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
            found: Box::new(Found {
                announcement: None,
                scoreboard: None,
                piece: None,
                board: BoardFound {
                    key: "daily:2026-10-22".to_string(),
                    shown: shown_day(today),
                    ink: None,
                },
                paper: None,
            }),
            post: None,
            networks: vec![HeraldNetwork::Mastodon, HeraldNetwork::Bluesky],
        }
    );

    daily_board(&client, shown_day(today), BOARD_MIN_INK + 3).await;
    match herald.run_command(HeraldCommand::Preview, now).await {
        CommandOutcome::Preview { found, post, .. } => {
            assert_eq!(
                found.board.ink,
                Some(BoardInk {
                    cells: BOARD_MIN_INK + 3,
                    area: Some(Crop {
                        x: 0,
                        y: 0,
                        width: 80,
                        height: 5,
                    }),
                })
            );
            let (picture, look) = post.expect("the board is a post");
            assert_eq!(picture.subject(), "daily:2026-10-22");
            assert_eq!(look, Look::Unseen, "the model is off");
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

/// Switching on in the morning does not catch up the day the switch
/// slept through: that day is settled unposted and the first post is
/// today's, at 16:00. After the hour, an unposted today goes out at the
/// next check.
#[tokio::test]
async fn switching_on_waits_for_the_hour() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    let herald = service(db.clone(), &base, ai_off());
    let morning = Utc.with_ymd_and_hms(2026, 10, 23, 10, 0, 0).unwrap();
    let today = morning.date_naive();
    let slept = post_due_day(morning);
    daily_board(&client, shown_day(slept), BOARD_MIN_INK).await;
    daily_board(&client, shown_day(today), BOARD_MIN_INK).await;

    assert_eq!(
        herald.run_command(HeraldCommand::On, morning).await,
        CommandOutcome::SwitchedOn(NextPost::AtHourAfterSkipping(slept))
    );
    assert_eq!(row(&client, slept).await.1, "skipped");

    let mut finished = None;
    assert!(
        !herald
            .check(morning + Duration::minutes(5), &mut finished)
            .await
    );
    assert_eq!(finished, Some(slept));
    assert!(
        log.lock().expect("log").statuses.is_empty(),
        "nothing goes out before the hour"
    );

    let hour = Utc.with_ymd_and_hms(2026, 10, 23, 16, 5, 0).unwrap();
    assert!(herald.check(hour, &mut finished).await);
    assert_eq!(finished, Some(today));
    assert_eq!(row(&client, today).await.1, "posted");
    {
        let log = log.lock().expect("log");
        assert_eq!(log.statuses.len(), 1);
        assert_eq!(
            log.picture_paths,
            vec!["/gallery/daily:2026-10-22/image.png?crop=0,0,80,5"],
            "today's board, not the one the switch slept through"
        );
    }
    assert_eq!(
        herald.run_command(HeraldCommand::On, hour).await,
        CommandOutcome::SwitchedOn(NextPost::AtHour),
        "today has its row"
    );

    // The next evening with nothing posted yet: on means the next check.
    let evening = Utc.with_ymd_and_hms(2026, 10, 24, 17, 0, 0).unwrap();
    assert_eq!(
        herald.run_command(HeraldCommand::On, evening).await,
        CommandOutcome::SwitchedOn(NextPost::NextCheck)
    );
}

/// A piece hung through the gallery's own model, the way the hang flow
/// does.
async fn hang_piece(client: &late_core::db::PooledClient, user_id: Uuid, title: &str) -> Uuid {
    use late_core::models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams};
    let mut canvas = Canvas::with_size(12, 4);
    canvas.set(Pos { x: 0, y: 0 }, '#');
    match ArtboardPiece::hang(
        client,
        HangParams {
            user_id,
            title: title.to_string(),
            width: 12,
            height: 4,
            canvas: serde_json::to_value(&canvas).expect("canvas json"),
            provenance: serde_json::json!({"cells": []}),
            glyph_count: 40,
            own_share_percent: 100,
            content_hash: format!("hash-{title}"),
        },
    )
    .await
    .expect("hang")
    {
        HangOutcome::Hung(piece) => piece.id,
        other => panic!("hung, not {other:?}"),
    }
}

fn model_on(base: &str) -> AiService {
    AiService::at_endpoint(true, Some("test-key".to_string()), base.to_string())
}

/// The first day after today on which the schedule leads with the piece:
/// the wall shows a piece the day after it was hung at the earliest, and
/// the test hangs one now.
fn next_piece_day() -> NaiveDate {
    let mut day = Utc::now().date_naive() + Duration::days(1);
    while schedule(day)[0] != HeraldKind::Piece {
        day += Duration::days(1);
    }
    day
}

/// On an art day the piece over the door beats the board; the model saw
/// it clean, so both networks get the picture, credited and linking its
/// page. The wall moves on: the next day has no piece and the board goes
/// out.
#[tokio::test]
async fn a_clean_piece_goes_out_as_a_picture_and_only_once() {
    use late_core::models::artboard_piece::ArtboardPiece;

    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let day = next_piece_day();
    let now = day
        .and_hms_opt(16, 30, 0)
        .expect("half past four")
        .and_utc();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    daily_board(&client, shown_day(day + Duration::days(1)), BOARD_MIN_INK).await;
    let artist = late_core::test_utils::create_test_user(&db, "herald-artist").await;
    let piece = hang_piece(&client, artist.id, "harbour").await;
    log.lock().expect("log").gemini_reply = serde_json::json!({
        "alt": "A lighthouse on a cliff.",
        "warning": "none",
        "reason": "",
    })
    .to_string();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, &mut finished).await);
    let posted = HeraldPost::posted_on(&client, day)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(
        (posted.kind, posted.subject.as_str()),
        (HeraldKind::Piece, format!("piece:{piece}").as_str())
    );
    assert_eq!(
        ArtboardPiece::splash_for_day(&client, day)
            .await
            .expect("the wall")
            .map(|over_the_door| over_the_door.id),
        Some(piece),
        "the post is the piece over the door that day"
    );
    {
        let log = log.lock().expect("log");
        assert_eq!(
            log.picture_paths,
            vec![format!("/gallery/pieces/{piece}/image.png")]
        );
        assert!(
            log.media_uploads[0]
                .1
                .contains("ASCII art: A lighthouse on a cliff."),
            "{}",
            log.media_uploads[0].1
        );
        let status = &log.statuses[0].2;
        assert_eq!(status["media_ids"], serde_json::json!(["77"]));
        assert_eq!(status["sensitive"], false);
        assert!(status.get("spoiler_text").is_none(), "{status}");
        let text = status["status"].as_str().expect("text");
        assert_eq!(
            text,
            format!(
                "\"harbour\" by ~herald-artist, hung in the late.sh gallery: drawn cell by cell on the shared ASCII canvas, over SSH.\n\nhttps://late.sh/gallery/pieces/{piece}\n\n#ASCIIart #MastoArt #tildeverse #terminal"
            )
        );
        let record = &log.records[0].1["record"];
        assert_eq!(record["text"], text, "clean: the same words on both");
        assert!(record.get("labels").is_none());
        assert_eq!(
            record["embed"]["images"][0]["alt"],
            "ASCII art: A lighthouse on a cliff."
        );
    }

    assert!(herald.check(now + Duration::days(1), &mut finished).await);
    let next = HeraldPost::posted_on(&client, day + Duration::days(1))
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(
        (next.kind, next.subject.as_str()),
        (
            HeraldKind::Artboard,
            Snapshot::daily_key(shown_day(day + Duration::days(1))).as_str()
        ),
        "the wall has no piece the next day"
    );
}

/// The cards are drawn from late.sh's own words and never shown to the
/// model: the week's scoreboard on Sunday, the paper on a Monday with
/// nothing drawn. Both carry the picture on both networks, alt text from
/// the same words.
#[tokio::test]
async fn the_cards_go_out_drawn_without_the_model() {
    use late_core::models::le_word;
    use late_core::models::paper::{PaperSectionKind, PaperSectionRow};

    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let sunday = Utc.with_ymd_and_hms(2026, 10, 25, 16, 30, 0).unwrap();
    let player = late_core::test_utils::create_test_user(&db, "card-player").await;
    le_word::DailyWin::record_win(&client, player.id, shown_day(sunday.date_naive()), 4)
        .await
        .expect("a win in the week");
    let monday = sunday + Duration::days(1);
    assert!(
        PaperSectionRow::claim_printing(
            &client,
            monday.date_naive(),
            PaperSectionKind::Outside,
            Utc::now() - Duration::minutes(15),
            3,
        )
        .await
        .expect("claim")
    );
    PaperSectionRow::finish(
        &client,
        monday.date_naive(),
        PaperSectionKind::Outside,
        Some("The world turned."),
    )
    .await
    .expect("print");
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(sunday, &mut finished).await);
    assert!(herald.check(monday, &mut finished).await);
    let kinds = [sunday, monday].map(|at| at.date_naive());
    let mut posted = Vec::new();
    for day in kinds {
        let row = HeraldPost::posted_on(&client, day)
            .await
            .expect("read")
            .expect("posted");
        posted.push((row.kind, row.subject));
    }
    assert_eq!(
        posted,
        vec![
            (HeraldKind::Scoreboard, "week:2026-10-24".to_string()),
            (HeraldKind::Paper, "paper:2026-10-26".to_string()),
        ]
    );
    let log = log.lock().expect("log");
    assert_eq!(log.gemini_calls, 0, "a card is never shown to the model");
    assert_eq!(
        log.picture_paths,
        vec![
            "/games/week/2026-10-24/image.png",
            "/paper/2026-10-26/image.png"
        ]
    );
    assert!(
        log.media_uploads[0].1.contains("Le Word: 1 win"),
        "{}",
        log.media_uploads[0].1
    );
    assert!(
        log.media_uploads[1]
            .1
            .contains("Outside: The world turned."),
        "{}",
        log.media_uploads[1].1
    );
    for (_, _, status) in &log.statuses {
        assert_eq!(status["sensitive"], false);
    }
    let paper = log.statuses[1].2["status"].as_str().expect("text");
    assert!(
        paper.contains("https://late.sh/paper/2026-10-26\n"),
        "{paper}"
    );
    assert!(paper.ends_with("#smolweb"), "{paper}");
    assert!(
        log.statuses[0].2["status"]
            .as_str()
            .expect("text")
            .contains("\n\nhttps://late.sh\n\n#puzzles"),
        "the scoreboard links the front page"
    );
    for (_, put) in &log.records {
        assert!(put["record"].get("labels").is_none());
    }
}

/// A picture route that answers anything but a PNG fails the day before
/// any network is called: an older late-web's landing page never goes out
/// as the picture.
#[tokio::test]
async fn a_page_where_the_picture_should_be_fails_the_day() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    log.lock().expect("log").web_answers_html = true;

    let herald = service(db.clone(), &base, ai_off());
    let mut finished = None;
    assert!(herald.check(now, &mut finished).await);
    assert_eq!(row(&client, day).await.1, "failed");
    let log = log.lock().expect("log");
    assert_eq!(log.picture_paths.len(), 1);
    assert!(log.statuses.is_empty());
    assert!(log.records.is_empty());
}

/// The note of the day, as the operator posts it in `#announcements`.
const NOTE: &str = "# Server updates no longer kick you out\n\n- A new `late` CLI is out: `curl -fsSL https://cli.late.sh/install.sh | bash`\n- Idle 5 minutes when we ship, and your session moves to the new server by itself.";

/// `NOTE` posted in `#announcements` on the day `edition` covers, and the
/// edition swept (one public column printed), the way the paper's tests
/// set a day up.
async fn announce(db: &late_core::db::Db, client: &Client, edition: NaiveDate, author: &str) {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::paper::{PaperEdition, PaperSectionKind, PaperSectionRow};

    let room = ChatRoom::find_non_dm_by_slug(client, "announcements")
        .await
        .expect("find announcements")
        .expect("announcements room");
    let admin = late_core::test_utils::create_test_user(db, author).await;
    let message = ChatMessage::create(
        client,
        ChatMessageParams {
            room_id: room.id,
            user_id: admin.id,
            body: NOTE.to_string(),
        },
    )
    .await
    .expect("post the note");
    let (floor, _) = PaperEdition::window(edition);
    client
        .execute(
            "UPDATE chat_messages SET created = $2 WHERE id = $1",
            &[&message.id, &(floor + Duration::hours(1))],
        )
        .await
        .expect("backdate the note");
    assert!(
        PaperSectionRow::claim_printing(
            client,
            edition,
            PaperSectionKind::Outside,
            Utc::now() - Duration::minutes(15),
            3,
        )
        .await
        .expect("claim")
    );
    PaperSectionRow::finish(
        client,
        edition,
        PaperSectionKind::Outside,
        Some("The world turned."),
    )
    .await
    .expect("print");
}

/// The operator's word goes out the day its edition prints, ahead of the
/// schedule, as words alone: the model's brief on both networks (no media
/// on Mastodon, no embed on Bluesky, the edition's page linked), nothing
/// fetched from late-web and nothing shown to the model but the note; and
/// when the model's answer is unusable, the note as written, cut.
#[tokio::test]
async fn an_announcement_goes_out_as_words_the_day_its_edition_prints() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    // A Friday: an art day, and a board with ink waits and loses.
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 16, 30, 0).unwrap();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    announce(&db, &client, day, "herald-admin").await;
    log.lock().expect("log").gemini_reply = serde_json::json!({
        "text": "A new late CLI is out, and server updates no longer kick you out: idle five minutes when we ship and your session moves to the new server by itself.",
    })
    .to_string();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, &mut finished).await);
    let posted = HeraldPost::posted_on(&client, day)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(
        (posted.kind, posted.subject.as_str()),
        (HeraldKind::Announcement, "announcement:2026-10-23")
    );
    let words = "A new late CLI is out, and server updates no longer kick you out: idle five minutes when we ship and your session moves to the new server by itself.\n\nhttps://late.sh/paper/2026-10-23\n\n#tildeverse #terminal";
    {
        let log = log.lock().expect("log");
        assert!(log.picture_paths.is_empty(), "nothing to fetch");
        assert_eq!((log.gemini_calls, log.gemini_images), (1, 0));
        assert!(log.media_uploads.is_empty());
        assert!(log.blobs.is_empty());
        let status = &log.statuses[0].2;
        assert_eq!(status["status"], words);
        assert_eq!(status["media_ids"], serde_json::json!([]));
        assert_eq!(status["sensitive"], false);
        assert!(status.get("spoiler_text").is_none(), "{status}");
        let record = &log.records[0].1["record"];
        assert_eq!(record["text"], words);
        assert!(record.get("embed").is_none(), "{record}");
        assert!(record.get("labels").is_none(), "{record}");
        let facets = record["facets"].as_array().expect("facets");
        assert!(
            facets
                .iter()
                .any(|facet| { facet["features"][0]["uri"] == "https://late.sh/paper/2026-10-23" }),
            "{facets:?}"
        );
    }

    // The next day, a Saturday: another note, and the model answers junk.
    let next = day + Duration::days(1);
    announce(&db, &client, next, "herald-admin-again").await;
    log.lock().expect("log").gemini_reply = "not json".to_string();
    assert!(herald.check(now + Duration::days(1), &mut finished).await);
    let posted = HeraldPost::posted_on(&client, next)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Announcement);
    let stored = HeraldPost::stored_pick(&client, next)
        .await
        .expect("read")
        .expect("stored");
    assert_eq!(stored.picture["Announcement"]["brief"], "cut");
    let log = log.lock().expect("log");
    let status = log.statuses[1].2["status"].as_str().expect("text");
    assert!(
        status.starts_with(
            "Server updates no longer kick you out\nA new `late` CLI is out: `curl -fsSL https: //cli.late.sh/install.sh | bash`\n"
        ),
        "the note as written, dead: {status}"
    );
    assert!(
        status.ends_with("https://late.sh/paper/2026-10-24\n\n#tildeverse #terminal"),
        "{status}"
    );
}
