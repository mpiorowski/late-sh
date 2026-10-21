use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use late_core::models::article_body::{ReaderLine, ReaderSpan};
use reqwest::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{
    BODY_CONTINUES, BODY_MAX_LINES, BODY_MAX_REDIRECTS, BodyFetchError, BodyFetchFailure,
    cap_lines, client_with_resolver, fetch_page, lines_from_html, readable_body,
};

const ARTICLE: &str = r#"<!doctype html>
<html>
<head>
  <title>Why LISTEN/NOTIFY stops scaling</title>
  <script>window.tracker = { id: 42 };</script>
  <style>body { color: red; }</style>
</head>
<body>
  <nav class="site-nav"><a href="/">Home</a> <a href="/about">About</a> <a href="/rss">RSS</a></nav>
  <main>
    <article>
      <p>Postgres ships a pub/sub channel that most teams reach for the first time they
         need two processes to agree on something, and for a long while it is exactly
         right. This piece walks through <a href="/docs/notify">the notify docs</a> and
         <a href="https://bench.example/listen">a benchmark</a> to show where it bends.</p>
      <h2>The setup</h2>
      <p>Every replica holds one connection that listens on every channel the domains
         subscribe to, and a worker per domain re-reads its snapshot whenever a notify
         lands. A burst of writes collapses into a single read, which is the whole trick.</p>
      <blockquote><p>One connection per replica is enough for a very long time.</p></blockquote>
      <pre>LISTEN articles_changed;
	NOTIFY articles_changed;</pre>
      <ul>
        <li>Payloads cap at eight thousand bytes.</li>
        <li>A lost notify is caught by the <em>resync</em> after reconnecting.</li>
      </ul>
      <hr>
      <p>So measure before you shard the listener, and keep the payload empty, because
         a re-read is always the committed row and a payload can arrive out of order.</p>
      <img src="/chart.png" alt="a chart">
    </article>
  </main>
  <footer>Copyright 2026, nobody reads this.</footer>
  <script>track();</script>
</body>
</html>"#;

fn text(text: &str) -> ReaderSpan {
    ReaderSpan::Text(text.to_string())
}

fn link(text: &str, href: &str) -> ReaderSpan {
    ReaderSpan::Link {
        text: text.to_string(),
        href: href.to_string(),
    }
}

fn base() -> Url {
    Url::parse("https://blog.example/posts/listen-notify").expect("base url")
}

/// The article walks into exactly these lines: navigation, scripts, the
/// footer, and the image gone; whitespace collapsed; relative links made
/// absolute against the page; the `pre` kept verbatim.
#[test]
fn an_article_walks_into_reader_lines() {
    let lines = lines_from_html(ARTICLE, &base()).expect("readable");

    assert_eq!(
        lines,
        vec![
            ReaderLine::Paragraph {
                spans: vec![
                    text(
                        "Postgres ships a pub/sub channel that most teams reach for the first \
                         time they need two processes to agree on something, and for a long \
                         while it is exactly right. This piece walks through "
                    ),
                    link("the notify docs", "https://blog.example/docs/notify"),
                    text(" and "),
                    link("a benchmark", "https://bench.example/listen"),
                    text(" to show where it bends."),
                ],
            },
            ReaderLine::Heading {
                level: 2,
                text: "The setup".to_string(),
            },
            ReaderLine::Paragraph {
                spans: vec![text(
                    "Every replica holds one connection that listens on every channel the \
                     domains subscribe to, and a worker per domain re-reads its snapshot \
                     whenever a notify lands. A burst of writes collapses into a single read, \
                     which is the whole trick."
                )],
            },
            ReaderLine::Quote {
                spans: vec![text(
                    "One connection per replica is enough for a very long time."
                )],
            },
            ReaderLine::Code {
                lines: vec![
                    "LISTEN articles_changed;".to_string(),
                    "    NOTIFY articles_changed;".to_string(),
                ],
            },
            ReaderLine::ListItem {
                spans: vec![text("Payloads cap at eight thousand bytes.")],
            },
            ReaderLine::ListItem {
                spans: vec![text(
                    "A lost notify is caught by the resync after reconnecting."
                )],
            },
            ReaderLine::Rule,
            ReaderLine::Paragraph {
                spans: vec![text(
                    "So measure before you shard the listener, and keep the payload empty, \
                     because a re-read is always the committed row and a payload can arrive \
                     out of order."
                )],
            },
        ]
    );
}

/// Past the line cap the body is cut and closed with the continuation line.
/// The paragraphs are short so the line cap, not the text cap, is the one hit.
#[test]
fn a_long_article_is_cut_at_the_line_cap() {
    let paragraphs: String = (0..BODY_MAX_LINES + 50)
        .map(|n| format!("<p>Paragraph {n} is a short line of body text, with commas.</p>"))
        .collect();
    let html = format!("<html><body><article>{paragraphs}</article></body></html>");

    let lines = lines_from_html(&html, &base()).expect("readable");

    assert_eq!(lines.len(), BODY_MAX_LINES + 1);
    assert_eq!(
        lines.last(),
        Some(&ReaderLine::Paragraph {
            spans: vec![text(BODY_CONTINUES)],
        })
    );
}

/// A page with no words in its readable part is `Empty`, not a ready body
/// with nothing in it.
#[test]
fn a_page_without_words_is_empty() {
    let html = "<html><body><article><img src=\"/a.png\"><hr></article></body></html>";

    let error = readable_body(html, &base()).expect_err("nothing to read");

    assert_eq!(error.failure, BodyFetchFailure::Empty);
}

