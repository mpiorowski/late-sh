//! The Late Edition on the public web: the columns every login reads, on
//! a page a link can point at, plus an RSS feed of them. Only what names
//! no room's conversation goes out: the operator's `#announcements` as
//! written, YESTERDAY ON LATE.SH, WHAT WE WERE READING, and OUTSIDE. The
//! room columns stay inside the clubhouse. The press is
//! `late-ssh/src/app/paper`; this module only reads the rows it prints.

use std::collections::BTreeMap;

use anyhow::Context;
use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    http::{StatusCode, header::CONTENT_TYPE},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use chrono::{Days, NaiveDate, Utc};
use late_core::{
    db::PooledClient,
    models::{
        chat_message::ChatMessage,
        chat_room::ChatRoom,
        paper::{
            ANNOUNCEMENTS_LIMIT, ANNOUNCEMENTS_SLUG, EditionStep, PaperEdition, PaperSectionKind,
            PaperSectionRow,
        },
    },
};

use crate::{
    AppState,
    error::AppError,
    metrics,
    pages::shared::{
        board_png,
        card::{self, CardLine, Ink},
        meta,
        text::{escape_html, split_paragraphs},
    },
};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/paper", get(latest_handler))
        .route("/paper/feed.xml", get(feed_handler))
        .route("/paper/{date}", get(dated_handler))
        .route("/paper/{date}/image.png", get(image_handler))
}

/// How much of each column the front-page card shows: its first
/// paragraphs, cut to this many lines.
const CARD_COLUMN_LINES: usize = 10;

/// How many editions the feed carries.
const FEED_EDITIONS: i64 = 20;

#[derive(Template)]
#[template(path = "pages/paper/page.html")]
struct Page {
    /// `2026-10-22`.
    edition: String,
    /// `Thursday, 22 October 2026`.
    dateline: String,
    /// False when the day has nothing to show: no printed column and no
    /// announcement over a swept edition.
    printed: bool,
    announcements: Vec<Announcement>,
    yesterday: Vec<String>,
    reading: Vec<String>,
    outside: Vec<String>,
    /// Paths of the nearest editions with a printed column either side.
    earlier: Option<String>,
    later: Option<String>,
    /// The preview card a pasted link unfurls into (`shared/meta.html`).
    page_url: String,
    og_title: String,
    og_description: String,
    og_image: String,
    og_image_alt: String,
}

struct Announcement {
    author: String,
    /// `HH:MM` UTC.
    time: String,
    lines: Vec<String>,
}

#[derive(Template)]
#[template(path = "pages/paper/feed.xml")]
struct Feed {
    base_url: String,
    items: Vec<FeedItem>,
}

struct FeedItem {
    title: String,
    url: String,
    /// RFC 2822, the edition's midnight UTC.
    pub_date: String,
    /// The columns as HTML, escaped once here; the template escapes it
    /// again for the XML, which is what a feed reader unescapes.
    description_html: String,
}

