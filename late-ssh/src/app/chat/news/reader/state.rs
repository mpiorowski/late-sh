//! The News reader: one story, opened from a News card, the live strip, or
//! the Live panel. Holds the story, its body as far as it has loaded, the
//! links the body numbers, and the scroll. The layout is pure and laid out
//! at draw time against the drawn width, then kept until the width, the
//! body, or the meta line changes, so a long body is not wrapped again every
//! frame. Lines carry [`ReaderInk`], and `ui.rs` turns inks into colours
//! inside the draw, the way the paper does.

use std::cell::{Ref, RefCell};

use chrono::{DateTime, Utc};
use late_core::models::article::ArticleFeedItem;
use late_core::models::article_body::{
    ArticleBody, BodyContent, BodyStatus, ReaderLine, ReaderSpan, reading_minutes,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use uuid::Uuid;

use crate::app::chat::news::svc::is_youtube_url;
use crate::app::chat::news::ui::split_summary_bullets;
use crate::app::chat::ui_text::format_news_ascii_art_for_display;
use crate::app::common::primitives::format_relative_time;
use crate::app::common::scroll_view::ScrollView;

/// The widest the article text runs, however wide the terminal.
pub(crate) const READER_MAX_TEXT_WIDTH: usize = 80;
/// Links `1`..`9` copy; later ones render as plain text.
pub(crate) const READER_MAX_NUMBERED_LINKS: usize = 9;
const ART_ROWS: usize = 6;

/// What a span of the reader is, never what colour it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReaderInk {
    Title,
    Meta,
    Heading,
    Body,
    Quote,
    Gutter,
    Code,
    Rule,
    /// A numbered link's `[n]`.
    LinkMark,
    /// A URL in the Links list.
    LinkUrl,
    Art,
    /// The fallback's one line: `Read on the web` or `Fetching`.
    Status,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReaderSegment {
    pub text: String,
    pub ink: ReaderInk,
}

impl ReaderSegment {
    fn new(text: impl Into<String>, ink: ReaderInk) -> Self {
        Self {
            text: text.into(),
            ink,
        }
    }
}

/// One drawn row, already wrapped to the text width.
pub(crate) type ReaderRow = Vec<ReaderSegment>;

/// The body as far as this session knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReaderBody {
    /// The card says the text is stored; its lines are on their way.
    Loading { reading_minutes: u16 },
    Ready {
        lines: Vec<ReaderLine>,
        /// The numbered links' URLs, in first-appearance order.
        links: Vec<String>,
        reading_minutes: u16,
    },
    /// The fetch has not landed.
    Pending,
    /// No text to show; the reader shows the summary and the link.
    Unavailable,
}

/// The rows last laid out, and what they were laid out for.
#[derive(Clone, Debug)]
struct ReaderLayout {
    width: usize,
    meta: String,
    rows: Vec<ReaderRow>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReaderModal {
    article_id: Uuid,
    title: String,
    url: String,
    summary: String,
    ascii_art: String,
    author: String,
    shared_at: DateTime<Utc>,
    /// A video the watch modal can play: the fallback names `Watch:`, and
    /// `w` hands the story over to the watch modal.
    watchable: bool,
    body: ReaderBody,
    view: ScrollView,
    /// Kept between draws: the draw asks for the rows every frame, and a
    /// long body is costly to wrap.
    layout: RefCell<Option<ReaderLayout>>,
}

impl ReaderModal {
    /// The reader on a snapshot story. A ready body starts `Loading` until
    /// its lines arrive; the rest show their summary straight away.
    pub(crate) fn open(item: &ArticleFeedItem) -> Self {
        let body = match item.body {
            BodyStatus::Ready { reading_minutes } => ReaderBody::Loading { reading_minutes },
            BodyStatus::Pending => ReaderBody::Pending,
            BodyStatus::Unavailable => ReaderBody::Unavailable,
        };
        Self {
            article_id: item.article.id,
            title: item.article.title.clone(),
            url: item.article.url.clone(),
            summary: item.article.summary.clone(),
            ascii_art: item.article.ascii_art.clone(),
            author: item.author_username.clone(),
            shared_at: item.article.created,
            watchable: is_youtube_url(&item.article.url),
            body,
            view: ScrollView::default(),
            layout: RefCell::new(None),
        }
    }

