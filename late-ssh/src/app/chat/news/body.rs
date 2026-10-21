//! The News reader's body: fetch a shared page and walk its readable part
//! into [`ReaderLine`]s. Pure work that returns data or a tagged failure;
//! `svc.rs` owns the timing budget, the store, the log, and the metric.
//!
//! The fetch goes out to whatever URL a user shared and its text is shown
//! to everyone, so it only ever connects to public addresses, under the
//! house rule in `app/common/public_net.rs`: the resolver drops non-public
//! answers, a literal IP host is checked before the request and on every
//! redirect, and no proxy is used (a proxy would resolve the name past the
//! resolver).

use std::time::Duration;

use dom_query::{Document, NodeRef};
use late_core::models::article_body::{ReaderLine, ReaderSpan};
use late_core::telemetry::TracedExt;
use reqwest::Url;

use crate::app::common::public_net::{PublicOnlyResolver, is_public_http_url};

const BODY_FETCH_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_MAX_BYTES: usize = 2_000_000;
pub(crate) const BODY_MAX_REDIRECTS: usize = 5;
const BODY_USER_AGENT: &str = "late.sh (https://late.sh)";
/// Past either cap the body is cut and closed with [`BODY_CONTINUES`]. A
/// code block's lines count one by one, so no block runs the body past them.
pub(crate) const BODY_MAX_LINES: usize = 2_000;
pub(crate) const BODY_MAX_TEXT_BYTES: usize = 200_000;
pub(crate) const BODY_CONTINUES: &str = "… continues on the web";

/// Why a story got no readable body. Labels the metric and the log; never
/// stored, since the card and the reader treat every reason alike.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyFetchFailure {
    /// The request failed: refused, unreachable, a non-2xx answer, a private
    /// address, too many redirects.
    Fetch,
    /// The answer was not an HTML page.
    NotHtml,
    /// The page is not UTF-8. Only UTF-8 is read: a page in another
    /// encoding gets no body rather than garbled text.
    Encoding,
    /// The page is bigger than [`BODY_MAX_BYTES`].
    TooLarge,
    /// The page did not arrive inside [`BODY_FETCH_TIMEOUT`], or the share's
    /// processing budget ran out first.
    Timeout,
    /// Readability found no article in the page.
    Extract,
    /// The article had no words in it.
    Empty,
}

/// A failure with what caused it, for the log.
#[derive(Debug)]
pub struct BodyFetchError {
    pub failure: BodyFetchFailure,
    pub source: anyhow::Error,
}

impl BodyFetchError {
    pub(crate) fn new(failure: BodyFetchFailure, source: impl Into<anyhow::Error>) -> Self {
        Self {
            failure,
            source: source.into(),
        }
    }
}

/// A fetched HTML page and where the fetch ended after redirects.
pub struct FetchedPage {
    pub final_url: Url,
    pub html: String,
}

/// A readable body, ready to store.
#[derive(Debug, PartialEq, Eq)]
pub struct ReadableBody {
    pub word_count: u32,
    pub lines: Vec<ReaderLine>,
}

/// The client every body fetch goes through.
pub fn http_client() -> reqwest::Client {
    client_with_resolver(PublicOnlyResolver)
}

/// The body fetch's client over `resolver`: production resolves through
/// [`PublicOnlyResolver`], and a test hands in one that reaches its own
/// loopback server, under the same redirect policy.
pub(crate) fn client_with_resolver(
    resolver: impl reqwest::dns::Resolve + 'static,
) -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(BODY_USER_AGENT)
        .timeout(BODY_FETCH_TIMEOUT)
        .no_proxy()
        .dns_resolver(resolver)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // `previous` starts with the shared URL itself, which is not a
            // redirect.
            if attempt.previous().len() > BODY_MAX_REDIRECTS {
                attempt.error("too many redirects")
            } else if !is_public_http_url(attempt.url()) {
                attempt.error("redirect to a non-public address")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("the news body http client always builds")
}

