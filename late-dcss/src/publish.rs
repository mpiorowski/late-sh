// Read-only HTTP publishing of the crawl server files, so late.sh can join the
// public DCSS ecosystem. Every public server (CAO, CPO, ...) exposes its shared
// `logfile`, `milestones`, and `morgue/` dumps over plain HTTP; the ecosystem
// tooling (dcss-stats.com, Sequell) fetches those URLs to build per-player
// stats pages and to link a morgue per game.
//
// The bytes served here are the same bytes `stats.rs` tails into our own
// ingestion, so the public fetcher and our boards validate each other.
//
// Shape:
// - Everything is served natively under `/crawl/...`, so the ingress needs no
//   rewrite annotation: a Prefix path rule on the apex host out-ranks
//   late-web's `/` catch-all by nginx longest-match (infra/service-dcss.tf).
// - `GET /crawl/` documents the layout for whoever opens it in a browser.
// - `GET /crawl/logfile`, `GET /crawl/milestones` serve the two shared xlog
//   files, with `Range` support so a fetcher can pull only what was appended
//   since its last visit (the whole point of the ecosystem's incremental
//   fetch).
// - `GET /crawl/morgue/...` walks the morgue tree: directories render an
//   nginx-autoindex-shaped listing, files stream.
//
// Public by design: playnames are arcade handles, which are already public.
// The one thing to get right is path handling, so a morgue request path is
// rebuilt from sanitized components (`..` is unrepresentable, dotfiles are
// refused) and the resolved path is re-checked against the morgue root after
// canonicalization, which is what catches a symlink pointing out of the tree.
//
// Only ever reads. Nothing here can write to the playground, and no route
// reaches outside `$HOME/.crawl` (saves, rc files, and macro dirs live beside
// the published files and stay unpublished).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Context;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use chrono::{DateTime, Utc};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Everything crawl writes lives under this subdirectory of the child HOME
/// (`config.data_dir`), the same root `stats.rs` tails.
const CRAWL_SUBDIR: &str = ".crawl";

/// Morgue dumps: one file per finished game, written by crawl at game end.
const MORGUE_SUBDIR: &str = "morgue";

const TEXT_PLAIN: &str = "text/plain; charset=utf-8";

/// Chunk size for streamed bodies. Only a buffer hint; `Body::from_stream`
/// backpressures on the socket, so a slow fetcher never buffers a whole file.
const STREAM_CHUNK: usize = 64 * 1024;

/// The shared xlog files published to the ecosystem. Closed on purpose: crawl
/// suffixes non-standard game types (`logfile-sprint`, ...) and those stay
/// unpublished, exactly like the ingestion tailer's file list.
#[derive(Clone, Copy)]
enum PublishedLog {
    Logfile,
    Milestones,
}

impl PublishedLog {
    fn file_name(self) -> &'static str {
        match self {
            Self::Logfile => "logfile",
            Self::Milestones => "milestones",
        }
    }
}

#[derive(Clone)]
struct Publish {
    /// `$HOME/.crawl` on the PVC.
    crawl_dir: PathBuf,
}

/// Bind the publish listener and serve until the process exits. A bind failure
/// is fatal for the pod (main.rs propagates it): a misconfigured port would
/// otherwise look healthy while the ecosystem's fetcher 502s.
pub(crate) async fn serve(listen_addr: &str, port: u16, data_dir: &str) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind((listen_addr, port))
        .await
        .with_context(|| format!("binding publish listener on {listen_addr}:{port}"))?;
    tracing::info!(%listen_addr, port, "crawl publish listener bound");
    axum::serve(listener, router(data_dir))
        .await
        .context("publish http server failed")
}

fn router(data_dir: &str) -> Router {
    let crawl_dir = Path::new(data_dir).join(CRAWL_SUBDIR);
    Router::new()
        .route("/crawl", get(index))
        .route("/crawl/", get(index))
        .route("/crawl/logfile", get(logfile))
        .route("/crawl/milestones", get(milestones))
        .route("/crawl/morgue", get(morgue_root))
        .route("/crawl/morgue/", get(morgue_root))
        .route("/crawl/morgue/{*path}", get(morgue_path))
        .with_state(Publish { crawl_dir })
}