    pub(crate) fn article_id(&self) -> Uuid {
        self.article_id
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn watchable(&self) -> bool {
        self.watchable
    }

    pub(crate) fn body(&self) -> &ReaderBody {
        &self.body
    }

    pub(crate) fn view(&self) -> &ScrollView {
        &self.view
    }

    /// One page: the body's height less a line, so a line carries over.
    pub(crate) fn page(&self, direction: i32) {
        let height = i32::from(self.view.viewport().body.height);
        self.view.scroll(direction * (height - 1).max(1));
    }

    /// The body row as loaded: `None` is a fetch still pending.
    pub(crate) fn apply_body(&mut self, body: Option<ArticleBody>) {
        self.set_body(match body.map(|body| body.content) {
            Some(BodyContent::Ready { word_count, lines }) => ReaderBody::Ready {
                links: numbered_links(&lines),
                lines,
                reading_minutes: reading_minutes(word_count),
            },
            Some(BodyContent::Unavailable) => ReaderBody::Unavailable,
            None => ReaderBody::Pending,
        });
    }

    /// The body could not be loaded: show the summary and the link rather
    /// than a loading line that will never resolve.
    pub(crate) fn fail_load(&mut self) {
        self.set_body(ReaderBody::Unavailable);
    }

    /// A new body lays out afresh, from the top, against a fresh draw.
    fn set_body(&mut self, body: ReaderBody) {
        self.body = body;
        self.view = ScrollView::default();
        *self.layout.get_mut() = None;
    }

    /// The URL behind `[n]`, for the `1`..`9` keys.
    pub(crate) fn link(&self, n: usize) -> Option<&str> {
        match &self.body {
            ReaderBody::Ready { links, .. } => n
                .checked_sub(1)
                .and_then(|index| links.get(index))
                .map(String::as_str),
            ReaderBody::Loading { .. } | ReaderBody::Pending | ReaderBody::Unavailable => None,
        }
    }

    pub(crate) fn has_links(&self) -> bool {
        match &self.body {
            ReaderBody::Ready { links, .. } => !links.is_empty(),
            ReaderBody::Loading { .. } | ReaderBody::Pending | ReaderBody::Unavailable => false,
        }
    }

    /// Every row of the reader at `width` columns: the header, then the
    /// article text or the summary fallback. Laid out again only when the
    /// width, the body, or the meta line (its relative time) has changed.
    pub(crate) fn lay_out(&self, width: usize) -> Ref<'_, [ReaderRow]> {
        let width = width.max(1);
        let meta = self.meta();
        let fresh = matches!(
            &*self.layout.borrow(),
            Some(layout) if layout.width == width && layout.meta == meta
        );
        if !fresh {
            let rows = self.build_rows(width, &meta);
            self.layout
                .replace(Some(ReaderLayout { width, meta, rows }));
        }
        Ref::map(self.layout.borrow(), |layout| match layout {
            Some(layout) => layout.rows.as_slice(),
            None => unreachable!("the layout was stored above"),
        })
    }

    fn build_rows(&self, width: usize, meta: &str) -> Vec<ReaderRow> {
        let mut rows = Vec::new();
        match &self.body {
            ReaderBody::Ready { lines, links, .. } => {
                self.push_header(&mut rows, width, meta);
                push_article(&mut rows, lines, links, width);
            }
            ReaderBody::Loading { .. } => {
                self.push_fallback(&mut rows, width, meta, "Loading the article…".to_string());
            }
            // A video never gets a body to fetch: it is watched, not read.
            ReaderBody::Pending | ReaderBody::Unavailable if self.watchable => {
                self.push_fallback(&mut rows, width, meta, format!("Watch: {}", self.url));
            }
            ReaderBody::Pending => {
                self.push_fallback(&mut rows, width, meta, "Fetching the article…".to_string());
            }
            ReaderBody::Unavailable => {
                let status = format!("Read on the web: {}", self.url);
                self.push_fallback(&mut rows, width, meta, status);
            }
        }
        rows
    }

    fn meta(&self) -> String {
        let mut parts = vec![domain_label(&self.url)];
        match &self.body {
            ReaderBody::Loading { reading_minutes }
            | ReaderBody::Ready {
                reading_minutes, ..
            } => {
                parts.push(format!("{reading_minutes} min read"));
            }
            ReaderBody::Pending | ReaderBody::Unavailable => {}
        }
        parts.push(format!(
            "shared by @{} {}",
            self.author,
            format_relative_time(self.shared_at)
        ));
        parts.join(" · ")
    }