/// GET the page, at most [`BODY_MAX_BYTES`] of it, and only if it is HTML.
pub async fn fetch_page(
    client: &reqwest::Client,
    url: &str,
) -> Result<FetchedPage, BodyFetchError> {
    let url = match Url::parse(url) {
        Ok(url) => url,
        Err(e) => return Err(BodyFetchError::new(BodyFetchFailure::Fetch, e)),
    };
    if !is_public_http_url(&url) {
        return Err(BodyFetchError::new(
            BodyFetchFailure::Fetch,
            anyhow::anyhow!("not a public http(s) url"),
        ));
    }

    let mut res = match client.get(url).send_traced().await {
        Ok(res) => res,
        Err(e) => return Err(request_error(e)),
    };
    if !res.status().is_success() {
        return Err(BodyFetchError::new(
            BodyFetchFailure::Fetch,
            anyhow::anyhow!("page answered {}", res.status()),
        ));
    }
    let content_type = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_html_content_type(&content_type) {
        return Err(BodyFetchError::new(
            BodyFetchFailure::NotHtml,
            anyhow::anyhow!("content type {content_type:?}"),
        ));
    }
    if let Some(length) = res.content_length()
        && length > BODY_MAX_BYTES as u64
    {
        return Err(BodyFetchError::new(
            BodyFetchFailure::TooLarge,
            anyhow::anyhow!("content-length {length}"),
        ));
    }

    let final_url = res.url().clone();
    let mut bytes = Vec::new();
    loop {
        match res.chunk().await {
            Ok(Some(chunk)) => {
                if bytes.len() + chunk.len() > BODY_MAX_BYTES {
                    return Err(BodyFetchError::new(
                        BodyFetchFailure::TooLarge,
                        anyhow::anyhow!("body passed {BODY_MAX_BYTES} bytes while streaming"),
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => return Err(request_error(e)),
        }
    }

    match String::from_utf8(bytes) {
        Ok(html) => Ok(FetchedPage { final_url, html }),
        Err(e) => Err(BodyFetchError::new(BodyFetchFailure::Encoding, e)),
    }
}

/// The page's readable part as reader lines, capped, with its word count.
/// CPU-bound: run it off the async workers.
pub fn readable_body(html: &str, base_url: &Url) -> Result<ReadableBody, BodyFetchError> {
    let lines = lines_from_html(html, base_url)?;
    let word_count = word_count(&lines);
    if word_count == 0 {
        return Err(BodyFetchError::new(
            BodyFetchFailure::Empty,
            anyhow::anyhow!("readable content had no words"),
        ));
    }
    Ok(ReadableBody { word_count, lines })
}

/// Readability over the page, then a walk of the article it found:
/// headings, paragraphs, quotes, code, list items, and rules, with links
/// resolved against `base_url`. Images, figures, tables, scripts, and
/// navigation are dropped.
pub fn lines_from_html(html: &str, base_url: &Url) -> Result<Vec<ReaderLine>, BodyFetchError> {
    let mut readability = match dom_smoothie::Readability::new(html, Some(base_url.as_str()), None)
    {
        Ok(readability) => readability,
        Err(e) => return Err(BodyFetchError::new(BodyFetchFailure::Extract, e)),
    };
    let article = match readability.parse() {
        Ok(article) => article,
        Err(e) => return Err(BodyFetchError::new(BodyFetchFailure::Extract, e)),
    };

    let content = Document::from(article.content.as_ref());
    let Some(body) = content.body() else {
        return Ok(Vec::new());
    };
    let mut walker = Walker {
        base_url,
        lines: Vec::new(),
        loose: SpanBuilder::default(),
    };
    walker.walk_block(&body, Context::Body);
    walker.flush(Context::Body);
    Ok(cap_lines(walker.lines))
}

/// Words across every line's text.
pub fn word_count(lines: &[ReaderLine]) -> u32 {
    let words = lines
        .iter()
        .map(|line| match line {
            ReaderLine::Heading { text, .. } => text.split_whitespace().count(),
            ReaderLine::Paragraph { spans }
            | ReaderLine::Quote { spans }
            | ReaderLine::ListItem { spans } => spans
                .iter()
                .map(|span| match span {
                    ReaderSpan::Text(text) | ReaderSpan::Link { text, .. } => {
                        text.split_whitespace().count()
                    }
                })
                .sum(),
            ReaderLine::Code { lines } => lines.iter().map(|l| l.split_whitespace().count()).sum(),
            ReaderLine::Rule => 0,
        })
        .sum::<usize>();
    u32::try_from(words).unwrap_or(u32::MAX)
}

/// Cut a body at [`BODY_MAX_LINES`] lines or [`BODY_MAX_TEXT_BYTES`] of
/// text, whichever comes first, and close it with [`BODY_CONTINUES`]. Each
/// line of a code block counts on its own and a block is cut inside, so the
/// reader's row count stays bounded whatever the page puts in a `pre`.
fn cap_lines(lines: Vec<ReaderLine>) -> Vec<ReaderLine> {
    let mut lines_left = BODY_MAX_LINES;
    let mut bytes_left = BODY_MAX_TEXT_BYTES;
    let mut kept = Vec::new();
    for line in lines {
        match line {
            ReaderLine::Code { lines: code } => {
                let mut fitting = Vec::new();
                let mut cut = false;
                for code_line in code {
                    if lines_left == 0 || code_line.len() > bytes_left {
                        cut = true;
                        break;
                    }
                    lines_left -= 1;
                    bytes_left -= code_line.len();
                    fitting.push(code_line);
                }
                if !fitting.is_empty() {
                    kept.push(ReaderLine::Code { lines: fitting });
                }
                if cut {
                    kept.push(continues_line());
                    return kept;
                }
            }
            line => {
                let bytes = line_text_bytes(&line);
                if lines_left == 0 || bytes > bytes_left {
                    kept.push(continues_line());
                    return kept;
                }
                lines_left -= 1;
                bytes_left -= bytes;
                kept.push(line);
            }
        }
    }
    kept
}

fn continues_line() -> ReaderLine {
    ReaderLine::Paragraph {
        spans: vec![ReaderSpan::Text(BODY_CONTINUES.to_string())],
    }
}

fn line_text_bytes(line: &ReaderLine) -> usize {
    match line {
        ReaderLine::Heading { text, .. } => text.len(),
        ReaderLine::Paragraph { spans }
        | ReaderLine::Quote { spans }
        | ReaderLine::ListItem { spans } => spans
            .iter()
            .map(|span| match span {
                ReaderSpan::Text(text) => text.len(),
                ReaderSpan::Link { text, href } => text.len() + href.len(),
            })
            .sum(),
        ReaderLine::Code { lines } => lines.iter().map(String::len).sum(),
        ReaderLine::Rule => 0,
    }
}

/// Whether a block's paragraphs read as body text or as a quote.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Body,
    Quote,
}

struct Walker<'a> {
    base_url: &'a Url,
    lines: Vec<ReaderLine>,
    /// Inline content sitting directly in a container (text between
    /// blocks, outside any `p`), flushed as one paragraph.
    loose: SpanBuilder,
}

impl Walker<'_> {
    fn walk_block(&mut self, node: &NodeRef<'_>, context: Context) {
        for child in node.children() {
            if child.is_text() {
                self.loose.push(&child.text(), None);
                continue;
            }
            let Some(name) = child.node_name() else {
                continue;
            };
            let name = name.to_ascii_lowercase();
            match name.as_str() {
                tag if is_dropped(tag) => {}
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    self.flush(context);
                    let text = collapse_whitespace(&child.text());
                    if !text.is_empty() {
                        let level = name.as_bytes()[1] - b'0';
                        self.lines.push(ReaderLine::Heading { level, text });
                    }
                }
                "p" => {
                    self.flush(context);
                    let spans = inline_spans(self.base_url, &child);
                    self.push_spans(context, spans);
                }
                "blockquote" => {
                    self.flush(context);
                    self.walk_block(&child, Context::Quote);
                    self.flush(Context::Quote);
                }
                "pre" => {
                    self.flush(context);
                    let lines = code_lines(&child.text());
                    if !lines.is_empty() {
                        self.lines.push(ReaderLine::Code { lines });
                    }
                }
                "li" => {
                    self.flush(context);
                    let spans = inline_spans(self.base_url, &child);
                    if !spans.is_empty() {
                        self.lines.push(ReaderLine::ListItem { spans });
                    }
                    // A nested list is its own items, after this one.
                    for nested in child.children() {
                        if nested.has_name("ul") || nested.has_name("ol") {
                            self.walk_block(&nested, context);
                        }
                    }
                }
                "hr" => {
                    self.flush(context);
                    self.lines.push(ReaderLine::Rule);
                }
                tag if is_inline(tag) => {
                    collect_inline_element(self.base_url, &child, None, &mut self.loose);
                }
                _ => {
                    self.flush(context);
                    self.walk_block(&child, context);
                    self.flush(context);
                }
            }
        }
    }

    fn flush(&mut self, context: Context) {
        let spans = std::mem::take(&mut self.loose).finish();
        self.push_spans(context, spans);
    }

    fn push_spans(&mut self, context: Context, spans: Vec<ReaderSpan>) {
        if spans.is_empty() {
            return;
        }
        self.lines.push(match context {
            Context::Body => ReaderLine::Paragraph { spans },
            Context::Quote => ReaderLine::Quote { spans },
        });
    }
}

