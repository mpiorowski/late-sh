use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use dartboard_core::{Canvas, Pos};
use late_core::models::artboard::Snapshot;
use late_core::models::herald::{
    HeraldDay, HeraldDayStatus, HeraldKind, HeraldNetwork, HeraldPost, HeraldSlot, HeraldSwitch,
};
use late_core::models::showcase::{Showcase, ShowcaseParams};
use tokio::net::TcpListener;
use tokio_postgres::Client;
use uuid::Uuid;

use super::bluesky::tid_for;
use super::bluesky_test::png;
use super::state::{
    ArtDay, ArtFound, BOARD_MIN_INK, BoardFound, BoardInk, Crop, Found, HeraldCommand, art_day,
    post_due_day, shown_day,
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
    /// Gemini: the text of the last call's prompt.
    gemini_prompt: String,
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
        .route("/static/og-image.png", get(picture))
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
    log.gemini_prompt = body["contents"][0]["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .filter_map(|part| part["text"].as_str())
        .collect();
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

async fn row(client: &Client, day: NaiveDate, slot: HeraldSlot) -> (Uuid, String, i32) {
    let row = client
        .query_one(
            "SELECT id, status, attempts FROM herald_posts WHERE day = $1 AND slot = $2",
            &[&day, &slot.as_str()],
        )
        .await
        .expect("row");
    (row.get("id"), row.get("status"), row.get("attempts"))
}

/// What the model answers about a picture it finds clean.
fn clean_look() -> String {
    serde_json::json!({
        "alt": "A lighthouse on a cliff.",
        "warning": "none",
        "reason": "",
    })
    .to_string()
}

/// Half past the art slot on the Monday of 26 October 2026, the board's day.
fn monday_art() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 20, 30, 0).unwrap()
}

/// Half past the words slot, the same Monday.
fn monday_words() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 15, 30, 0).unwrap()
}

/// Monday's art is yesterday's board, seen clean, as a picture on both
/// networks under the row's own key; a settled slot is settled, the words slot
/// is untouched by it, and the next Monday's bare board rests.
#[tokio::test]
async fn the_monday_board_goes_out_and_a_bare_one_rests() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = monday_art();
    let day = now.date_naive();
    let next_monday = day + Duration::days(7);
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    daily_board(&client, shown_day(next_monday), BOARD_MIN_INK - 1).await;
    log.lock().expect("log").gemini_reply = clean_look();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, HeraldSlot::Art, &mut finished).await);
    assert_eq!(finished, Some(day));
    let (key, status, attempts) = row(&client, day, HeraldSlot::Art).await;
    assert_eq!((status.as_str(), attempts), ("posted", 1));
    let posted = HeraldPost::posted_on(&client, day, HeraldSlot::Art)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Artboard);
    assert_eq!(posted.subject, "daily:2026-10-25");
    let rkey = tid_for(key);
    assert_eq!(posted.deliveries.len(), 2);
    assert_eq!(posted.deliveries[0].network, HeraldNetwork::Bluesky);
    assert_eq!(
        posted.deliveries[0].url,
        format!("https://bsky.app/profile/late.test/post/{rkey}")
    );
    assert_eq!(posted.deliveries[1].network, HeraldNetwork::Mastodon);
    assert_eq!(posted.deliveries[1].url, "https://instance.test/@late/1");
    let words = "Yesterday's Artboard on late.sh, 25 October 2026. A shared ASCII canvas, anyone logged in can draw on it.\n\nhttps://late.sh/gallery\n\n#ASCIIart #MastoArt #tildeverse #terminal";
    {
        let log = log.lock().expect("log");
        assert_eq!(
            log.picture_paths,
            vec!["/gallery/daily:2026-10-25/image.png?crop=0,0,80,5"],
            "fetched once for both networks, cropped to the drawn rows"
        );
        assert_eq!((log.gemini_calls, log.gemini_images), (1, 5));
        assert_eq!(log.media_uploads[0].0, "Bearer secret-token");
        assert!(
            log.media_uploads[0]
                .1
                .contains("ASCII art: A lighthouse on a cliff."),
            "{}",
            log.media_uploads[0].1
        );
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
        assert_eq!(status["status"], words);

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
        assert_eq!(record["text"], words, "the same words on both");
        assert!(record.get("labels").is_none(), "{record}");
        assert_eq!(record["langs"], serde_json::json!(["en"]));
        assert!(
            record["createdAt"]
                .as_str()
                .unwrap()
                .starts_with("2026-10-26T20:30:00")
        );
        assert!(!record["facets"].as_array().unwrap().is_empty());
        let image = &record["embed"]["images"][0];
        assert_eq!(image["alt"], "ASCII art: A lighthouse on a cliff.");
        assert_eq!(image["image"]["ref"]["$link"], "bafyfake");
        assert_eq!(
            image["aspectRatio"],
            serde_json::json!({"width": 640, "height": 200})
        );
    }

    // The same slot again, memoized or not, is over; words have no row.
    assert!(
        !herald
            .check(now + Duration::hours(1), HeraldSlot::Art, &mut finished)
            .await
    );
    let mut fresh = None;
    assert!(
        !herald
            .check(now + Duration::hours(2), HeraldSlot::Art, &mut fresh)
            .await
    );
    assert_eq!(fresh, Some(day), "a settled slot is memoized on sight");
    assert_eq!(log.lock().expect("log").statuses.len(), 1);
    assert_eq!(
        HeraldPost::day(&client, day, HeraldSlot::Words)
            .await
            .expect("read"),
        None
    );

    // The next Monday: the board is a smudge, and nothing goes out.
    assert!(
        herald
            .check(now + Duration::days(7), HeraldSlot::Art, &mut finished)
            .await
    );
    assert_eq!(
        row(&client, next_monday, HeraldSlot::Art).await.1,
        "skipped"
    );
    assert!(
        HeraldPost::is_finished(
            &client,
            next_monday,
            HeraldSlot::Art,
            Utc::now() - HERALD_CLAIM_WINDOW,
            HERALD_MAX_ATTEMPTS
        )
        .await
        .expect("read"),
        "a skipped slot is settled"
    );
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 1);
    assert_eq!(log.records.len(), 1);
}