/// A code block's lines count one by one against the line cap, so a `pre`
/// of mostly blank lines (no text to weigh against the byte cap) is cut
/// inside instead of carrying the body past both caps.
#[test]
fn a_code_block_counts_line_by_line_and_is_cut_inside() {
    let intro = ReaderLine::Paragraph {
        spans: vec![text("Intro.")],
    };
    let code: Vec<String> = (0..BODY_MAX_LINES * 2)
        .map(|n| {
            if n % 2 == 0 {
                "x".to_string()
            } else {
                String::new()
            }
        })
        .collect();

    let capped = cap_lines(vec![
        intro.clone(),
        ReaderLine::Code {
            lines: code.clone(),
        },
        ReaderLine::Paragraph {
            spans: vec![text("After the code.")],
        },
    ]);

    assert_eq!(
        capped,
        vec![
            intro,
            ReaderLine::Code {
                lines: code[..BODY_MAX_LINES - 1].to_vec(),
            },
            ReaderLine::Paragraph {
                spans: vec![text(BODY_CONTINUES)],
            },
        ]
    );
}

/// Resolves every name to this machine, so a test's pages are reachable by
/// name while the client's own redirect policy still sees every hop.
struct LoopbackResolver;

impl reqwest::dns::Resolve for LoopbackResolver {
    fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        Box::pin(async {
            let loopback = SocketAddr::from(([127, 0, 0, 1], 0));
            Ok(Box::new(std::iter::once(loopback)) as reqwest::dns::Addrs)
        })
    }
}

/// A loopback server answering each request with `answer(path, port)`.
/// Returns its port and every path it was asked for, in order.
async fn serve(answer: fn(&str, u16) -> Vec<u8>) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind page server");
    let port = listener.local_addr().expect("page server addr").port();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&asked);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                match socket.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            let path = String::from_utf8_lossy(&request)
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            log.lock().expect("request log").push(path.clone());
            let _ = socket.write_all(&answer(&path, port)).await;
        }
    });
    (port, asked)
}

fn redirect(location: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 302 Found\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
    )
    .into_bytes()
}

fn page(content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

async fn fetch_error(url: &str) -> BodyFetchError {
    match fetch_page(&client_with_resolver(LoopbackResolver), url).await {
        Ok(page) => panic!("expected the fetch to fail, it read {}", page.final_url),
        Err(error) => error,
    }
}

const PAGE_HTML: &[u8] = b"<html><body><article><p>Read me.</p></article></body></html>";

/// A redirect to a host name is followed, and the page is read where the
/// chain ended.
#[tokio::test]
async fn a_redirect_to_a_host_name_is_followed() {
    let (port, asked) = serve(|path, port| match path {
        "/start" => redirect(&format!("http://elsewhere.test:{port}/page")),
        _ => page("text/html", PAGE_HTML),
    })
    .await;

    let fetched = match fetch_page(
        &client_with_resolver(LoopbackResolver),
        &format!("http://late.test:{port}/start"),
    )
    .await
    {
        Ok(fetched) => fetched,
        Err(error) => panic!("the redirect is followed: {error:?}"),
    };

    assert_eq!(
        fetched.final_url.as_str(),
        format!("http://elsewhere.test:{port}/page")
    );
    assert_eq!(*asked.lock().expect("request log"), ["/start", "/page"]);
}

/// A redirect to a literal private address is refused before any
/// connection. The resolver never sees a literal, so the client's redirect
/// policy is the only guard there.
#[tokio::test]
async fn a_redirect_to_a_private_literal_is_refused_without_connecting() {
    let (port, asked) = serve(|path, port| match path {
        "/start" => redirect(&format!("http://127.0.0.1:{port}/inside")),
        _ => page("text/html", PAGE_HTML),
    })
    .await;

    let error = fetch_error(&format!("http://late.test:{port}/start")).await;

    assert_eq!(error.failure, BodyFetchFailure::Fetch);
    assert_eq!(
        *asked.lock().expect("request log"),
        ["/start"],
        "the private address is never asked"
    );
}

/// A chain of redirects is followed [`BODY_MAX_REDIRECTS`] times, and the
/// one after that is refused.
#[tokio::test]
async fn a_redirect_chain_stops_after_the_limit() {
    let (port, asked) = serve(|path, port| {
        let hop: usize = path
            .trim_start_matches("/hop/")
            .parse()
            .expect("a hop number");
        redirect(&format!("http://late.test:{port}/hop/{}", hop + 1))
    })
    .await;

    let error = fetch_error(&format!("http://late.test:{port}/hop/0")).await;

    assert_eq!(error.failure, BodyFetchFailure::Fetch);
    // The shared URL, then one request per redirect followed.
    assert_eq!(
        asked.lock().expect("request log").len(),
        BODY_MAX_REDIRECTS + 1
    );
}

/// A page that is not UTF-8 gets no body rather than garbled text.
#[tokio::test]
async fn a_page_that_is_not_utf8_is_refused() {
    let (port, _) = serve(|_, _| {
        page(
            "text/html; charset=iso-8859-1",
            b"<html><body><article><p>caf\xe9 cr\xe8me</p></article></body></html>",
        )
    })
    .await;

    let error = fetch_error(&format!("http://late.test:{port}/page")).await;

    assert_eq!(error.failure, BodyFetchFailure::Encoding);
}
