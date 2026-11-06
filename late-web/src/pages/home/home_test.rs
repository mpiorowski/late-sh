use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header::LOCATION},
};
use late_core::db::{Db, DbConfig};
use tower::ServiceExt;

use crate::{
    AppState, app,
    config::{Config, Env},
};

#[tokio::test]
async fn thanks_redirects_get_and_head_to_ko_fi() {
    // The redirect needs no DB or upstream service; the pool stays inert.
    let db = DbConfig::default();
    let router = app(AppState {
        config: Config {
            env: Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".to_string(),
            audio_base_url: "http://127.0.0.1:9".to_string(),
            public_base_url: "http://late.test".to_string(),
            db: db.clone(),
        },
        db: Db::new(&db).expect("lazy db"),
        http_client: reqwest::Client::new(),
    });

    for method in [Method::GET, Method::HEAD] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri("/thanks")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()[LOCATION],
            "https://ko-fi.com/mateuszpiorowski"
        );
    }
}

/// The landing page is the URL in the herald's Mastodon profile, and the
/// `rel="me"` link back to that profile is what makes Mastodon show the
/// late.sh link verified there (the same mechanism as the members'
/// profile links). Dropping it un-verifies the account.
#[tokio::test]
async fn the_landing_page_links_back_to_the_herald_accounts_with_rel_me() {
    let db = DbConfig::default();
    let router = app(AppState {
        config: Config {
            env: Env::Dev,
            port: 0,
            ssh_internal_url: "http://127.0.0.1:9".to_string(),
            audio_base_url: "http://127.0.0.1:9".to_string(),
            public_base_url: "http://late.test".to_string(),
            db: db.clone(),
        },
        db: Db::new(&db).expect("lazy db"),
        http_client: reqwest::Client::new(),
    });

    let response = router
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let html = String::from_utf8(body.to_vec()).unwrap();
    for anchor in [
        r#"<a href="https://fosstodon.org/@late" rel="me"#,
        r#"<a href="https://bsky.app/profile/late.sh" rel="me"#,
    ] {
        assert!(html.contains(anchor), "missing {anchor}");
    }
}