const INDEX_HTML: &str = r#"<html>
<head><title>late.sh crawl server files</title></head>
<body>
<h1>late.sh crawl server files</h1>
<p>Dungeon Crawl Stone Soup logs from the late.sh door game, served read-only
for the public tooling (dcss-stats, Sequell). Playnames are late.sh arcade
handles. Range requests are supported on the log files.</p>
<pre>
<a href="/crawl/logfile">logfile</a>       every finished game, one xlog line each
<a href="/crawl/milestones">milestones</a>    live mid-run milestones, one xlog line each
<a href="/crawl/morgue/">morgue/</a>       per-game dump files
</pre>
</body>
</html>
"#;

async fn index() -> Response {
    Html(INDEX_HTML).into_response()
}

async fn logfile(State(publish): State<Publish>, headers: HeaderMap) -> Response {
    serve_log(&publish, PublishedLog::Logfile, &headers).await
}

async fn milestones(State(publish): State<Publish>, headers: HeaderMap) -> Response {
    serve_log(&publish, PublishedLog::Milestones, &headers).await
}

async fn morgue_root(State(publish): State<Publish>, uri: Uri, headers: HeaderMap) -> Response {
    serve_morgue(&publish, "", uri.path(), &headers).await
}

async fn morgue_path(
    State(publish): State<Publish>,
    UrlPath(rel): UrlPath<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response {
    serve_morgue(&publish, &rel, uri.path(), &headers).await
}

async fn serve_log(publish: &Publish, log: PublishedLog, headers: &HeaderMap) -> Response {
    let path = publish.crawl_dir.join(log.file_name());
    // Missing is normal until the first game finishes: crawl creates both files
    // on demand.
    let Ok(meta) = tokio::fs::metadata(&path).await else {
        return not_found();
    };
    serve_file(&path, &meta, TEXT_PLAIN, headers).await
}

async fn serve_morgue(
    publish: &Publish,
    rel: &str,
    request_path: &str,
    headers: &HeaderMap,
) -> Response {
    let root = publish.crawl_dir.join(MORGUE_SUBDIR);
    let requested = match sanitize_rel(rel) {
        Ok(sanitized) => root.join(sanitized),
        Err(reject) => {
            tracing::debug!(rel, ?reject, "refused morgue path");
            return not_found();
        }
    };

    // `sanitize_rel` already makes `..` unrepresentable, so this catches only
    // the other escape: a symlink inside the morgue tree pointing out of it.
    // Both canonicalizations also serve as the existence check.
    let (Ok(real), Ok(real_root)) = (
        tokio::fs::canonicalize(&requested).await,
        tokio::fs::canonicalize(&root).await,
    ) else {
        return not_found();
    };
    if !real.starts_with(&real_root) {
        tracing::warn!(path = %requested.display(), "morgue path escaped the morgue root");
        return not_found();
    }

    let Ok(meta) = tokio::fs::metadata(&real).await else {
        return not_found();
    };
    if meta.is_dir() {
        // A listing's links are relative, so they only resolve correctly from a
        // path that ends in a slash. Redirect rather than serve a page whose
        // every link points one level too high.
        if !request_path.ends_with('/') {
            return redirect_to_dir(request_path);
        }
        serve_listing(&real, rel).await
    } else {
        serve_file(&real, &meta, content_type_for(&real), headers).await
    }
}

/// Stream a file, honoring a single-range `Range` header. The ecosystem's
/// fetchers pull the log files incrementally (`bytes=<last size>-`), which is
/// the reason this is not a plain whole-file read.
async fn serve_file(
    path: &Path,
    meta: &std::fs::Metadata,
    content_type: &str,
    headers: &HeaderMap,
) -> Response {
    let len = meta.len();
    let requested = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map_or(RangeSpec::Full, |value| parse_range(value, len));
    let (status, start, length) = match requested {
        RangeSpec::Full => (StatusCode::OK, 0, len),
        RangeSpec::Partial { start, end } => (StatusCode::PARTIAL_CONTENT, start, end - start + 1),
        RangeSpec::Unsatisfiable => return range_not_satisfiable(len),
    };

    let mut file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(e) => {
            tracing::warn!(error = ?e, path = %path.display(), "failed to open published file");
            return not_found();
        }
    };
    match file.seek(std::io::SeekFrom::Start(start)).await {
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(error = ?e, path = %path.display(), start, "failed to seek published file");
            return internal_error();
        }
    }

    let mut out = HeaderMap::new();
    out.insert(header::CONTENT_TYPE, header_value(content_type));
    out.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    if let Ok(modified) = meta.modified() {
        out.insert(header::LAST_MODIFIED, header_value(&http_date(modified)));
    }
    if let RangeSpec::Partial { start, end } = requested {
        out.insert(
            header::CONTENT_RANGE,
            header_value(&format!("bytes {start}-{end}/{len}")),
        );
    }

    let stream = tokio_util::io::ReaderStream::with_capacity(file.take(length), STREAM_CHUNK);
    (status, out, Body::from_stream(stream)).into_response()
}