    fn push_header(&self, rows: &mut Vec<ReaderRow>, width: usize, meta: &str) {
        let title = if self.title.trim().is_empty() {
            "news update"
        } else {
            self.title.trim()
        };
        push_wrapped(
            rows,
            &[ReaderSegment::new(title, ReaderInk::Title)],
            width,
            0,
        );
        push_wrapped(rows, &[ReaderSegment::new(meta, ReaderInk::Meta)], width, 0);
        rows.push(vec![ReaderSegment::new("─".repeat(width), ReaderInk::Rule)]);
    }

    fn push_fallback(&self, rows: &mut Vec<ReaderRow>, width: usize, meta: &str, status: String) {
        let art = format_news_ascii_art_for_display(&self.ascii_art, ART_ROWS);
        if !art.is_empty()
            && art
                .iter()
                .all(|line| UnicodeWidthStr::width(line.as_str()) <= width)
        {
            for line in art {
                rows.push(vec![ReaderSegment::new(line, ReaderInk::Art)]);
            }
            rows.push(Vec::new());
        }
        self.push_header(rows, width, meta);
        rows.push(Vec::new());
        for bullet in split_summary_bullets(&self.summary) {
            push_wrapped(
                rows,
                &[ReaderSegment::new(bullet, ReaderInk::Body)],
                width,
                2,
            );
        }
        rows.push(Vec::new());
        push_wrapped(
            rows,
            &[ReaderSegment::new(status, ReaderInk::Status)],
            width,
            0,
        );
    }
}

/// The first [`READER_MAX_NUMBERED_LINKS`] distinct link targets, in the
/// order the text first reaches them.
fn numbered_links(lines: &[ReaderLine]) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    for line in lines {
        let spans = match line {
            ReaderLine::Paragraph { spans }
            | ReaderLine::Quote { spans }
            | ReaderLine::ListItem { spans } => spans,
            ReaderLine::Heading { .. } | ReaderLine::Code { .. } | ReaderLine::Rule => continue,
        };
        for span in spans {
            if let ReaderSpan::Link { href, .. } = span
                && !links.contains(href)
            {
                if links.len() == READER_MAX_NUMBERED_LINKS {
                    return links;
                }
                links.push(href.clone());
            }
        }
    }
    links
}

fn push_article(rows: &mut Vec<ReaderRow>, lines: &[ReaderLine], links: &[String], width: usize) {
    rows.push(Vec::new());
    for line in lines {
        match line {
            ReaderLine::Heading { level, text } => {
                if rows.last().is_some_and(|row| !row.is_empty()) {
                    rows.push(Vec::new());
                }
                let text = if *level <= 2 {
                    text.to_uppercase()
                } else {
                    text.clone()
                };
                push_wrapped(
                    rows,
                    &[ReaderSegment::new(text, ReaderInk::Heading)],
                    width,
                    0,
                );
            }
            ReaderLine::Paragraph { spans } => {
                push_wrapped(
                    rows,
                    &span_segments(spans, links, ReaderInk::Body),
                    width,
                    0,
                );
                rows.push(Vec::new());
            }
            ReaderLine::Quote { spans } => {
                let segments = span_segments(spans, links, ReaderInk::Quote);
                for row in wrap_segments(&segments, width.saturating_sub(2).max(1), 0) {
                    let mut gutter = vec![ReaderSegment::new("│ ", ReaderInk::Gutter)];
                    gutter.extend(row);
                    rows.push(gutter);
                }
                rows.push(Vec::new());
            }
            ReaderLine::Code { lines } => {
                for code in lines {
                    rows.push(vec![
                        ReaderSegment::new("│ ", ReaderInk::Gutter),
                        ReaderSegment::new(
                            clip_to_width(code, width.saturating_sub(2)),
                            ReaderInk::Code,
                        ),
                    ]);
                }
                rows.push(Vec::new());
            }
            ReaderLine::ListItem { spans } => {
                let mut segments = vec![ReaderSegment::new("• ", ReaderInk::Body)];
                segments.extend(span_segments(spans, links, ReaderInk::Body));
                push_wrapped(rows, &segments, width, 2);
            }
            ReaderLine::Rule => {
                rows.push(vec![ReaderSegment::new(
                    "─".repeat(width.min(24)),
                    ReaderInk::Rule,
                )]);
                rows.push(Vec::new());
            }
        }
    }
    if !links.is_empty() {
        if rows.last().is_some_and(|row| !row.is_empty()) {
            rows.push(Vec::new());
        }
        rows.push(vec![ReaderSegment::new("LINKS", ReaderInk::Heading)]);
        for (index, href) in links.iter().enumerate() {
            let mark = format!("[{}] ", index + 1);
            let room = width.saturating_sub(UnicodeWidthStr::width(mark.as_str()));
            rows.push(vec![
                ReaderSegment::new(mark, ReaderInk::LinkMark),
                ReaderSegment::new(clip_to_width(href, room), ReaderInk::LinkUrl),
            ]);
        }
    }
}

