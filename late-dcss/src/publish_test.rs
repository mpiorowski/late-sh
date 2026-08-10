use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use tower::ServiceExt as _;

use super::{
    ListingEntry, PathReject, RangeSpec, parse_range, render_listing, router, sanitize_rel,
};

#[test]
fn sanitize_rel_keeps_plain_components() {
    assert_eq!(
        sanitize_rel("mat/morgue-mat-20260810-120000.txt"),
        Ok(std::path::PathBuf::from(
            "mat/morgue-mat-20260810-120000.txt"
        ))
    );
    // A trailing or doubled slash is just an empty component.
    assert_eq!(sanitize_rel("mat//"), Ok(std::path::PathBuf::from("mat")));
    assert_eq!(sanitize_rel(""), Ok(std::path::PathBuf::new()));
}

#[test]
fn sanitize_rel_refuses_escapes_and_hidden_files() {
    assert_eq!(sanitize_rel("../logfile"), Err(PathReject::Traversal));
    assert_eq!(sanitize_rel("mat/../../.crawl"), Err(PathReject::Traversal));
    assert_eq!(sanitize_rel("./mat"), Err(PathReject::Traversal));
    assert_eq!(sanitize_rel(".hidden"), Err(PathReject::Hidden));
    assert_eq!(sanitize_rel("mat/a\\b"), Err(PathReject::Invalid));
    assert_eq!(sanitize_rel("mat/a\0b"), Err(PathReject::Invalid));
}

#[test]
fn parse_range_reads_the_incremental_fetch_shape() {
    // What an ecosystem fetcher sends: everything appended since its last size.
    assert_eq!(
        parse_range("bytes=100-", 250),
        RangeSpec::Partial {
            start: 100,
            end: 249
        }
    );
    assert_eq!(
        parse_range("bytes=0-9", 250),
        RangeSpec::Partial { start: 0, end: 9 }
    );
    // An end past EOF clamps rather than failing.
    assert_eq!(
        parse_range("bytes=200-999", 250),
        RangeSpec::Partial {
            start: 200,
            end: 249
        }
    );
    // Suffix form: the last N bytes.
    assert_eq!(
        parse_range("bytes=-50", 250),
        RangeSpec::Partial {
            start: 200,
            end: 249
        }
    );
    assert_eq!(
        parse_range("bytes=-999", 250),
        RangeSpec::Partial { start: 0, end: 249 }
    );
}

#[test]
fn parse_range_reports_unsatisfiable_and_ignorable_headers() {
    // Caught up: the fetcher's cursor is already at EOF.
    assert_eq!(parse_range("bytes=250-", 250), RangeSpec::Unsatisfiable);
    assert_eq!(parse_range("bytes=9-4", 250), RangeSpec::Unsatisfiable);
    assert_eq!(parse_range("bytes=-0", 250), RangeSpec::Unsatisfiable);
    assert_eq!(parse_range("bytes=0-", 0), RangeSpec::Unsatisfiable);
    // Anything we do not implement falls back to serving the whole file.
    assert_eq!(parse_range("bytes=0-9,20-29", 250), RangeSpec::Full);
    assert_eq!(parse_range("items=0-9", 250), RangeSpec::Full);
    assert_eq!(parse_range("bytes=abc-", 250), RangeSpec::Full);
    assert_eq!(parse_range("nonsense", 250), RangeSpec::Full);
}

#[test]
fn render_listing_links_entries_and_marks_directories() {
    let listing = render_listing(
        "/morgue/",
        &[
            ListingEntry {
                name: "mat".to_string(),
                is_dir: true,
                size: 0,
                modified: None,
            },
            ListingEntry {
                name: "morgue-mat-20260810-120000.txt".to_string(),
                is_dir: false,
                size: 4096,
                modified: None,
            },
        ],
    );
    assert!(listing.contains("<h1>Index of /morgue/</h1>"));
    assert!(listing.contains("<a href=\"../\">../</a>"));
    assert!(listing.contains("<a href=\"mat/\">mat/</a>"));
    assert!(
        listing.contains(
            "<a href=\"morgue-mat-20260810-120000.txt\">morgue-mat-20260810-120000.txt</a>"
        )
    );
    assert!(listing.contains("4096"));
}

