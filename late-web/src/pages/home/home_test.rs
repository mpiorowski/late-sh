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
