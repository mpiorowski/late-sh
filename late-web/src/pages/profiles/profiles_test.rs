use late_core::models::work_profile::WorkStatus;

use super::{dash_or, parse_contacts, render_markdown, status_label};

/// `/profiles/{slug}` for `router`, as the page's HTML.
async fn page(router: &axum::Router, slug: &str) -> String {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/profiles/{slug}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

/// A standing runner's poster hangs on their web profile, its cells as JSON
/// inside a script element that the cells themselves cannot close; a
/// profile with no runner behind it, and one whose runner left, has none.
#[tokio::test]
async fn a_runners_poster_hangs_on_their_web_profile() {
    use dartboard_core::{Canvas, Pos, RgbColor};
    use late_core::{
        db::DbConfig,
        models::{
            deadchannel_runner::DeadchannelRunner,
            work_profile::{WorkProfile, WorkProfileParams, WorkType},
        },
        test_utils::{create_test_user, test_db},
    };

    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    let card = |user_id, slug: &str| WorkProfileParams {
        user_id,
        slug: slug.into(),
        headline: "Terminal artist".into(),
        status: WorkStatus::Open,
        work_type: WorkType::Any,
        location: "Melbourne".into(),
        contact: String::new(),
        links: vec!["https://example.com".into()],
        skills: vec![],
        skills_tags: vec![],
        summary: "Painted at the press.".into(),
    };
    let runner = create_test_user(&test.db, "web-poster").await;
    WorkProfile::create_by_user_id(&client, runner.id, card(runner.id, "w_postertest01"))
        .await
        .unwrap();
    let civilian = create_test_user(&test.db, "web-civilian").await;
    WorkProfile::create_by_user_id(&client, civilian.id, card(civilian.id, "w_postertest02"))
        .await
        .unwrap();

    DeadchannelRunner::ensure_for_user(&client, runner.id, &serde_json::json!({}))
        .await
        .unwrap();
    let mut poster = Canvas::with_size(32, 12);
    for (x, ch) in "</script><script>".chars().enumerate() {
        poster.put_glyph_colored(Pos { x, y: 0 }, ch, RgbColor::new(255, 0, 80));
    }
    DeadchannelRunner::store_poster(&client, runner.id, &serde_json::to_value(&poster).unwrap())
        .await
        .unwrap();

    let router = crate::app(crate::AppState {
        config: crate::config::Config {
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".into(),
            audio_base_url: "http://127.0.0.1:9".into(),
            public_base_url: "http://late.test".into(),
            db: DbConfig::default(),
        },
        db: test.db.clone(),
        http_client: reqwest::Client::new(),
    });

    let html = page(&router, "w_postertest01").await;
    let open = r#"<script id="poster-data" type="application/json">"#;
    let data = &html[html.find(open).expect("the poster's data") + open.len()..];
    let data = &data[..data.find("</script>").expect("the data's script closes")];
    let board: serde_json::Value = serde_json::from_str(data).expect("the whole board, unbroken");
    assert_eq!(
        (board["width"].clone(), board["height"].clone()),
        (32.into(), 12.into())
    );
    assert_eq!(
        board["cells"][0],
        serde_json::json!([0, 0, "<", 1, "#FF0050", null])
    );
    assert_eq!(board["cells"].as_array().unwrap().len(), 17);
    for needle in [
        r#"id="profile-poster""#,
        "/static/board.js",
        "/static/poster.js",
        "save PNG",
    ] {
        assert!(html.contains(needle), "{needle}");
    }

    let html = page(&router, "w_postertest02").await;
    assert!(!html.contains(r#"id="profile-poster""#));
    assert!(!html.contains("/static/board.js"));

    DeadchannelRunner::mark_left(&client, runner.id)
        .await
        .unwrap();
    let html = page(&router, "w_postertest01").await;
    assert!(!html.contains(r#"id="profile-poster""#));
}

#[test]
fn status_label_spells_out_every_status() {
    assert_eq!(status_label(WorkStatus::Open), "open to work");
    assert_eq!(status_label(WorkStatus::Casual), "casually listening");
    assert_eq!(status_label(WorkStatus::NotLooking), "not looking");
}

#[test]
fn dash_or_handles_blank() {
    assert_eq!(dash_or(None), "—");
    assert_eq!(dash_or(Some("   ")), "—");
    assert_eq!(dash_or(Some(" rust ")), "rust");
}

#[test]
fn render_markdown_renders_headings_and_lists() {
    let html = render_markdown("# Hi\n\n- one\n- two");
    assert!(html.contains("<h1>Hi</h1>"));
    assert!(html.contains("<li>one</li>"));
}

#[test]
fn render_markdown_strips_raw_html() {
    // Raw HTML in source must not survive — author content is untrusted.
    let html = render_markdown("hello <script>alert(1)</script> world");
    assert!(!html.contains("<script>"));
    assert!(html.contains("hello"));
    assert!(html.contains("world"));
}

#[test]
fn render_markdown_empty_returns_empty() {
    assert_eq!(render_markdown(""), "");
    assert_eq!(render_markdown("   \n  "), "");
}

#[test]
fn parse_contacts_splits_on_commas_and_trims() {
    let items = parse_contacts("foo@bar.com, DM on late.sh ,  ");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].value, "foo@bar.com");
    assert_eq!(items[0].href, "mailto:foo@bar.com");
    assert_eq!(items[1].value, "DM on late.sh");
    assert_eq!(items[1].href, "");
}

#[test]
fn parse_contacts_links_urls_but_not_bare_text() {
    let items = parse_contacts("https://t.me/me, just say hi");
    assert_eq!(items[0].href, "https://t.me/me");
    assert_eq!(items[1].href, "");
}

#[test]
fn parse_contacts_empty_input_yields_empty_list() {
    assert!(parse_contacts("").is_empty());
    assert!(parse_contacts("   ,  , ").is_empty());
}

/// `uri` through `router`: the status, the headers, the body bytes.
async fn fetch(
    router: &axum::Router,
    uri: &str,
) -> (axum::http::StatusCode, axum::http::HeaderMap, Vec<u8>) {
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;

    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, headers, body.to_vec())
}

/// A PNG's width and height.
fn png_size(png: &[u8]) -> (u32, u32) {
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    (reader.info().width, reader.info().height)
}

/// A profile's contact and links carry `rel="me"`, so a Mastodon profile
/// that links back here shows the link verified. The page's preview card
/// names the author and, for a standing runner, shows the poster, which
/// `/profiles/{slug}/poster.png` serves as a PNG. A civilian's card falls
/// back to the site's picture and their poster route is 404, as is a
/// leaver's and an unknown slug's.
#[tokio::test]
async fn a_profile_verifies_its_links_and_unfurls_into_its_poster() {
    use axum::http::StatusCode;
    use dartboard_core::{Canvas, Pos, RgbColor};
    use late_core::{
        db::DbConfig,
        models::{
            deadchannel_runner::DeadchannelRunner,
            work_profile::{WorkProfile, WorkProfileParams, WorkType},
        },
        test_utils::{create_test_user, test_db},
    };

    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    let card = |user_id, slug: &str| WorkProfileParams {
        user_id,
        slug: slug.into(),
        headline: "Terminal artist".into(),
        status: WorkStatus::Open,
        work_type: WorkType::Any,
        location: "Melbourne".into(),
        contact: "https://fosstodon.org/@late, hi@example.com".into(),
        links: vec!["https://example.com".into()],
        skills: vec![],
        skills_tags: vec![],
        summary: "Painted at the press.".into(),
    };
    let runner = create_test_user(&test.db, "web-card").await;
    WorkProfile::create_by_user_id(&client, runner.id, card(runner.id, "w_cardtest0001"))
        .await
        .unwrap();
    let civilian = create_test_user(&test.db, "web-plain").await;
    WorkProfile::create_by_user_id(&client, civilian.id, card(civilian.id, "w_cardtest0002"))
        .await
        .unwrap();
    DeadchannelRunner::ensure_for_user(&client, runner.id, &serde_json::json!({}))
        .await
        .unwrap();
    let mut poster = Canvas::with_size(32, 12);
    poster.put_glyph_colored(Pos { x: 0, y: 0 }, '█', RgbColor::new(255, 0, 80));
    DeadchannelRunner::store_poster(&client, runner.id, &serde_json::to_value(&poster).unwrap())
        .await
        .unwrap();

    let router = crate::app(crate::AppState {
        config: crate::config::Config {
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".into(),
            audio_base_url: "http://127.0.0.1:9".into(),
            public_base_url: "http://late.test".into(),
            db: DbConfig::default(),
        },
        db: test.db.clone(),
        http_client: reqwest::Client::new(),
    });

    let html = page(&router, "w_cardtest0001").await;
    for needle in [
        r#"<a href="https://fosstodon.org/@late" rel="me""#,
        r#"<a href="mailto:hi@example.com" rel="me""#,
        r#"<a href="https://example.com" target="_blank" rel="me noopener noreferrer""#,
        r#"<meta property="og:url" content="http://late.test/profiles/w_cardtest0001">"#,
        r#"<meta property="og:image" content="http://late.test/profiles/w_cardtest0001/poster.png">"#,
        &format!(
            r#"<meta property="og:title" content="@{} on late.sh">"#,
            runner.username
        ),
        r#"<meta property="og:description" content="Terminal artist · open to work">"#,
        r#"<meta property="twitter:card" content="summary_large_image">"#,
    ] {
        assert!(html.contains(needle), "{needle}");
    }

    let (status, headers, png) = fetch(&router, "/profiles/w_cardtest0001/poster.png").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["cache-control"], "public, max-age=600");
    assert_eq!(png_size(&png), (1536, 1152));

    let html = page(&router, "w_cardtest0002").await;
    assert!(
        html.contains(
            r#"<meta property="og:image" content="http://late.test/static/og-image.png">"#
        )
    );
    let (status, _, _) = fetch(&router, "/profiles/w_cardtest0002/poster.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = fetch(&router, "/profiles/w_nobody/poster.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    DeadchannelRunner::mark_left(&client, runner.id)
        .await
        .unwrap();
    let (status, _, _) = fetch(&router, "/profiles/w_cardtest0001/poster.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let html = page(&router, "w_cardtest0001").await;
    assert!(
        html.contains(
            r#"<meta property="og:image" content="http://late.test/static/og-image.png">"#
        )
    );
}