/// A throwaway `$HOME` with the crawl tree a live playground would have.
fn playground() -> tempfile::TempDir {
    let home = tempfile::tempdir().expect("tempdir");
    let crawl = home.path().join(".crawl");
    std::fs::create_dir_all(crawl.join("morgue/mat")).expect("morgue dir");
    std::fs::write(crawl.join("logfile"), "v=0.34.1:name=mat:sc=1234\n").expect("logfile");
    std::fs::write(crawl.join("milestones"), "v=0.34.1:name=mat:type=orb\n").expect("milestones");
    std::fs::write(
        crawl.join("morgue/mat/morgue-mat-20260810-120000.txt"),
        "dungeon crawl stone soup version 0.34.1\n",
    )
    .expect("morgue dump");
    // Beside the published files, and never reachable through them.
    std::fs::write(crawl.join("mat.rc"), "show_more = false\n").expect("rc file");
    home
}

async fn get(
    home: &tempfile::TempDir,
    uri: &str,
    range: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let mut request = Request::builder().uri(uri);
    if let Some(range) = range {
        request = request.header(header::RANGE, range);
    }
    let response = router(home.path().to_str().expect("utf-8 tempdir"))
        .oneshot(request.body(Body::empty()).expect("request"))
        .await
        .expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    (status, headers, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn serves_the_shared_logs_whole_and_by_range() {
    let home = playground();

    let (status, headers, body) = get(&home, "/crawl/logfile", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "v=0.34.1:name=mat:sc=1234\n");
    assert_eq!(headers[header::CONTENT_TYPE], "text/plain; charset=utf-8");
    assert_eq!(headers[header::ACCEPT_RANGES], "bytes");
    assert!(headers.contains_key(header::LAST_MODIFIED));

    // The incremental fetch: only what was appended past the caller's cursor.
    let (status, headers, body) = get(&home, "/crawl/logfile", Some("bytes=18-")).await;
    assert_eq!(status, StatusCode::PARTIAL_CONTENT);
    assert_eq!(body, "sc=1234\n");
    assert_eq!(headers[header::CONTENT_RANGE], "bytes 18-25/26");

    // Caught up already.
    let (status, headers, _) = get(&home, "/crawl/logfile", Some("bytes=26-")).await;
    assert_eq!(status, StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(headers[header::CONTENT_RANGE], "bytes */26");

    let (status, _, body) = get(&home, "/crawl/milestones", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, "v=0.34.1:name=mat:type=orb\n");
}

#[tokio::test]
async fn walks_the_morgue_tree() {
    let home = playground();

    let (status, headers, body) = get(&home, "/crawl/morgue/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/html; charset=utf-8");
    assert!(body.contains("<a href=\"mat/\">mat/</a>"));

    // A directory without the trailing slash redirects: the listing's links are
    // relative and would otherwise resolve one level too high.
    let (status, headers, _) = get(&home, "/crawl/morgue/mat", None).await;
    assert_eq!(status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(headers[header::LOCATION], "/crawl/morgue/mat/");

    let (status, _, body) = get(&home, "/crawl/morgue/mat/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("morgue-mat-20260810-120000.txt"));

    let (status, headers, body) = get(
        &home,
        "/crawl/morgue/mat/morgue-mat-20260810-120000.txt",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/plain; charset=utf-8");
    assert_eq!(body, "dungeon crawl stone soup version 0.34.1\n");
}

#[tokio::test]
async fn refuses_everything_outside_the_published_files() {
    let home = playground();

    // Traversal out of the morgue tree, encoded or not.
    for uri in [
        "/crawl/morgue/../logfile",
        "/crawl/morgue/%2e%2e/mat.rc",
        "/crawl/morgue/mat/../../mat.rc",
    ] {
        let (status, _, _) = get(&home, uri, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} must not resolve");
    }

    // A symlink planted inside the morgue tree cannot smuggle the rc file out.
    let escape = home.path().join(".crawl/morgue/escape.txt");
    std::os::unix::fs::symlink(home.path().join(".crawl/mat.rc"), &escape).expect("symlink");
    let (status, _, _) = get(&home, "/crawl/morgue/escape.txt", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Neighbours of the published files are not routes at all.
    let (status, _, _) = get(&home, "/crawl/mat.rc", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn missing_logs_are_not_found_and_the_index_documents_the_layout() {
    let home = tempfile::tempdir().expect("tempdir");

    // A playground that has not finished a game yet.
    let (status, _, _) = get(&home, "/crawl/logfile", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, headers, body) = get(&home, "/crawl/", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/html; charset=utf-8");
    assert!(body.contains("/crawl/logfile"));
    assert!(body.contains("/crawl/milestones"));
    assert!(body.contains("/crawl/morgue/"));
}
