use std::net::IpAddr;

use late_core::models::article_body::{ReaderLine, ReaderSpan};
use reqwest::Url;

use super::{
    BODY_CONTINUES, BODY_MAX_LINES, BodyFetchFailure, is_public_ip, lines_from_html, readable_body,
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

/// The fetch never connects inside the network.
#[test]
fn only_public_addresses_are_fetchable() {
    for private in [
        "127.0.0.1",
        "10.1.2.3",
        "172.16.0.9",
        "192.168.1.1",
        "169.254.169.254",
        "100.64.0.1",
        "0.0.0.0",
        "::1",
        "fd00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
    ] {
        let ip: IpAddr = private.parse().expect("ip");
        assert!(!is_public_ip(ip), "{private} must not be fetchable");
    }
    for public in ["93.184.216.34", "2606:4700::6810:85e5"] {
        let ip: IpAddr = public.parse().expect("ip");
        assert!(is_public_ip(ip), "{public} is public");
    }
}
