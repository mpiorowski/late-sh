use late_core::models::work_profile::WorkStatus;

use super::{dash_or, parse_contacts, render_markdown, split_paragraphs, status_label};

#[test]
fn avatar_json_keeps_cells_colours_and_cannot_close_its_script_element() {
    use dartboard_core::{Canvas, Pos, RgbColor};
    use late_core::models::avatar::Avatar;
    let mut board = Canvas::with_size(32, 32);
    for (x, ch) in "</script><script>".chars().enumerate() {
        board.put_glyph_colored(Pos { x, y: 0 }, ch, RgbColor::new(255, 0, 80));
    }
    board.put_glyph(Pos { x: 0, y: 2 }, '界');
    let avatar = Avatar::crop(&board, Pos { x: 0, y: 0 }, 32, 4).unwrap();
    let (json, text, size) = super::avatar_data(Some(&avatar));
    assert_eq!(size, 32);
    assert!(!json.contains('<'));
    assert!(text.contains("</script><script>"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["width"], 32);
    assert_eq!(value["height"], 4);
    assert_eq!(text.lines().count(), 4);
    assert_eq!(
        value["cells"][0],
        serde_json::json!([0, 0, "<", 1, "#ff0050"])
    );
    assert!(
        value["cells"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!([0, 2, "界", 2, null]))
    );
    assert_eq!(super::avatar_data(None), ("null".into(), String::new(), 0));
}

#[tokio::test]
async fn web_profile_shows_saved_avatar_and_hides_it_after_removal() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use dartboard_core::{Canvas, Pos};
    use late_core::{
        db::DbConfig,
        models::{
            avatar::Avatar,
            work_profile::{WorkProfile, WorkProfileParams, WorkType},
        },
        test_utils::{create_test_user, test_db},
    };
    use tower::ServiceExt;
    let test = test_db().await;
    let user = create_test_user(&test.db, "web-avatar").await;
    let client = test.db.get().await.unwrap();
    let slug = "w_avatartest01";
    WorkProfile::create_by_user_id(
        &client,
        user.id,
        WorkProfileParams {
            user_id: user.id,
            slug: slug.into(),
            headline: "Terminal artist".into(),
            status: WorkStatus::Open,
            work_type: WorkType::Any,
            location: "Melbourne".into(),
            contact: String::new(),
            links: vec!["https://example.com".into()],
            skills: vec![],
            skills_tags: vec![],
            summary: "Drawn in public.".into(),
        },
    )
    .await
    .unwrap();
    let mut board = Canvas::with_size(32, 32);
    board.put_glyph(Pos { x: 0, y: 0 }, '@');
    let avatar = Avatar::crop(&board, Pos { x: 0, y: 0 }, 1, 32).unwrap();
    Avatar::save(&client, user.id, Some(&avatar)).await.unwrap();
    let router = crate::app(crate::AppState {
        config: crate::config::Config {
            env: crate::config::Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".into(),
            audio_base_url: "http://127.0.0.1:9".into(),
            db: DbConfig::default(),
        },
        db: test.db.clone(),
        http_client: reqwest::Client::new(),
    });
    for present in [true, false] {
        if !present {
            Avatar::save(&client, user.id, None).await.unwrap();
        }
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
        let html = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert_eq!(html.contains("id=\"profile-avatar\""), present);
        assert_eq!(html.contains("/static/avatar.js"), present);
        if present {
            assert!(html.contains("\"width\":1"));
            assert!(html.contains("\"height\":32"));
            assert!(html.contains("save PNG"));
        }
    }
}

#[test]
fn paragraphs_drop_empty_and_trim() {
    let para = split_paragraphs("hello\n\n  world  \n");
    assert_eq!(para, vec!["hello", "world"]);
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
