//! The News reader: one story, opened from a News card, the live strip, or
//! the Live panel. Holds the story, its body as far as it has loaded, the
//! links the body numbers, and the scroll. The layout is pure and laid out
//! at draw time against the drawn width; lines carry [`ReaderInk`], and
//! `ui.rs` turns inks into colours inside the draw, the way the paper does.

use std::cell::Cell;

use chrono::{DateTime, Utc};
use late_core::models::article::ArticleFeedItem;
use late_core::models::article_body::{
    ArticleBody, BodyContent, BodyStatus, ReaderLine, ReaderSpan,
};
use ratatui::layout::Rect;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use uuid::Uuid;

use crate::app::chat::news::ui::split_summary_bullets;
use crate::app::chat::ui_text::format_news_ascii_art_for_display;
use crate::app::common::primitives::format_relative_time;

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
    Loading,
    Ready(Vec<ReaderLine>),
    /// The fetch has not landed.
    Pending,
    /// No text to show; the reader shows the summary and the link.
    Unavailable,
}

/// Geometry and wrapped extent published by the draw, in frame cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ReaderViewport {
    pub popup: Rect,
    pub body: Rect,
    pub close: Rect,
    pub track: Rect,
    pub content_height: u16,
}

impl ReaderViewport {
    fn max_scroll(self) -> u16 {
        self.content_height.saturating_sub(self.body.height)
    }
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
    reading_minutes: Option<u16>,
    body: ReaderBody,
    /// The numbered links' URLs, in first-appearance order.
    links: Vec<String>,
    scroll_offset: Cell<u16>,
    viewport: Cell<ReaderViewport>,
    drag_grab: Cell<Option<u16>>,
}

impl ReaderModal {
    /// The reader on a snapshot story. A ready body starts `Loading` until
    /// its lines arrive; the rest show their summary straight away.
    pub(crate) fn open(item: &ArticleFeedItem) -> Self {
        let body = match item.body {
            BodyStatus::Ready => ReaderBody::Loading,
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
            reading_minutes: item.reading_minutes,
            body,
            links: Vec::new(),
            scroll_offset: Cell::new(0),
            viewport: Cell::new(ReaderViewport::default()),
            drag_grab: Cell::new(None),
        }
    }