/// A line's spans with each numbered link followed by its `[n]`.
fn span_segments(spans: &[ReaderSpan], links: &[String], ink: ReaderInk) -> Vec<ReaderSegment> {
    let mut segments = Vec::new();
    for span in spans {
        match span {
            ReaderSpan::Text(text) => segments.push(ReaderSegment::new(text.clone(), ink)),
            ReaderSpan::Link { text, href } => {
                segments.push(ReaderSegment::new(text.clone(), ink));
                if let Some(index) = links.iter().position(|link| link == href) {
                    segments.push(ReaderSegment::new(
                        format!(" [{}]", index + 1),
                        ReaderInk::LinkMark,
                    ));
                }
            }
        }
    }
    segments
}

/// `example.com` out of the story's URL, without a leading `www.`.
pub(crate) fn domain_label(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(parsed) => {
            let host = parsed.host_str().unwrap_or_default();
            host.strip_prefix("www.").unwrap_or(host).to_string()
        }
        Err(_) => url.to_string(),
    }
}

fn push_wrapped(
    rows: &mut Vec<ReaderRow>,
    segments: &[ReaderSegment],
    width: usize,
    hanging: usize,
) {
    rows.extend(wrap_segments(segments, width, hanging));
}

/// Word-wrap styled text to `width` cells. Continuation rows are indented
/// by `hanging` spaces; a word wider than a row is broken across rows.
fn wrap_segments(segments: &[ReaderSegment], width: usize, hanging: usize) -> Vec<ReaderRow> {
    // A word keeps each of its characters' inks, so a word that changes
    // ink midway still wraps as one word.
    let mut words: Vec<Vec<ReaderSegment>> = Vec::new();
    let mut current: Vec<ReaderSegment> = Vec::new();
    for segment in segments {
        for ch in segment.text.chars() {
            if ch == ' ' {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                continue;
            }
            match current.last_mut() {
                Some(last) if last.ink == segment.ink => last.text.push(ch),
                _ => current.push(ReaderSegment::new(ch.to_string(), segment.ink)),
            }
        }
    }
    if !current.is_empty() {
        words.push(current);
    }

    let hanging = hanging.min(width.saturating_sub(1));
    let indent = || -> ReaderRow {
        match hanging {
            0 => Vec::new(),
            n => vec![ReaderSegment::new(" ".repeat(n), ReaderInk::Body)],
        }
    };
    let mut rows: Vec<ReaderRow> = Vec::new();
    let mut row: ReaderRow = Vec::new();
    // Cells used on the row, and how many of them are its indent.
    let mut used = 0;
    let mut row_start = 0;
    for word in words {
        let word_width: usize = word
            .iter()
            .map(|segment| UnicodeWidthStr::width(segment.text.as_str()))
            .sum();
        if used > row_start {
            if used + 1 + word_width > width {
                rows.push(std::mem::replace(&mut row, indent()));
                used = hanging;
                row_start = hanging;
            } else {
                append(&mut row, " ", word[0].ink);
                used += 1;
            }
        }
        for segment in word {
            for ch in segment.text.chars() {
                let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
                if used + ch_width > width && used > row_start {
                    rows.push(std::mem::replace(&mut row, indent()));
                    used = hanging;
                    row_start = hanging;
                }
                append(&mut row, &ch.to_string(), segment.ink);
                used += ch_width;
            }
        }
    }
    if used > row_start {
        rows.push(row);
    }
    rows
}

fn append(row: &mut ReaderRow, text: &str, ink: ReaderInk) {
    match row.last_mut() {
        Some(last) if last.ink == ink => last.text.push_str(text),
        _ => row.push(ReaderSegment::new(text, ink)),
    }
}

fn clip_to_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