/// With the row off nothing is claimed and nothing is sent, however the
/// process is configured.
#[tokio::test]
async fn the_switch_off_posts_nothing() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    let now = monday_art();
    daily_board(&client, shown_day(now.date_naive()), BOARD_MIN_INK).await;

    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;
    assert!(!herald.check(now, HeraldSlot::Art, &mut finished).await);
    assert!(!herald.check(now, HeraldSlot::Words, &mut finished).await);
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

/// One network down leaves the post failed with the other's delivery kept;
/// the next check sends the same picture to the missing network only, under
/// the same key, without asking the model again.
#[tokio::test]
async fn a_failed_network_is_retried_alone_under_the_same_key() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = monday_art();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    {
        let mut log = log.lock().expect("log");
        log.gemini_reply = clean_look();
        log.fail_bluesky = true;
    }

    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;
    assert!(
        herald.check(now, HeraldSlot::Art, &mut finished).await,
        "the slot was claimed and tried"
    );
    assert_eq!(finished, None, "a failed post is not finished");
    let (key, status, attempts) = row(&client, day, HeraldSlot::Art).await;
    assert_eq!((status.as_str(), attempts), ("failed", 1));
    let landed = HeraldPost::deliveries(&client, day, HeraldSlot::Art)
        .await
        .expect("read");
    assert_eq!(landed.len(), 1);
    assert_eq!(landed[0].network, HeraldNetwork::Mastodon);
    assert_eq!(
        HeraldPost::posted_on(&client, day, HeraldSlot::Art)
            .await
            .expect("read"),
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
            .check(now + Duration::minutes(5), HeraldSlot::Art, &mut finished)
            .await
    );
    assert_eq!(finished, Some(day));
    let (retry_key, status, attempts) = row(&client, day, HeraldSlot::Art).await;
    assert_eq!((status.as_str(), attempts), ("posted", 2));
    assert_eq!(retry_key, key, "the retry carries the first claim's key");
    let posted = HeraldPost::posted_on(&client, day, HeraldSlot::Art)
        .await
        .expect("read")
        .expect("posted on the retry");
    assert_eq!(posted.subject, "daily:2026-10-25", "the stored pick");
    assert_eq!(posted.deliveries.len(), 2);
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 1, "mastodon was not sent again");
    assert_eq!(log.records.len(), 1);
    assert_eq!(log.records[0].1["rkey"], tid_for(key));
    assert_eq!(log.gemini_calls, 1, "the stored look, not a second call");
    assert_eq!(
        log.records[0].1["record"]["embed"]["images"][0]["alt"],
        "ASCII art: A lighthouse on a cliff.",
        "the stored look's alt text"
    );
}

