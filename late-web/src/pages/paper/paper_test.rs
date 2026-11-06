use axum::{
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use chrono::{Duration, NaiveDate, Utc};
use late_core::{
    db::{Db, DbConfig},
    models::{
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
        paper::{PaperEdition, PaperRoomEdition, PaperSectionKind, PaperSectionRow},
    },
    test_utils::{create_test_user, test_db},
};
use tower::ServiceExt;

use super::feed_items;

const EDITION: NaiveDate = match NaiveDate::from_ymd_opt(2026, 9, 3) {
    Some(date) => date,
    None => unreachable!(),
};

/// The sweeper's attempt cap, as the tests here drive it.
const MAX_ATTEMPTS: i32 = 3;

/// `uri` through `router`: the status, the headers, the body.
async fn get(router: &axum::Router, uri: &str) -> (StatusCode, HeaderMap, String) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

fn router(db: &Db) -> axum::Router {
    crate::app(crate::AppState {
        config: crate::config::Config {
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".into(),
            audio_base_url: "http://127.0.0.1:9".into(),
            public_base_url: "http://late.test".into(),
            db: DbConfig::default(),
        },
        db: db.clone(),
        http_client: reqwest::Client::new(),
    })
}

/// Print `text` as `section` of `edition`, the way the press does.
async fn print_section(
    client: &late_core::db::PooledClient,
    edition: NaiveDate,
    section: PaperSectionKind,
    text: &str,
) {
    let stale_before = Utc::now() - Duration::minutes(15);
    assert!(
        PaperSectionRow::claim_printing(client, edition, section, stale_before, MAX_ATTEMPTS)
            .await
            .expect("claim")
    );
    PaperSectionRow::finish(client, edition, section, Some(text))
        .await
        .expect("finish");
}

/// One `#announcements` post by `author`, stamped inside `edition`'s
/// window the way the press's own tests stamp theirs.
async fn announce(
    client: &late_core::db::PooledClient,
    db: &Db,
    edition: NaiveDate,
    author: &str,
    body: &str,
) {
    let room = ChatRoom::find_non_dm_by_slug(client, "announcements")
        .await
        .expect("find announcements")
        .expect("announcements room");
    let user = create_test_user(db, author).await;
    let message = ChatMessage::create(
        client,
        ChatMessageParams {
            room_id: room.id,
            user_id: user.id,
            body: body.to_string(),
        },
    )
    .await
    .expect("create message");
    let (floor, _) = PaperEdition::window(edition);
    client
        .execute(
            "UPDATE chat_messages SET created = $2 WHERE id = $1",
            &[&message.id, &(floor + Duration::hours(1))],
        )
        .await
        .expect("backdate message");
}

/// A printed edition shows the operator's word, the reading column and
/// the outside column, never a room's column; leafs to the nearest
/// edition with a printed section; is the latest at `/paper`; and the
/// feed carries it newest first with its columns escaped once for the
/// reader. A day with nothing printed is 404, an announcement over an
/// unswept day included, and so is a path that is not a date.
#[tokio::test]
async fn the_paper_prints_its_columns_on_the_web_and_in_the_feed() {
    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    let stale_before = Utc::now() - Duration::minutes(15);

    print_section(
        &client,
        EDITION,
        PaperSectionKind::Reading,
        "We read about ducks.\nAnd geese & swans.",
    )
    .await;
    print_section(
        &client,
        EDITION,
        PaperSectionKind::Outside,
        "The world turned.",
    )
    .await;
    print_section(
        &client,
        EDITION - Duration::days(3),
        PaperSectionKind::Outside,
        "Three days before.",
    )
    .await;
    announce(
        &client,
        &test.db,
        EDITION,
        "operator",
        "maintenance tonight\nback by 02:00",
    )
    .await;
    // The day after is unswept: its announcement must wait for the press.
    announce(
        &client,
        &test.db,
        EDITION + Duration::days(1),
        "operator2",
        "too early",
    )
    .await;
    // A room column prints inside the clubhouse only.
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    assert!(
        PaperRoomEdition::claim_printing(
            &client,
            lounge.id,
            EDITION,
            12,
            3,
            stale_before,
            MAX_ATTEMPTS
        )
        .await
        .expect("claim room")
    );
    PaperRoomEdition::finish(&client, lounge.id, EDITION, Some("- lounge gossip"))
        .await
        .expect("finish room");

    let router = router(&test.db);

    let (status, _, html) = get(&router, "/paper/2026-09-03").await;
    assert_eq!(status, StatusCode::OK);
    for needle in [
        "We read about ducks.",
        "And geese &#38; swans.",
        "The world turned.",
        "@operator",
        "maintenance tonight",
        "back by 02:00",
        r#"href="/paper/2026-08-31""#,
        r#"<meta property="og:url" content="http://late.test/paper/2026-09-03">"#,
        r#"<meta property="og:title" content="The Late Edition, 3 September 2026">"#,
        r#"<meta property="og:description" content="The world turned.">"#,
        "Thursday, 3 September 2026",
    ] {
        assert!(html.contains(needle), "{needle}");
    }
    assert!(!html.contains("lounge gossip"));
    assert!(!html.contains("later &rarr;"));

    let (status, _, html) = get(&router, "/paper/2026-08-31").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("Three days before."));
    assert!(html.contains(r#"href="/paper/2026-09-03""#));
    assert!(!html.contains("&larr; earlier"));

    let (status, _, html) = get(&router, "/paper").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("The world turned."));

    let (status, _, html) = get(&router, "/paper/2026-09-04").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(html.contains("Nothing printed for this day."));
    assert!(!html.contains("too early"));
    let (status, _, _) = get(&router, "/paper/yesterday").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, headers, xml) = get(&router, "/paper/feed.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("application/rss+xml")
    );
    for needle in [
        r#"<?xml version="1.0" encoding="UTF-8"?>"#,
        "<link>http://late.test/paper/2026-09-03</link>",
        "<title>The Late Edition, 3 September 2026</title>",
        "<pubDate>Thu, 03 Sep 2026 00:00:00 +0000</pubDate>",
        // Askama escapes with numeric entities; the inner `&amp;` is the
        // HTML escape a reader keeps after unescaping the XML.
        "&#60;h3&#62;What we were reading&#60;/h3&#62;&#60;p&#62;We read about ducks.&#60;/p&#62;&#60;p&#62;And geese &#38;amp; swans.&#60;/p&#62;&#60;h3&#62;Outside&#60;/h3&#62;&#60;p&#62;The world turned.&#60;/p&#62;",
        "<link>http://late.test/paper/2026-08-31</link>",
    ] {
        assert!(xml.contains(needle), "{needle}");
    }
    assert!(xml.find("2026-09-03").unwrap() < xml.find("2026-08-31").unwrap());
}

/// One item per edition, newest first, both columns when both printed,
/// the text escaped once as HTML for the reader to unescape.
#[test]
fn feed_items_merge_both_columns_newest_first_and_escape_once() {
    let earlier = EDITION - Duration::days(2);
    let items = feed_items(
        "http://late.test",
        vec![(earlier, "ducks & geese".to_string())],
        vec![
            (EDITION, "the <world>".to_string()),
            (earlier, "quiet\n\nstreets".to_string()),
        ],
    );
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "The Late Edition, 3 September 2026");
    assert_eq!(items[0].url, "http://late.test/paper/2026-09-03");
    assert_eq!(items[0].pub_date, "Thu, 03 Sep 2026 00:00:00 +0000");
    assert_eq!(
        items[0].description_html,
        "<h3>Outside</h3><p>the &lt;world&gt;</p>"
    );
    assert_eq!(items[1].url, "http://late.test/paper/2026-09-01");
    assert_eq!(
        items[1].description_html,
        "<h3>What we were reading</h3><p>ducks &amp; geese</p><h3>Outside</h3><p>quiet</p><p>streets</p>"
    );
}
