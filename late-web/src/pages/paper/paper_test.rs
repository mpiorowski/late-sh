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
        leaderboard::{ArcadeGame, DailyPuzzle, ScoreGame},
        paper::{PaperEdition, PaperRoomEdition, PaperSectionKind, PaperSectionRow},
    },
    test_utils::{create_test_user, test_db},
};
use tower::ServiceExt;

use super::{feed_items, front_page};
use crate::pages::shared::card::{CardLine, Ink};

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
        PaperSectionKind::Yesterday,
        "Booth: 2 tracks queued\nArcade: 3 games finished, all Sudoku",
    )
    .await;
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
        "yesterday on late.sh",
        "Booth: 2 tracks queued",
        "Arcade: 3 games finished, all Sudoku",
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
    assert!(
        html.find("yesterday on late.sh").unwrap() < html.find("what we were reading").unwrap(),
        "the numbers come before the columns"
    );

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
        "&#60;h3&#62;Yesterday on late.sh&#60;/h3&#62;&#60;p&#62;Booth: 2 tracks queued&#60;/p&#62;&#60;p&#62;Arcade: 3 games finished, all Sudoku&#60;/p&#62;&#60;h3&#62;What we were reading&#60;/h3&#62;&#60;p&#62;We read about ducks.&#60;/p&#62;&#60;p&#62;And geese &#38;amp; swans.&#60;/p&#62;&#60;h3&#62;Outside&#60;/h3&#62;&#60;p&#62;The world turned.&#60;/p&#62;",
        "<link>http://late.test/paper/2026-08-31</link>",
    ] {
        assert!(xml.contains(needle), "{needle}");
    }
    assert!(xml.find("2026-09-03").unwrap() < xml.find("2026-08-31").unwrap());
}

/// One item per edition, newest first, every printed column in page
/// order, the text escaped once as HTML for the reader to unescape.
#[test]
fn feed_items_merge_the_columns_newest_first_and_escape_once() {
    let earlier = EDITION - Duration::days(2);
    let items = feed_items(
        "http://late.test",
        vec![(earlier, "Booth: 1 track queued".to_string())],
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
        "<h3>Yesterday on late.sh</h3><p>Booth: 1 track queued</p><h3>What we were reading</h3><p>ducks &amp; geese</p><h3>Outside</h3><p>quiet</p><p>streets</p>"
    );
}

/// The front page's lines: the masthead, the dateline, the most played
/// arcade game with its picture and its finishes when anyone played, the
/// numbers whole and then each printed column's head under its name (a
/// column that did not print is left out), the way in; nothing when none
/// printed, whoever played.
#[test]
fn the_front_page_card_leads_with_the_most_played_game() {
    let line = |ink, text: &str| CardLine::new(ink, text);
    let yesterday = [
        "Booth: 2 tracks queued".to_string(),
        "Arcade: 3 games finished, all Sudoku".to_string(),
    ];
    let sudoku = Some((ArcadeGame::Puzzle(DailyPuzzle::Sudoku), 3));
    assert_eq!(
        front_page(
            EDITION,
            sudoku,
            &yesterday,
            &["The world turned.".to_string()],
            &[]
        ),
        Some(vec![
            line(Ink::Title, "THE LATE EDITION"),
            line(Ink::Dim, "Thursday, 3 September 2026"),
            CardLine::blank(),
            line(Ink::Heading, "MOST PLAYED YESTERDAY"),
            line(Ink::Art, "  +---+---+---+"),
            line(Ink::Art, "  | 5 |   | 3 |"),
            line(Ink::Art, "  +---+---+---+"),
            line(Ink::Art, "  |   | 7 |   |"),
            line(Ink::Art, "  +---+---+---+"),
            line(Ink::Body, "Sudoku, 3 games finished"),
            CardLine::blank(),
            line(Ink::Heading, "YESTERDAY ON LATE.SH"),
            line(Ink::Body, "Booth: 2 tracks queued"),
            line(Ink::Body, "Arcade: 3 games finished, all Sudoku"),
            CardLine::blank(),
            line(Ink::Heading, "OUTSIDE"),
            line(Ink::Body, "The world turned."),
            CardLine::blank(),
            line(Ink::Dim, "late.sh/paper"),
        ])
    );
    let quiet = front_page(EDITION, None, &[], &["The world turned.".to_string()], &[])
        .expect("a column printed");
    assert_eq!(quiet.len(), 7, "nobody played: no headline");
    let one = front_page(
        EDITION,
        Some((ArcadeGame::Score(ScoreGame::Snake), 1)),
        &[],
        &["The world turned.".to_string()],
        &[],
    )
    .expect("a column printed");
    assert!(
        one.contains(&line(Ink::Body, "Snake, 1 game finished")),
        "{one:?}"
    );
    assert_eq!(front_page(EDITION, sudoku, &[], &[], &[]), None);
}

/// The card route serves a printed edition as a PNG; an unprinted day and
/// a path that is not a date are 404.
#[tokio::test]
async fn the_front_page_card_is_a_png_for_a_printed_edition() {
    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    print_section(
        &client,
        EDITION,
        PaperSectionKind::Reading,
        "We read about ducks.",
    )
    .await;
    let router = router(&test.db);

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/paper/2026-09-03/image.png")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let png = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(png.starts_with(b"\x89PNG"));
    for missing in ["/paper/2026-09-02/image.png", "/paper/someday/image.png"] {
        assert_eq!(
            get(&router, missing).await.0,
            StatusCode::NOT_FOUND,
            "{missing}"
        );
    }
}