/// A flagged board stays off every network: the slot rests for the day, nothing
/// is stored and nothing is sent, no note and no label.
#[tokio::test]
async fn a_flagged_board_stays_off_every_network() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = monday_art();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    log.lock().expect("log").gemini_reply = serde_json::json!({
        "alt": "A cyborg stands on the left.",
        "warning": "nudity",
        "reason": "bare breasts on the cyborg",
    })
    .to_string();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, HeraldSlot::Art, &mut finished).await);
    assert_eq!(finished, Some(day), "settled, not retried");
    assert_eq!(row(&client, day, HeraldSlot::Art).await.1, "skipped");
    assert_eq!(
        HeraldPost::stored_pick(&client, day, HeraldSlot::Art)
            .await
            .expect("read"),
        None
    );
    let log = log.lock().expect("log");
    assert_eq!((log.gemini_calls, log.gemini_images), (1, 5));
    assert!(log.media_uploads.is_empty());
    assert!(log.statuses.is_empty());
    assert!(log.blobs.is_empty());
    assert!(log.records.is_empty());
}

/// A look that fails is no verdict: nothing goes out, the row is `failed` and
/// the next check asks again, so one bad answer does not lose the day's picture.
#[tokio::test]
async fn a_failed_look_is_retried_with_nothing_sent() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = monday_art();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    log.lock().expect("log").gemini_reply = "not json".to_string();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, HeraldSlot::Art, &mut finished).await);
    let (_, status, attempts) = row(&client, day, HeraldSlot::Art).await;
    assert_eq!((status.as_str(), attempts), ("failed", 1));
    assert_eq!(finished, None, "not settled");
    assert_eq!(
        HeraldPost::stored_pick(&client, day, HeraldSlot::Art)
            .await
            .expect("read"),
        None
    );
    {
        let log = log.lock().expect("log");
        assert!(log.statuses.is_empty());
        assert!(log.records.is_empty());
    }

    log.lock().expect("log").gemini_reply = clean_look();
    assert!(herald.check(now, HeraldSlot::Art, &mut finished).await);
    let (_, status, attempts) = row(&client, day, HeraldSlot::Art).await;
    assert_eq!((status.as_str(), attempts), ("posted", 2));
    assert_eq!(finished, Some(day));
    let log = log.lock().expect("log");
    assert_eq!(log.gemini_calls, 2);
    assert_eq!(log.statuses.len(), 1);
    assert_eq!(log.records.len(), 1);
}

/// An admin's `/herald`: the row flips, status and preview read without
/// posting, and `post` works today's slot before its hour with the row off; the
/// same slot again is settled, and the checker then leaves it alone.
#[tokio::test]
async fn the_commands_flip_read_and_post_a_slot_by_hand() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    log.lock().expect("log").gemini_reply = clean_look();
    let herald = service(db.clone(), &base, model_on(&base));
    // Mid-morning: the checker's due day is yesterday, the admin's is today.
    let now = Utc.with_ymd_and_hms(2026, 10, 26, 9, 0, 0).unwrap();
    let today = now.date_naive();
    let yesterday = today - Duration::days(1);

    assert_eq!(
        herald.run_command(HeraldCommand::On, now).await,
        CommandOutcome::SwitchedOn {
            words: NextPost::AtHourAfterSkipping(yesterday),
            art: NextPost::AtHourAfterSkipping(yesterday),
        },
        "yesterday's two posts were due while off and stay unposted"
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
            words: None,
            art: None,
        }
    );
    let art = HeraldCommand::Preview(HeraldSlot::Art);
    assert_eq!(
        herald.run_command(art, now).await,
        CommandOutcome::Preview {
            day: today,
            slot: HeraldSlot::Art,
            found: Box::new(Found::Art(ArtFound::Board(BoardFound {
                key: "daily:2026-10-25".to_string(),
                shown: shown_day(today),
                ink: None,
            }))),
            post: None,
            networks: vec![HeraldNetwork::Mastodon, HeraldNetwork::Bluesky],
        }
    );

    daily_board(&client, shown_day(today), BOARD_MIN_INK + 3).await;
    match herald.run_command(art, now).await {
        CommandOutcome::Preview { found, post, .. } => {
            let Found::Art(ArtFound::Board(board)) = *found else {
                panic!("Monday's art is the board, not {found:?}");
            };
            assert_eq!(
                board.ink,
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
            assert_eq!(picture.subject(), "daily:2026-10-25");
            assert!(look.goes_out(), "{look:?}");
        }
        other => panic!("a preview, not {other:?}"),
    }
    assert!(
        log.lock().expect("log").statuses.is_empty(),
        "a preview sends nothing"
    );

    // Posting by hand, with the row off and before the post time.
    let post_art = HeraldCommand::Post(HeraldSlot::Art);
    let worked = herald.run_command(post_art, now).await;
    let CommandOutcome::Worked(DayWorked::Posted(posted)) = worked else {
        panic!("posted, not {worked:?}");
    };
    assert_eq!(posted.subject, "daily:2026-10-25");
    assert_eq!(posted.deliveries.len(), 2);
    assert_eq!(
        herald.run_command(HeraldCommand::Status, now).await,
        CommandOutcome::Status {
            on: false,
            day: today,
            words: None,
            art: Some(HeraldDay {
                status: HeraldDayStatus::Posted,
                attempts: 1,
                deliveries: posted.deliveries.clone(),
            }),
        }
    );
    assert_eq!(
        herald.run_command(post_art, now).await,
        CommandOutcome::Worked(DayWorked::Finished),
        "a slot goes out once"
    );

    // The checker at the art hour with the row on finds it settled.
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let mut finished = None;
    assert!(
        !herald
            .check(now + Duration::hours(11), HeraldSlot::Art, &mut finished)
            .await
    );
    assert_eq!(finished, Some(today));
    let log = log.lock().expect("log");
    assert_eq!(log.statuses.len(), 1);
    assert_eq!(log.records.len(), 1);
}