async fn serve_listing(dir: &Path, rel: &str) -> Response {
    let mut read_dir = match tokio::fs::read_dir(dir).await {
        Ok(read_dir) => read_dir,
        Err(e) => {
            tracing::warn!(error = ?e, path = %dir.display(), "failed to read morgue directory");
            return not_found();
        }
    };

    let mut entries = Vec::new();
    loop {
        match read_dir.next_entry().await {
            Ok(Some(entry)) => {
                let name = entry.file_name().to_string_lossy().into_owned();
                // Dotfiles are not part of the published surface, same rule the
                // request path sanitizer applies.
                if name.starts_with('.') {
                    continue;
                }
                let Ok(meta) = entry.metadata().await else {
                    continue;
                };
                entries.push(ListingEntry {
                    name,
                    is_dir: meta.is_dir(),
                    size: meta.len(),
                    modified: meta.modified().ok().map(DateTime::<Utc>::from),
                });
            }
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = ?e, path = %dir.display(), "failed to walk morgue directory");
                break;
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    let title = format!("/{MORGUE_SUBDIR}/{rel}");
    Html(render_listing(&title, &entries)).into_response()
}

/// Why a request path under `morgue/` was refused. Every arm is a 404: the
/// caller learns nothing about the tree it tried to reach.
#[derive(Debug, PartialEq, Eq)]
enum PathReject {
    /// A `.` or `..` component.
    Traversal,
    /// A dotfile component; nothing hidden is published.
    Hidden,
    /// A component the filesystem should never see from a URL.
    Invalid,
}

/// Rebuild a URL-supplied relative path from sanitized components. The result
/// contains only plain names, so joining it onto the morgue root cannot leave
/// that root. Empty components (a trailing or doubled slash) are dropped.
fn sanitize_rel(rel: &str) -> Result<PathBuf, PathReject> {
    let mut out = PathBuf::new();
    for component in rel.split('/') {
        match component {
            "" => continue,
            "." | ".." => return Err(PathReject::Traversal),
            name if name.starts_with('.') => return Err(PathReject::Hidden),
            name if name.contains('\0') || name.contains('\\') => return Err(PathReject::Invalid),
            name => out.push(name),
        }
    }
    Ok(out)
}

/// What a `Range` header asks for, resolved against the file's length. A header
/// we do not implement (multi-range, malformed, a non-`bytes` unit) resolves to
/// `Full`: serving the whole file is always a legal answer to a range request.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum RangeSpec {
    Full,
    /// Inclusive byte range, already clamped to the file.
    Partial {
        start: u64,
        end: u64,
    },
    Unsatisfiable,
}