/// The newest edition with a printed column, or today's date with nothing
/// on it while the press has never printed.
#[tracing::instrument(skip(state))]
async fn latest_handler(State(state): State<AppState>) -> Result<Response, AppError> {
    metrics::record_page_view("paper", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for the paper")?;
    let today = Utc::now().date_naive();
    let latest = PaperSectionRow::nearest_ready(&client, tomorrow(today), EditionStep::Earlier)
        .await
        .context("failed to find the latest edition")?;
    let page = read_page(
        &client,
        &state.config.public_base_url,
        latest.unwrap_or(today),
    )
    .await?;
    Ok(Html(page.render()?).into_response())
}

/// One dated edition; a date that is not one, or one with nothing to
/// show, is 404.
#[tracing::instrument(skip(state))]
async fn dated_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Result<Response, AppError> {
    metrics::record_page_view("paper", false);

    let Ok(edition) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for the paper")?;
    let page = read_page(&client, &state.config.public_base_url, edition).await?;
    let status = match page.printed {
        true => StatusCode::OK,
        false => StatusCode::NOT_FOUND,
    };
    Ok((status, Html(page.render()?)).into_response())
}

/// The edition's front page as a card: the masthead, the dateline,
/// YESTERDAY ON LATE.SH whole and the head of OUTSIDE and WHAT WE WERE
/// READING, the picture the herald posts on a day with nothing drawn
/// (`late-ssh/src/app/herald`). A path that is not a date, or an edition
/// with no column printed, is 404.
#[tracing::instrument(skip(state))]
async fn image_handler(
    State(state): State<AppState>,
    Path(date): Path<String>,
) -> Result<Response, AppError> {
    metrics::record_page_view("paper_image", false);

    let Ok(edition) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for the paper card")?;
    let issue = PaperEdition::load(&client, edition)
        .await
        .context("failed to load the edition")?;
    let Some(lines) = front_page(
        edition,
        &ready_column(&issue, PaperSectionKind::Yesterday),
        &ready_column(&issue, PaperSectionKind::Outside),
        &ready_column(&issue, PaperSectionKind::Reading),
    ) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let png = board_png::render_blocking(card::canvas(&lines))
        .await
        .context("failed to render the paper card")?;
    Ok(board_png::response(png))
}

/// A column the press printed, as paragraphs; empty when it did not.
fn ready_column(issue: &PaperEdition, kind: PaperSectionKind) -> Vec<String> {
    match issue.ready_text(kind) {
        Some(text) => split_paragraphs(text),
        None => Vec::new(),
    }
}

/// The front page's lines, `None` when no column printed. YESTERDAY's
/// lines are numbers and few, so they all fit under the cap the prose
/// columns are cut to.
pub(crate) fn front_page(
    edition: NaiveDate,
    yesterday: &[String],
    outside: &[String],
    reading: &[String],
) -> Option<Vec<CardLine>> {
    if yesterday.is_empty() && outside.is_empty() && reading.is_empty() {
        return None;
    }
    let mut lines = vec![
        CardLine::new(Ink::Title, "THE LATE EDITION"),
        CardLine::new(Ink::Dim, edition.format("%A, %-d %B %Y").to_string()),
    ];
    for (heading, column) in [
        ("YESTERDAY ON LATE.SH", yesterday),
        ("OUTSIDE", outside),
        ("WHAT WE WERE READING", reading),
    ] {
        if column.is_empty() {
            continue;
        }
        lines.push(CardLine::blank());
        lines.push(CardLine::new(Ink::Heading, heading));
        let mut room = CARD_COLUMN_LINES;
        for text in column {
            if room == 0 {
                break;
            }
            let wrapped = card::paragraph(Ink::Body, text, room);
            room -= wrapped.len();
            lines.extend(wrapped);
        }
    }
    lines.push(CardLine::blank());
    lines.push(CardLine::new(Ink::Dim, "late.sh/paper"));
    Some(lines)
}

/// RSS 2.0: one item per edition with a printed column, newest first.
#[tracing::instrument(skip(state))]
async fn feed_handler(State(state): State<AppState>) -> Result<Response, AppError> {
    metrics::record_page_view("paper_feed", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for the paper feed")?;
    let before = tomorrow(Utc::now().date_naive());
    let yesterday = PaperSectionRow::list_recent_ready(
        &client,
        PaperSectionKind::Yesterday,
        before,
        FEED_EDITIONS,
    )
    .await
    .context("failed to list the yesterday columns")?;
    let reading = PaperSectionRow::list_recent_ready(
        &client,
        PaperSectionKind::Reading,
        before,
        FEED_EDITIONS,
    )
    .await
    .context("failed to list the reading columns")?;
    let outside = PaperSectionRow::list_recent_ready(
        &client,
        PaperSectionKind::Outside,
        before,
        FEED_EDITIONS,
    )
    .await
    .context("failed to list the outside columns")?;
    let feed = Feed {
        items: feed_items(&state.config.public_base_url, yesterday, reading, outside),
        base_url: state.config.public_base_url.clone(),
    };
    Ok((
        [(CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
        feed.render()?,
    )
        .into_response())
}

/// The day after `date`, so a query over editions strictly before it
/// counts the date itself.
fn tomorrow(date: NaiveDate) -> NaiveDate {
    date.checked_add_days(Days::new(1))
        .expect("tomorrow exists on every date the press prints")
}

fn edition_path(edition: NaiveDate) -> String {
    format!("/paper/{edition}")
}

/// Everything the page shows for `edition`, in page order.
async fn read_page(client: &PooledClient, base: &str, edition: NaiveDate) -> anyhow::Result<Page> {
    let issue = PaperEdition::load(client, edition)
        .await
        .context("failed to load the edition")?;
    let announcements = match issue.is_swept() {
        true => read_announcements(client, edition).await?,
        false => Vec::new(),
    };
    let yesterday = ready_column(&issue, PaperSectionKind::Yesterday);
    let reading = ready_column(&issue, PaperSectionKind::Reading);
    let outside = ready_column(&issue, PaperSectionKind::Outside);
    let printed = !yesterday.is_empty()
        || !reading.is_empty()
        || !outside.is_empty()
        || !announcements.is_empty();
    let earlier = PaperSectionRow::nearest_ready(client, edition, EditionStep::Earlier)
        .await
        .context("failed to find the earlier edition")?
        .map(edition_path);
    let later = PaperSectionRow::nearest_ready(client, edition, EditionStep::Later)
        .await
        .context("failed to find the later edition")?
        .map(edition_path);
    let og_description = outside
        .first()
        .or(reading.first())
        .cloned()
        .unwrap_or_else(|| "late.sh's daily paper, printed once a day.".to_string());
    let (og_image, og_image_alt) = meta::site_image(base);
    Ok(Page {
        edition: edition.to_string(),
        dateline: edition.format("%A, %-d %B %Y").to_string(),
        printed,
        announcements,
        yesterday,
        reading,
        outside,
        earlier,
        later,
        page_url: format!("{base}{}", edition_path(edition)),
        og_title: format!("The Late Edition, {}", edition.format("%-d %B %Y")),
        og_description,
        og_image,
        og_image_alt,
    })
}

/// The operator's `#announcements` posts over the edition's window,
/// oldest first, as written. No room means no announcements.
async fn read_announcements(
    client: &PooledClient,
    edition: NaiveDate,
) -> anyhow::Result<Vec<Announcement>> {
    let Some(room) = ChatRoom::find_public_non_dm_by_slug(client, ANNOUNCEMENTS_SLUG)
        .await
        .context("failed to find the announcements room")?
    else {
        return Ok(Vec::new());
    };
    let (floor, ceiling) = PaperEdition::window(edition);
    let posts = ChatMessage::list_public_room_between_with_author(
        client,
        room.id,
        floor,
        ceiling,
        ANNOUNCEMENTS_LIMIT,
    )
    .await
    .context("failed to list the announcements")?;
    Ok(posts
        .into_iter()
        .map(|post| Announcement {
            author: post.author,
            time: post.created.format("%H:%M").to_string(),
            lines: split_paragraphs(&post.body),
        })
        .collect())
}

/// An edition's public columns as the feed found them.
#[derive(Default)]
struct FeedColumns {
    yesterday: Option<String>,
    reading: Option<String>,
    outside: Option<String>,
}

/// One item per edition that printed any column, newest first, every
/// printed column in it, in page order.
fn feed_items(
    base: &str,
    yesterday: Vec<(NaiveDate, String)>,
    reading: Vec<(NaiveDate, String)>,
    outside: Vec<(NaiveDate, String)>,
) -> Vec<FeedItem> {
    let mut by_edition: BTreeMap<NaiveDate, FeedColumns> = BTreeMap::new();
    for (edition, text) in yesterday {
        by_edition.entry(edition).or_default().yesterday = Some(text);
    }
    for (edition, text) in reading {
        by_edition.entry(edition).or_default().reading = Some(text);
    }
    for (edition, text) in outside {
        by_edition.entry(edition).or_default().outside = Some(text);
    }
    by_edition
        .into_iter()
        .rev()
        .take(FEED_EDITIONS as usize)
        .map(|(edition, columns)| {
            let mut description_html = String::new();
            if let Some(text) = columns.yesterday {
                push_column(&mut description_html, "Yesterday on late.sh", &text);
            }
            if let Some(text) = columns.reading {
                push_column(&mut description_html, "What we were reading", &text);
            }
            if let Some(text) = columns.outside {
                push_column(&mut description_html, "Outside", &text);
            }
            FeedItem {
                title: format!("The Late Edition, {}", edition.format("%-d %B %Y")),
                url: format!("{base}{}", edition_path(edition)),
                // RFC 2822 with the day zero-padded, the form every feed
                // reader has seen; chrono's own `to_rfc2822` leaves it bare.
                pub_date: edition
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight exists on every date")
                    .and_utc()
                    .format("%a, %d %b %Y %H:%M:%S +0000")
                    .to_string(),
                description_html,
            }
        })
        .collect()
}

fn push_column(html: &mut String, heading: &str, text: &str) {
    html.push_str("<h3>");
    html.push_str(&escape_html(heading));
    html.push_str("</h3>");
    for paragraph in split_paragraphs(text) {
        html.push_str("<p>");
        html.push_str(&escape_html(&paragraph));
        html.push_str("</p>");
    }
}

#[cfg(test)]
mod paper_test;