/// Switching on in the morning does not catch up what the switch slept through:
/// those posts are settled unposted and the first is today's, at its hour. After
/// the hour, an unposted slot goes out at the next check. Each slot answers for
/// itself.
#[tokio::test]
async fn switching_on_waits_for_each_slots_hour() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    log.lock().expect("log").gemini_reply = clean_look();
    let herald = service(db.clone(), &base, model_on(&base));
    let morning = Utc.with_ymd_and_hms(2026, 10, 26, 10, 0, 0).unwrap();
    let today = morning.date_naive();
    let slept = post_due_day(morning, HeraldSlot::Art);
    daily_board(&client, shown_day(slept), BOARD_MIN_INK).await;
    daily_board(&client, shown_day(today), BOARD_MIN_INK).await;

    assert_eq!(
        herald.run_command(HeraldCommand::On, morning).await,
        CommandOutcome::SwitchedOn {
            words: NextPost::AtHourAfterSkipping(slept),
            art: NextPost::AtHourAfterSkipping(slept),
        }
    );
    assert_eq!(row(&client, slept, HeraldSlot::Art).await.1, "skipped");
    assert_eq!(row(&client, slept, HeraldSlot::Words).await.1, "skipped");

    let mut finished = None;
    assert!(
        !herald
            .check(
                morning + Duration::minutes(5),
                HeraldSlot::Art,
                &mut finished
            )
            .await
    );
    assert_eq!(finished, Some(slept));
    assert!(
        log.lock().expect("log").statuses.is_empty(),
        "nothing goes out before the hour"
    );

    let hour = Utc.with_ymd_and_hms(2026, 10, 26, 20, 5, 0).unwrap();
    assert!(herald.check(hour, HeraldSlot::Art, &mut finished).await);
    assert_eq!(finished, Some(today));
    assert_eq!(row(&client, today, HeraldSlot::Art).await.1, "posted");
    {
        let log = log.lock().expect("log");
        assert_eq!(log.statuses.len(), 1);
        assert_eq!(
            log.picture_paths,
            vec!["/gallery/daily:2026-10-25/image.png?crop=0,0,80,5"],
            "today's board, not the one the switch slept through"
        );
    }
    assert_eq!(
        herald.run_command(HeraldCommand::On, hour).await,
        CommandOutcome::SwitchedOn {
            words: NextPost::NextCheck,
            art: NextPost::AtHour,
        },
        "art has its row today, words do not"
    );

    // The next evening with nothing posted yet: on means the next check.
    let evening = Utc.with_ymd_and_hms(2026, 10, 27, 21, 0, 0).unwrap();
    assert_eq!(
        herald.run_command(HeraldCommand::On, evening).await,
        CommandOutcome::SwitchedOn {
            words: NextPost::NextCheck,
            art: NextPost::NextCheck,
        }
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

/// The first day after today that shows a piece and is followed by another:
/// the wall shows a piece the day after it was hung at the earliest, and the
/// tests hang theirs now.
fn next_piece_days() -> NaiveDate {
    let mut day = Utc::now().date_naive() + Duration::days(1);
    while art_day(day) != ArtDay::Piece || art_day(day + Duration::days(1)) != ArtDay::Piece {
        day += Duration::days(1);
    }
    day
}

/// Half past the art slot on `day`.
fn art_hour(day: NaiveDate) -> DateTime<Utc> {
    day.and_hms_opt(20, 30, 0)
        .expect("half past eight")
        .and_utc()
}

/// On a piece day the piece over the door goes out, seen clean, credited and
/// linking its page. The wall moves on: the next day has no piece, and the board
/// waiting there is no fallback, so that day's art rests.
#[tokio::test]
async fn a_clean_piece_goes_out_and_an_empty_wall_rests() {
    use late_core::models::artboard_piece::ArtboardPiece;

    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let day = next_piece_days();
    let next = day + Duration::days(1);
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    daily_board(&client, shown_day(next), BOARD_MIN_INK).await;
    let artist = late_core::test_utils::create_test_user(&db, "herald-artist").await;
    let piece = hang_piece(&client, artist.id, "harbour").await;
    log.lock().expect("log").gemini_reply = clean_look();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(
        herald
            .check(art_hour(day), HeraldSlot::Art, &mut finished)
            .await
    );
    let posted = HeraldPost::posted_on(&client, day, HeraldSlot::Art)
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
        let text = status["status"].as_str().expect("text");
        assert_eq!(
            text,
            format!(
                "\"harbour\" by ~herald-artist, from the late.sh gallery. ASCII art drawn on the shared canvas over SSH.\n\nhttps://late.sh/gallery/pieces/{piece}\n\n#ASCIIart #MastoArt #tildeverse #terminal"
            )
        );
        let record = &log.records[0].1["record"];
        assert_eq!(record["text"], text, "the same words on both");
        assert_eq!(
            record["embed"]["images"][0]["alt"],
            "ASCII art: A lighthouse on a cliff."
        );
    }

    assert!(
        herald
            .check(art_hour(next), HeraldSlot::Art, &mut finished)
            .await
    );
    assert_eq!(row(&client, next, HeraldSlot::Art).await.1, "skipped");
    let log = log.lock().expect("log");
    assert_eq!(log.picture_paths.len(), 1, "the board was never fetched");
    assert_eq!(log.statuses.len(), 1);
}