fn inline_spans(base_url: &Url, node: &NodeRef<'_>) -> Vec<ReaderSpan> {
    let mut builder = SpanBuilder::default();
    collect_inline(base_url, node, None, &mut builder);
    builder.finish()
}

fn collect_inline(base_url: &Url, node: &NodeRef<'_>, link: Option<&str>, out: &mut SpanBuilder) {
    for child in node.children() {
        if child.is_text() {
            out.push(&child.text(), link);
        } else if child.is_element() {
            // A nested list belongs to the walk, not to this item's text.
            if child.has_name("ul") || child.has_name("ol") {
                continue;
            }
            collect_inline_element(base_url, &child, link, out);
        }
    }
}

fn collect_inline_element(
    base_url: &Url,
    node: &NodeRef<'_>,
    link: Option<&str>,
    out: &mut SpanBuilder,
) {
    let Some(name) = node.node_name() else {
        return;
    };
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        tag if is_dropped(tag) => {}
        "br" => out.push(" ", link),
        "a" if link.is_none() => {
            let href = node
                .attr("href")
                .and_then(|href| resolve_link(base_url, &href));
            collect_inline(base_url, node, href.as_deref(), out);
        }
        _ => collect_inline(base_url, node, link, out),
    }
}

/// Builds a line's spans with whitespace collapsed across span boundaries:
/// one space between words wherever they came from, none at either end.
#[derive(Default)]
struct SpanBuilder {
    spans: Vec<ReaderSpan>,
    pending_space: bool,
}