    pub(crate) fn article_id(&self) -> Uuid {
        self.article_id
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn body(&self) -> &ReaderBody {
        &self.body
    }

    /// The body row as loaded: `None` is a fetch still pending.
    pub(crate) fn apply_body(&mut self, body: Option<ArticleBody>) {
        self.body = match body.map(|body| body.content) {
            Some(BodyContent::Ready { word_count, lines }) => {
                self.reading_minutes =
                    Some(late_core::models::article_body::reading_minutes(word_count));
                ReaderBody::Ready(lines)
            }
            Some(BodyContent::Unavailable) => ReaderBody::Unavailable,
            None => ReaderBody::Pending,
        };
        self.links = match &self.body {
            ReaderBody::Ready(lines) => numbered_links(lines),
            ReaderBody::Loading | ReaderBody::Pending | ReaderBody::Unavailable => Vec::new(),
        };
        self.invalidate_viewport();
        self.scroll_offset.set(0);
    }

    /// The URL behind `[n]`, for the `1`..`9` keys.
    pub(crate) fn link(&self, n: usize) -> Option<&str> {
        n.checked_sub(1)
            .and_then(|index| self.links.get(index))
            .map(String::as_str)
    }

    pub(crate) fn has_links(&self) -> bool {
        !self.links.is_empty()
    }

    /// Every row of the reader at `width` columns: the header, then the
    /// article text or the summary fallback.
    pub(crate) fn lay_out(&self, width: usize) -> Vec<ReaderRow> {
        let width = width.max(1);
        let mut rows = Vec::new();
        match &self.body {
            ReaderBody::Ready(lines) => {
                self.push_header(&mut rows, width);
                self.push_article(&mut rows, lines, width);
            }
            ReaderBody::Loading => {
                self.push_fallback(&mut rows, width, "Loading the article…".to_string());
            }
            ReaderBody::Pending => {
                self.push_fallback(&mut rows, width, "Fetching the article…".to_string());
            }
            ReaderBody::Unavailable => {
                self.push_fallback(&mut rows, width, format!("Read on the web: {}", self.url));
            }
        }
        rows
    }

    fn meta(&self) -> String {
        let mut parts = vec![domain_label(&self.url)];
        if let (ReaderBody::Ready(_) | ReaderBody::Loading, Some(minutes)) =
            (&self.body, self.reading_minutes)
        {
            parts.push(format!("{minutes} min read"));
        }
        parts.push(format!(
            "shared by @{} {}",
            self.author,
            format_relative_time(self.shared_at)
        ));
        parts.join(" · ")
    }

    fn push_header(&self, rows: &mut Vec<ReaderRow>, width: usize) {
        let title = if self.title.trim().is_empty() {
            "news update"
        } else {
            self.title.trim()
        };
        push_wrapped(
            rows,
            &[ReaderSegment::new(title, ReaderInk::Title)],
            width,
            None,
        );
        push_wrapped(
            rows,
            &[ReaderSegment::new(self.meta(), ReaderInk::Meta)],
            width,
            None,
        );
        rows.push(vec![ReaderSegment::new("─".repeat(width), ReaderInk::Rule)]);
    }

    fn push_fallback(&self, rows: &mut Vec<ReaderRow>, width: usize, status: String) {
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
        self.push_header(rows, width);
        rows.push(Vec::new());
        for bullet in split_summary_bullets(&self.summary) {
            push_wrapped(
                rows,
                &[ReaderSegment::new(bullet, ReaderInk::Body)],
                width,
                Some(2),
            );
        }
        rows.push(Vec::new());
        push_wrapped(
            rows,
            &[ReaderSegment::new(status, ReaderInk::Status)],
            width,
            None,
        );
    }

    fn push_article(&self, rows: &mut Vec<ReaderRow>, lines: &[ReaderLine], width: usize) {
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
                        None,
                    );
                }
                ReaderLine::Paragraph { spans } => {
                    push_wrapped(
                        rows,
                        &self.span_segments(spans, ReaderInk::Body),
                        width,
                        None,
                    );
                    rows.push(Vec::new());
                }
                ReaderLine::Quote { spans } => {
                    let segments = self.span_segments(spans, ReaderInk::Quote);
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
                    segments.extend(self.span_segments(spans, ReaderInk::Body));
                    push_wrapped(rows, &segments, width, Some(2));
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
        if !self.links.is_empty() {
            if rows.last().is_some_and(|row| !row.is_empty()) {
                rows.push(Vec::new());
            }
            rows.push(vec![ReaderSegment::new("LINKS", ReaderInk::Heading)]);
            for (index, href) in self.links.iter().enumerate() {
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
    fn span_segments(&self, spans: &[ReaderSpan], ink: ReaderInk) -> Vec<ReaderSegment> {
        let mut segments = Vec::new();
        for span in spans {
            match span {
                ReaderSpan::Text(text) => segments.push(ReaderSegment::new(text.clone(), ink)),
                ReaderSpan::Link { text, href } => {
                    segments.push(ReaderSegment::new(text.clone(), ink));
                    if let Some(index) = self.links.iter().position(|link| link == href) {
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

    pub(crate) fn scroll_offset(&self) -> u16 {
        self.scroll_offset.get()
    }

    /// Clamped to the last drawn body; with no body drawn yet the offset
    /// stays put and the next draw clamps it.
    pub(crate) fn scroll(&self, delta: i32) {
        let viewport = self.viewport.get();
        if viewport.body.is_empty() {
            return;
        }
        let next = i32::from(self.scroll_offset.get()) + delta;
        self.scroll_offset
            .set(next.clamp(0, i32::from(viewport.max_scroll())) as u16);
    }

    /// One page: the body's height less a line, so a line carries over.
    pub(crate) fn page(&self, direction: i32) {
        let height = i32::from(self.viewport.get().body.height);
        self.scroll(direction * (height - 1).max(1));
    }

    pub(crate) fn scroll_to_top(&self) {
        self.scroll_offset.set(0);
    }

    pub(crate) fn scroll_to_bottom(&self) {
        self.scroll(i32::from(u16::MAX));
    }

    pub(crate) fn viewport(&self) -> ReaderViewport {
        self.viewport.get()
    }

    pub(crate) fn set_viewport(&self, viewport: ReaderViewport) {
        if self.viewport.replace(viewport) != viewport {
            self.cancel_drag();
        }
        self.scroll(0);
    }

    fn invalidate_viewport(&self) {
        self.viewport.set(ReaderViewport::default());
        self.cancel_drag();
    }

    pub(crate) fn thumb(&self) -> Rect {
        let viewport = self.viewport.get();
        let track = viewport.track;
        if track.is_empty() || viewport.max_scroll() == 0 {
            return Rect::default();
        }
        let height = (u32::from(track.height) * u32::from(viewport.body.height)
            / u32::from(viewport.content_height))
        .max(1)
        .min(u32::from(track.height)) as u16;
        let travel = track.height - height;
        let top = (u32::from(self.scroll_offset.get()) * u32::from(travel)
            + u32::from(viewport.max_scroll()) / 2)
            / u32::from(viewport.max_scroll());
        Rect::new(track.x, track.y + top as u16, 1, height)
    }

    pub(crate) fn begin_drag(&self, y: u16) {
        self.drag_grab.set(Some(y.saturating_sub(self.thumb().y)));
    }

    pub(crate) fn cancel_drag(&self) {
        self.drag_grab.set(None);
    }

    pub(crate) fn drag_to(&self, y: u16) {
        let Some(grab) = self.drag_grab.get() else {
            return;
        };
        let viewport = self.viewport.get();
        let travel = viewport.track.height.saturating_sub(self.thumb().height);
        if travel == 0 {
            return;
        }
        let top = (i32::from(y) - i32::from(viewport.track.y) - i32::from(grab))
            .clamp(0, i32::from(travel)) as u32;
        self.scroll_offset.set(
            ((top * u32::from(viewport.max_scroll()) + u32::from(travel) / 2) / u32::from(travel))
                as u16,
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

/// `example.com` out of the story's URL, without a leading `www.`.
fn domain_label(url: &str) -> String {
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
    hanging: Option<usize>,
) {
    rows.extend(wrap_segments(segments, width, hanging.unwrap_or(0)));
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