/// A piece the look flags rests and does not reach for the board; so does a
/// piece nobody could check. The two days hang two pieces, one for each.
#[tokio::test]
async fn a_flagged_or_unchecked_piece_rests_and_the_board_is_no_fallback() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let day = next_piece_days();
    let next = day + Duration::days(1);
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    daily_board(&client, shown_day(next), BOARD_MIN_INK).await;
    let artist = late_core::test_utils::create_test_user(&db, "herald-artist").await;
    let flagged = hang_piece(&client, artist.id, "flagged").await;
    hang_piece(&client, artist.id, "unchecked").await;
    log.lock().expect("log").gemini_reply = serde_json::json!({
        "alt": "A drawn figure.",
        "warning": "explicit",
        "reason": "a drawn penis at the centre",
    })
    .to_string();
    let mut finished = None;

    let herald = service(db.clone(), &base, model_on(&base));
    assert!(
        herald
            .check(art_hour(day), HeraldSlot::Art, &mut finished)
            .await
    );
    assert_eq!(row(&client, day, HeraldSlot::Art).await.1, "skipped");
    {
        let log = log.lock().expect("log");
        assert_eq!(
            log.picture_paths,
            vec![format!("/gallery/pieces/{flagged}/image.png")],
            "the piece alone: the board was never fetched"
        );
        assert_eq!(log.gemini_calls, 1, "one look, at the piece");
    }

    // The model off: the next piece has no look, and stays off too.
    let herald = service(db.clone(), &base, ai_off());
    assert!(
        herald
            .check(art_hour(next), HeraldSlot::Art, &mut finished)
            .await
    );
    assert_eq!(row(&client, next, HeraldSlot::Art).await.1, "skipped");
    let log = log.lock().expect("log");
    assert_eq!(log.gemini_calls, 1, "no call with the model off");
    assert!(log.media_uploads.is_empty());
    assert!(log.statuses.is_empty());
    assert!(log.records.is_empty());
}