fn parse_range(value: &str, len: u64) -> RangeSpec {
    let Some(spec) = value.trim().strip_prefix("bytes=") else {
        return RangeSpec::Full;
    };
    if spec.contains(',') {
        return RangeSpec::Full;
    }
    let Some((first, last)) = spec.split_once('-') else {
        return RangeSpec::Full;
    };
    let (first, last) = (first.trim(), last.trim());
    if len == 0 {
        // There is no byte to hand back out of an empty file, whatever was
        // asked for. The file exists though, so this is a 416, not a 404.
        return RangeSpec::Unsatisfiable;
    }

    match (first.is_empty(), last.is_empty()) {
        // `bytes=-N`: the final N bytes.
        (true, false) => match last.parse::<u64>() {
            Ok(0) => RangeSpec::Unsatisfiable,
            Ok(suffix) => RangeSpec::Partial {
                start: len.saturating_sub(suffix),
                end: len - 1,
            },
            Err(_) => RangeSpec::Full,
        },
        // `bytes=N-`: from N to the end. This is the incremental-fetch case.
        (false, true) => match first.parse::<u64>() {
            Ok(start) if start < len => RangeSpec::Partial {
                start,
                end: len - 1,
            },
            Ok(_) => RangeSpec::Unsatisfiable,
            Err(_) => RangeSpec::Full,
        },
        // `bytes=N-M`: an explicit window, clamped to the file.
        (false, false) => match (first.parse::<u64>(), last.parse::<u64>()) {
            (Ok(start), Ok(end)) if start < len && start <= end => RangeSpec::Partial {
                start,
                end: end.min(len - 1),
            },
            (Ok(_), Ok(_)) => RangeSpec::Unsatisfiable,
            _ => RangeSpec::Full,
        },
        (true, true) => RangeSpec::Unsatisfiable,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ListingEntry {
    name: String,
    is_dir: bool,
    size: u64,
    modified: Option<DateTime<Utc>>,
}

/// Render a directory listing in nginx's autoindex shape (one `<a>` per line
/// inside a `<pre>`, then date and size columns). Scrapers in this ecosystem
/// are written against the public servers' nginx output, so matching it is the
/// compatible thing to serve.
fn render_listing(title: &str, entries: &[ListingEntry]) -> String {
    let mut out = String::new();
    out.push_str("<html>\n<head><title>Index of ");
    out.push_str(&escape_html(title));
    out.push_str("</title></head>\n<body>\n<h1>Index of ");
    out.push_str(&escape_html(title));
    out.push_str("</h1><hr><pre><a href=\"../\">../</a>\n");
    for entry in entries {
        let suffix = if entry.is_dir { "/" } else { "" };
        let name = format!("{}{}", escape_html(&entry.name), suffix);
        let date = match entry.modified {
            Some(modified) => modified.format("%d-%b-%Y %H:%M").to_string(),
            None => "-".to_string(),
        };
        let size = if entry.is_dir {
            "-".to_string()
        } else {
            entry.size.to_string()
        };
        out.push_str(&format!(
            "<a href=\"{name}\">{name}</a>{:pad$}{date:>20}{size:>20}\n",
            "",
            pad = 62usize.saturating_sub(name.chars().count()),
        ));
    }
    out.push_str("</pre><hr></body>\n</html>\n");
    out
}

/// Escape the four characters that could break out of the listing markup.
/// Names come off the playground (crawl builds them from arcade handles, which
/// are already sanitized), so this is belt-and-braces for a hand-made file.
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn content_type_for(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        // crawl writes morgues as plain text; browsers should show them.
        Some("txt") | Some("lst") => TEXT_PLAIN,
        _ => "application/octet-stream",
    }
}

/// RFC 7231 IMF-fixdate, the only format `Last-Modified` may carry.
fn http_date(time: SystemTime) -> String {
    DateTime::<Utc>::from(time)
        .format("%a, %d %b %Y %H:%M:%S GMT")
        .to_string()
}

/// Every value we build here is ASCII by construction (media types, HTTP
/// dates, byte counts), so a rejected header value is a bug, not an input.
fn header_value(value: &str) -> HeaderValue {
    HeaderValue::from_str(value).expect("header value is ascii by construction")
}

/// nginx answers a directory URL missing its trailing slash with a 301, and the
/// scrapers in this ecosystem are written against the public servers' nginx.
/// Hand-built rather than axum's `Redirect::permanent`, which is a 308: same
/// meaning to a modern client, but 301 is the one every old script understands.
fn redirect_to_dir(request_path: &str) -> Response {
    let mut out = HeaderMap::new();
    out.insert(header::LOCATION, header_value(&format!("{request_path}/")));
    (StatusCode::MOVED_PERMANENTLY, out).into_response()
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found\n").into_response()
}

fn internal_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "internal error\n").into_response()
}

fn range_not_satisfiable(len: u64) -> Response {
    let mut out = HeaderMap::new();
    out.insert(
        header::CONTENT_RANGE,
        header_value(&format!("bytes */{len}")),
    );
    (StatusCode::RANGE_NOT_SATISFIABLE, out).into_response()
}

#[cfg(test)]
#[path = "publish_test.rs"]
mod publish_test;
