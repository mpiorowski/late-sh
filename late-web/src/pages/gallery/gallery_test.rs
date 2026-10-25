use super::{snapshot_label, snapshot_title};

#[test]
fn snapshot_labels_are_human_readable() {
    assert_eq!(snapshot_label("main"), "Live");
    assert_eq!(snapshot_label("curated:2026-05-25"), "2026-05-25");
    assert_eq!(snapshot_label("daily:2026-04-24"), "2026-04-24");
    assert_eq!(snapshot_label("monthly:2026-04"), "2026-04");
}

#[test]
fn snapshot_titles_include_kind() {
    assert_eq!(snapshot_title("main"), "Live / latest saved");
    assert_eq!(snapshot_title("curated:2026-05-25"), "Curated 2026-05-25");
    assert_eq!(snapshot_title("daily:2026-04-24"), "Daily 2026-04-24");
    assert_eq!(snapshot_title("monthly:2026-04"), "Monthly 2026-04");
}

#[test]
fn canvas_json_keeps_cells_colours_and_cannot_close_its_script_element() {
    use dartboard_core::{Canvas, Pos, RgbColor};

    let mut canvas = Canvas::with_size(32, 4);
    for (x, ch) in "</script><script>".chars().enumerate() {
        canvas.put_glyph_colored(Pos { x, y: 0 }, ch, RgbColor::new(255, 0, 80));
    }
    canvas.put_glyph(Pos { x: 0, y: 2 }, '界');
    let json = super::canvas_json(&canvas).unwrap();
    assert!(!json.contains("</"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["width"], 32);
    assert_eq!(value["height"], 4);
    assert_eq!(
        value["cells"][0],
        serde_json::json!([0, 0, "<", 1, "#FF0050", null])
    );
    assert!(
        value["cells"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!([0, 2, "界", 2, null, null]))
    );
}

/// A snapshot's link unfurls into the board itself: the gallery page names
/// the snapshot's picture at `/gallery/{key}/image.png`, which serves it as
/// a PNG at preview-card size, and 404 for a key nobody saved.
#[tokio::test]
async fn a_gallery_link_unfurls_into_its_snapshot() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use dartboard_core::{Canvas, Pos, RgbColor};
    use late_core::{db::DbConfig, models::artboard::Snapshot, test_utils::test_db};
    use tower::ServiceExt;

    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    let mut canvas = Canvas::with_size(8, 4);
    canvas.put_glyph_colored(Pos { x: 0, y: 0 }, '█', RgbColor::new(0, 255, 0));
    Snapshot::upsert(
        &client,
        "daily:2026-09-03",
        serde_json::to_value(&canvas).unwrap(),
        serde_json::json!({ "cells": [] }),
    )
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
    let fetch = |uri: &'static str| {
        let router = router.clone();
        async move {
            let response = router
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let content_type = response
                .headers()
                .get("content-type")
                .map(|value| value.to_str().unwrap().to_string());
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            (status, content_type, body.to_vec())
        }
    };

    let (status, _, html) = fetch("/gallery?key=daily:2026-09-03").await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(html).unwrap();
    for needle in [
        r#"<meta property="og:url" content="http://late.test/gallery?key=daily:2026-09-03">"#,
        r#"<meta property="og:title" content="Artboard: Daily 2026-09-03">"#,
        r#"<meta property="og:image" content="http://late.test/gallery/daily:2026-09-03/image.png">"#,
    ] {
        assert!(html.contains(needle), "{needle}");
    }

    let (status, content_type, png) = fetch("/gallery/daily:2026-09-03/image.png").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content_type.as_deref(), Some("image/png"));
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    assert_eq!((reader.info().width, reader.info().height), (384, 384));

    let (status, _, _) = fetch("/gallery/daily:2099-01-01/image.png").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // The herald's crop: the cells inside, at the scale of their width.
    let (status, _, png) = fetch("/gallery/daily:2026-09-03/image.png?crop=0,0,4,2").await;
    assert_eq!(status, StatusCode::OK);
    let reader = png::Decoder::new(std::io::Cursor::new(png))
        .read_info()
        .unwrap();
    assert_eq!((reader.info().width, reader.info().height), (192, 192));
    for bad in [
        "/gallery/daily:2026-09-03/image.png?crop=0,0,9,1",
        "/gallery/daily:2026-09-03/image.png?crop=0,0,0,1",
        "/gallery/daily:2026-09-03/image.png?crop=1,2,3",
    ] {
        assert_eq!(fetch(bad).await.0, StatusCode::BAD_REQUEST, "{bad}");
    }
}

/// A piece has a page of its own, credited and unfurling into its PNG;
/// an unknown id, a path that is no id, and a piece taken down are 404.
#[tokio::test]
async fn a_piece_has_a_page_and_a_picture_until_it_comes_down() {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use dartboard_core::{Canvas, Pos};
    use late_core::{
        db::DbConfig,
        models::artboard_piece::{ArtboardPiece, HangOutcome, HangParams},
        test_utils::{create_test_user, test_db},
    };
    use tower::ServiceExt;

    let test = test_db().await;
    let client = test.db.get().await.unwrap();
    let painter = create_test_user(&test.db, "piece-painter").await;
    let mut canvas = Canvas::with_size(12, 4);
    canvas.put_glyph(Pos { x: 0, y: 0 }, '#');
    let HangOutcome::Hung(piece) = ArtboardPiece::hang(
        &client,
        HangParams {
            user_id: painter.id,
            title: "harbour <at> dusk".to_string(),
            width: 12,
            height: 4,
            canvas: serde_json::to_value(&canvas).unwrap(),
            provenance: serde_json::json!({ "cells": [] }),
            glyph_count: 40,
            own_share_percent: 100,
            content_hash: "hash-harbour".to_string(),
        },
    )
    .await
    .unwrap() else {
        panic!("hung");
    };

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
    let fetch = |uri: String| {
        let router = router.clone();
        async move {
            let response = router
                .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let status = response.status();
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            (status, body.to_vec())
        }
    };
    let page = format!("/gallery/pieces/{}", piece.id);

    let (status, html) = fetch(page.clone()).await;
    assert_eq!(status, StatusCode::OK);
    let html = String::from_utf8(html).unwrap();
    assert!(!html.contains("<at>"), "the title is escaped");
    for needle in [
        "harbour ".to_string(),
        "by ~piece-painter".to_string(),
        format!(r#"<meta property="og:image" content="http://late.test{page}/image.png">"#),
    ] {
        assert!(html.contains(&needle), "{needle}");
    }
    let (status, png) = fetch(format!("{page}/image.png")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(png.starts_with(b"\x89PNG"));

    for missing in [
        format!("/gallery/pieces/{}", uuid::Uuid::now_v7()),
        "/gallery/pieces/not-an-id".to_string(),
        "/gallery/pieces/not-an-id/image.png".to_string(),
    ] {
        assert_eq!(
            fetch(missing.clone()).await.0,
            StatusCode::NOT_FOUND,
            "{missing}"
        );
    }
    ArtboardPiece::remove(&client, piece.id).await.unwrap();
    assert_eq!(fetch(page.clone()).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        fetch(format!("{page}/image.png")).await.0,
        StatusCode::NOT_FOUND
    );
}