/// One public column printed for `edition`, the way the paper's tests do it.
async fn print_edition(client: &Client, edition: NaiveDate) {
    use late_core::models::paper::{PaperSectionKind, PaperSectionRow};

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

/// The cards are drawn from late.sh's own words and never shown to the
/// model: the week's scoreboard on Sunday, the paper on a Monday with no project
/// to post. Both carry the picture on both networks, alt text from the same
/// words.
#[tokio::test]
async fn the_cards_go_out_drawn_without_the_model() {
    use late_core::models::door_opening::{DoorCard, DoorOpening};

    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let sunday = Utc.with_ymd_and_hms(2026, 10, 25, 15, 30, 0).unwrap();
    let player = late_core::test_utils::create_test_user(&db, "card-player").await;
    DoorOpening::record(
        &client,
        player.id,
        DoorCard::Nethack,
        sunday - Duration::days(2),
    )
    .await
    .expect("a play in the week");
    let monday = sunday + Duration::days(1);
    print_edition(&client, monday.date_naive()).await;
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(sunday, HeraldSlot::Words, &mut finished).await);
    assert!(herald.check(monday, HeraldSlot::Words, &mut finished).await);
    let kinds = [sunday, monday].map(|at| at.date_naive());
    let mut posted = Vec::new();
    for day in kinds {
        let row = HeraldPost::posted_on(&client, day, HeraldSlot::Words)
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
        log.media_uploads[0].1.contains("NetHack: 1 play"),
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
}

/// A picture route that answers anything but a PNG fails the post before
/// any network is called: an older late-web's landing page never goes out
/// as the picture.
#[tokio::test]
async fn a_page_where_the_picture_should_be_fails_the_post() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let now = monday_art();
    let day = now.date_naive();
    daily_board(&client, shown_day(day), BOARD_MIN_INK).await;
    log.lock().expect("log").web_answers_html = true;

    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;
    assert!(herald.check(now, HeraldSlot::Art, &mut finished).await);
    assert_eq!(row(&client, day, HeraldSlot::Art).await.1, "failed");
    let log = log.lock().expect("log");
    assert_eq!(log.picture_paths.len(), 1);
    assert_eq!(log.gemini_calls, 0);
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
    use late_core::models::paper::PaperEdition;

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
    print_edition(client, edition).await;
}

/// The operator's word goes out in the words slot the day its edition prints,
/// ahead of the schedule, as words alone: the model's brief on both networks (no
/// media on Mastodon, a link card on Bluesky, the edition's page linked); and
/// when the model's answer is unusable, the note as written, cut.
#[tokio::test]
async fn an_announcement_goes_out_as_words_the_day_its_edition_prints() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    // A Friday, then a Saturday: the Saturday's printed paper waits and loses.
    let now = Utc.with_ymd_and_hms(2026, 10, 23, 15, 30, 0).unwrap();
    let day = now.date_naive();
    announce(&db, &client, day, "herald-admin").await;
    log.lock().expect("log").gemini_reply = serde_json::json!({
        "text": "A new late CLI is out, and server updates no longer kick you out.\n\nIdle five minutes when we ship and your session moves to the new server by itself.",
    })
    .to_string();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, HeraldSlot::Words, &mut finished).await);
    let posted = HeraldPost::posted_on(&client, day, HeraldSlot::Words)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(
        (posted.kind, posted.subject.as_str()),
        (HeraldKind::Announcement, "announcement:2026-10-23")
    );
    let words = "A new late CLI is out, and server updates no longer kick you out.\n\nIdle five minutes when we ship and your session moves to the new server by itself.\n\nFull notes: https://late.sh/paper/2026-10-23\n\n#tildeverse #terminal #TUI";
    {
        let log = log.lock().expect("log");
        assert_eq!(
            log.picture_paths,
            vec!["/static/og-image.png"],
            "only the card's thumb is fetched"
        );
        assert_eq!((log.gemini_calls, log.gemini_images), (1, 0));
        assert!(log.media_uploads.is_empty(), "no media on Mastodon");
        assert_eq!(log.blobs.len(), 1, "the thumb");
        let status = &log.statuses[0].2;
        assert_eq!(status["status"], words);
        assert_eq!(status["media_ids"], serde_json::json!([]));
        let record = &log.records[0].1["record"];
        assert_eq!(record["text"], words);
        assert_eq!(record["embed"]["$type"], "app.bsky.embed.external");
        let external = &record["embed"]["external"];
        assert_eq!(external["uri"], "https://late.sh/paper/2026-10-23");
        assert_eq!(external["title"], "The Late Edition, 23 October 2026");
        assert_eq!(external["description"], "The world turned.");
        assert!(external.get("thumb").is_some(), "{record}");
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
    assert!(
        herald
            .check(now + Duration::days(1), HeraldSlot::Words, &mut finished)
            .await
    );
    let posted = HeraldPost::posted_on(&client, next, HeraldSlot::Words)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(posted.kind, HeraldKind::Announcement);
    let stored = HeraldPost::stored_pick(&client, next, HeraldSlot::Words)
        .await
        .expect("read")
        .expect("stored");
    assert_eq!(stored.picture["Announcement"]["brief"], "cut");
    let log = log.lock().expect("log");
    let status = log.statuses[1].2["status"].as_str().expect("text");
    assert!(
        status.starts_with(
            "Server updates no longer kick you out\n\nA new `late` CLI is out: `curl -fsSL https: //cli.late.sh/install.sh | bash`\n"
        ),
        "the note as written, dead: {status}"
    );
    assert!(
        status.ends_with(
            "\n\nFull notes: https://late.sh/paper/2026-10-24\n\n#tildeverse #terminal #TUI"
        ),
        "{status}"
    );
}

