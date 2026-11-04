use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use late_core::db::{Db, DbConfig};
use tower::ServiceExt;

use crate::{
    AppState, app,
    config::{Config, Env},
};

#[tokio::test]
async fn the_readiness_probe_gets_a_plain_ok() {
    // Readiness needs no DB or upstream service; the pool stays inert.
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
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
}