impl SpanBuilder {
    fn push(&mut self, text: &str, link: Option<&str>) {
        for ch in text.chars() {
            if ch.is_whitespace() {
                self.pending_space = !self.spans.is_empty();
                continue;
            }
            if self.pending_space {
                self.pending_space = false;
                if self.last_matches(link) {
                    self.last_text().push(' ');
                } else {
                    self.text_tail().push(' ');
                }
            }
            if !self.last_matches(link) {
                self.spans.push(match link {
                    None => ReaderSpan::Text(String::new()),
                    Some(href) => ReaderSpan::Link {
                        text: String::new(),
                        href: href.to_string(),
                    },
                });
            }
            self.last_text().push(ch);
        }
    }

    fn last_matches(&self, link: Option<&str>) -> bool {
        match (self.spans.last(), link) {
            (Some(ReaderSpan::Text(_)), None) => true,
            (Some(ReaderSpan::Link { href, .. }), Some(link)) => href == link,
            (Some(_), _) | (None, _) => false,
        }
    }

    fn last_text(&mut self) -> &mut String {
        match self.spans.last_mut() {
            Some(ReaderSpan::Text(text) | ReaderSpan::Link { text, .. }) => text,
            None => unreachable!("a span is pushed before its text"),
        }
    }

    /// The trailing plain-text span, opened if the last span is a link.
    fn text_tail(&mut self) -> &mut String {
        if !matches!(self.spans.last(), Some(ReaderSpan::Text(_))) {
            self.spans.push(ReaderSpan::Text(String::new()));
        }
        self.last_text()
    }

    fn finish(self) -> Vec<ReaderSpan> {
        self.spans
    }
}

fn is_dropped(tag: &str) -> bool {
    matches!(
        tag,
        "img"
            | "picture"
            | "figure"
            | "figcaption"
            | "table"
            | "script"
            | "style"
            | "noscript"
            | "template"
            | "nav"
            | "svg"
            | "video"
            | "audio"
            | "iframe"
            | "object"
            | "embed"
            | "canvas"
            | "form"
            | "button"
            | "input"
            | "select"
            | "textarea"
    )
}

fn is_inline(tag: &str) -> bool {
    matches!(
        tag,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "br"
            | "cite"
            | "code"
            | "data"
            | "del"
            | "dfn"
            | "em"
            | "font"
            | "i"
            | "ins"
            | "kbd"
            | "mark"
            | "q"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "u"
            | "var"
            | "wbr"
    )
}

/// An absolute http(s) link, or none: a `javascript:` or `mailto:` link
/// and a jump within the same page read as plain text.
fn resolve_link(base_url: &Url, href: &str) -> Option<String> {
    let resolved = base_url.join(href.trim()).ok()?;
    if !matches!(resolved.scheme(), "http" | "https") {
        return None;
    }
    let mut target = resolved.clone();
    target.set_fragment(None);
    let mut page = base_url.clone();
    page.set_fragment(None);
    if resolved.fragment().is_some() && target == page {
        return None;
    }
    Some(resolved.into())
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A `pre` block's lines verbatim, tabs as four spaces, trailing space and
/// blank lines at either end dropped.
fn code_lines(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .lines()
        .map(|line| line.replace('\t', "    ").trim_end().to_string())
        .collect();
    let first = lines.iter().position(|line| !line.is_empty());
    let last = lines.iter().rposition(|line| !line.is_empty());
    match (first, last) {
        (Some(first), Some(last)) => lines[first..=last].to_vec(),
        (None, _) | (_, None) => Vec::new(),
    }
}

fn is_html_content_type(content_type: &str) -> bool {
    let mime = content_type.split(';').next().unwrap_or_default().trim();
    mime == "text/html" || mime == "application/xhtml+xml"
}

fn request_error(e: reqwest::Error) -> BodyFetchError {
    let failure = if e.is_timeout() {
        BodyFetchFailure::Timeout
    } else {
        BodyFetchFailure::Fetch
    };
    BodyFetchError::new(failure, e)
}

#[cfg(test)]
#[path = "body_test.rs"]
mod body_test;