/// A member's project, the way the editor saves one.
async fn share_project(client: &Client, user_id: Uuid, title: &str) -> Uuid {
    Showcase::create_by_user_id(
        client,
        user_id,
        ShowcaseParams {
            user_id,
            title: title.to_string(),
            url: format!("https://example.com/{title}"),
            description: "A clock for your terminal.".to_string(),
            tags: vec!["rust".to_string()],
        },
    )
    .await
    .expect("share a project")
    .id
}

/// What the model answers when it features a project. Its words try to mention
/// and link; they must go out dead.
fn featured() -> String {
    serde_json::json!({
        "verdict": "feature",
        "text": "A clock for your terminal, by way of @someone at https://other.example.",
        "reason": "",
    })
    .to_string()
}

fn refused() -> String {
    serde_json::json!({
        "verdict": "commercial",
        "text": "",
        "reason": "a paid service",
    })
    .to_string()
}

/// Monday's words are a member's project, written up by the model: words alone
/// under the member's own link, with a link card on Bluesky. It pays the week, so
/// Tuesday stays quiet with another project waiting; the next Monday posts that
/// other project, since the first is in its cooldown; with both in theirs, the
/// Monday after has nothing.
#[tokio::test]
async fn the_weeks_project_goes_out_once_and_waits_out_its_cooldown() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let member = late_core::test_utils::create_test_user(&db, "maker").await;
    let tinyclock = share_project(&client, member.id, "tinyclock").await;
    log.lock().expect("log").gemini_reply = featured();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;
    let now = monday_words();
    let day = now.date_naive();

    assert!(herald.check(now, HeraldSlot::Words, &mut finished).await);
    let posted = HeraldPost::posted_on(&client, day, HeraldSlot::Words)
        .await
        .expect("read")
        .expect("posted");
    assert_eq!(
        (posted.kind, posted.subject),
        (HeraldKind::Project, format!("project:{tinyclock}"))
    );
    let words = format!(
        "\"tinyclock\" by ~{}, from late.sh.\n\nA clock for your terminal, by way of someone at https: //other.example.\n\nhttps://example.com/tinyclock\n\n#tildeverse #terminal",
        member.username
    );
    {
        let log = log.lock().expect("log");
        assert_eq!((log.gemini_calls, log.gemini_images), (1, 0));
        assert!(
            log.gemini_prompt.contains("https://example.com/tinyclock")
                && log.gemini_prompt.contains("A clock for your terminal."),
            "the model is told the project: {}",
            log.gemini_prompt
        );
        assert_eq!(log.picture_paths, vec!["/static/og-image.png"]);
        assert!(log.media_uploads.is_empty(), "no media on Mastodon");
        let status = &log.statuses[0].2;
        assert_eq!(status["status"], words);
        assert_eq!(status["media_ids"], serde_json::json!([]));
        let record = &log.records[0].1["record"];
        assert_eq!(record["text"], words);
        let external = &record["embed"]["external"];
        assert_eq!(external["uri"], "https://example.com/tinyclock");
        assert_eq!(external["title"], "tinyclock");
        assert_eq!(
            external["description"],
            "A clock for your terminal, by way of someone at https: //other.example."
        );
        let links: Vec<&str> = record["facets"]
            .as_array()
            .expect("facets")
            .iter()
            .filter_map(|facet| facet["features"][0]["uri"].as_str())
            .collect();
        assert_eq!(
            links,
            vec!["https://example.com/tinyclock"],
            "the member's link alone is live"
        );
    }

    // Tuesday: another project waits, and the week is already paid.
    let lantern = share_project(&client, member.id, "lantern").await;
    assert!(
        herald
            .check(now + Duration::days(1), HeraldSlot::Words, &mut finished)
            .await
    );
    assert_eq!(
        row(&client, day + Duration::days(1), HeraldSlot::Words)
            .await
            .1,
        "skipped"
    );
    assert_eq!(log.lock().expect("log").gemini_calls, 1);

    // The next Monday: the first is in its cooldown, so the other goes.
    assert!(
        herald
            .check(now + Duration::days(7), HeraldSlot::Words, &mut finished)
            .await
    );
    assert_eq!(
        HeraldPost::posted_on(&client, day + Duration::days(7), HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .subject,
        format!("project:{lantern}")
    );

    // The Monday after: both in their cooldown, no paper, nothing.
    assert!(
        herald
            .check(now + Duration::days(14), HeraldSlot::Words, &mut finished)
            .await
    );
    assert_eq!(
        row(&client, day + Duration::days(14), HeraldSlot::Words)
            .await
            .1,
        "skipped"
    );
    let log = log.lock().expect("log");
    assert_eq!(log.gemini_calls, 2, "no call with nothing to draw");
    assert_eq!(log.statuses.len(), 2);
}

/// A project the model turns down goes unposted and the next is tried, three at
/// most; with all turned down the day's next kind goes, and the week still owes
/// a project: Tuesday's goes out.
#[tokio::test]
async fn a_refused_project_falls_to_the_paper_and_stays_owed() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let member = late_core::test_utils::create_test_user(&db, "maker").await;
    for title in ["one", "two", "three", "four"] {
        share_project(&client, member.id, title).await;
    }
    let now = monday_words();
    let day = now.date_naive();
    print_edition(&client, day).await;
    log.lock().expect("log").gemini_reply = refused();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, HeraldSlot::Words, &mut finished).await);
    assert_eq!(
        HeraldPost::posted_on(&client, day, HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .kind,
        HeraldKind::Paper
    );
    assert_eq!(log.lock().expect("log").gemini_calls, 3, "three tries");

    log.lock().expect("log").gemini_reply = featured();
    assert!(
        herald
            .check(now + Duration::days(1), HeraldSlot::Words, &mut finished)
            .await
    );
    assert_eq!(
        HeraldPost::posted_on(&client, day + Duration::days(1), HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .kind,
        HeraldKind::Project,
        "the week still owed one"
    );
}

/// The operator's word on Monday moves the project along to Tuesday.
#[tokio::test]
async fn an_announcement_moves_the_project_to_the_next_free_day() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let member = late_core::test_utils::create_test_user(&db, "maker").await;
    share_project(&client, member.id, "tinyclock").await;
    let now = monday_words();
    let day = now.date_naive();
    announce(&db, &client, day, "herald-admin").await;
    log.lock().expect("log").gemini_reply = featured();
    let herald = service(db.clone(), &base, model_on(&base));
    let mut finished = None;

    assert!(herald.check(now, HeraldSlot::Words, &mut finished).await);
    assert!(
        herald
            .check(now + Duration::days(1), HeraldSlot::Words, &mut finished)
            .await
    );
    let kinds = [
        HeraldPost::posted_on(&client, day, HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .kind,
        HeraldPost::posted_on(&client, day + Duration::days(1), HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .kind,
    ];
    assert_eq!(kinds, [HeraldKind::Announcement, HeraldKind::Project]);
}

/// A member's text never goes out unchecked: with the model off or failing no
/// project goes out, the day's next kind does, and the project stays owed until
/// the model answers.
#[tokio::test]
async fn no_project_goes_out_unchecked() {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let (base, log) = fake_servers().await;
    HeraldSwitch::set(&client, true).await.expect("switch on");
    let member = late_core::test_utils::create_test_user(&db, "maker").await;
    share_project(&client, member.id, "tinyclock").await;
    let now = monday_words();
    let day = now.date_naive();
    print_edition(&client, day).await;
    let mut finished = None;

    // Monday, the model off: the paper, and no call.
    let herald = service(db.clone(), &base, ai_off());
    assert!(herald.check(now, HeraldSlot::Words, &mut finished).await);
    assert_eq!(
        HeraldPost::posted_on(&client, day, HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .kind,
        HeraldKind::Paper
    );
    assert_eq!(log.lock().expect("log").gemini_calls, 0);

    // Tuesday, the model answering junk: one call, then nothing on the schedule.
    log.lock().expect("log").gemini_reply = "not json".to_string();
    let herald = service(db.clone(), &base, model_on(&base));
    assert!(
        herald
            .check(now + Duration::days(1), HeraldSlot::Words, &mut finished)
            .await
    );
    assert_eq!(
        row(&client, day + Duration::days(1), HeraldSlot::Words)
            .await
            .1,
        "skipped"
    );
    {
        let log = log.lock().expect("log");
        assert_eq!(log.gemini_calls, 1, "a failed call stops the walk");
        assert_eq!(log.statuses.len(), 1, "Monday's paper alone");
    }

    // Wednesday, the model back: the project is still owed.
    log.lock().expect("log").gemini_reply = featured();
    assert!(
        herald
            .check(now + Duration::days(2), HeraldSlot::Words, &mut finished)
            .await
    );
    assert_eq!(
        HeraldPost::posted_on(&client, day + Duration::days(2), HeraldSlot::Words)
            .await
            .expect("read")
            .expect("posted")
            .kind,
        HeraldKind::Project
    );
}
