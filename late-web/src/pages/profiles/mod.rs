use std::collections::HashSet;

use anyhow::Context;
use askama::Template;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
};
use chrono::{DateTime, Utc};
use dartboard_core::Canvas;
use late_core::models::{
    deadchannel_runner::DeadchannelRunner,
    profile::Profile,
    showcase::Showcase,
    work_profile::{WorkProfile, WorkStatus},
};

use crate::{
    AppState,
    error::AppError,
    metrics,
    pages::shared::{board_png, text::split_paragraphs},
};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/profiles", get(index_handler))
        .route("/profiles/{slug}", get(handler))
        .route("/profiles/{slug}/poster.png", get(poster_handler))
}

#[derive(Template)]
#[template(path = "pages/profiles/page.html")]
struct Page {
    /// The runner's poster, its cells drawn by `/static/board.js` like a
    /// gallery snapshot. `None` without a standing runner or a poster.
    poster_json: Option<String>,
    /// The preview card a pasted link unfurls into (`shared/meta.html`):
    /// the poster as its picture when there is one, the site's otherwise.
    page_url: String,
    og_title: String,
    og_description: String,
    og_image: String,
    og_image_alt: String,
    headline: String,
    username: String,
    status_id: &'static str,
    status_label: &'static str,
    work_type: String,
    location: String,
    show_contact: bool,
    contact: String,
    contacts: Vec<ContactItem>,
    skills: Vec<String>,
    links: Vec<String>,
    summary_paragraphs: Vec<String>,
    slug: String,
    created: String,
    updated: String,
    show_bio: bool,
    bio_html: String,
    show_late_fetch: bool,
    fetch_created: String,
    fetch_theme: String,
    fetch_ide: String,
    fetch_terminal: String,
    fetch_os: String,
    fetch_langs: Vec<String>,
    show_showcases: bool,
    showcases: Vec<ShowcaseItem>,
}

#[derive(Template)]
#[template(path = "pages/profiles/not_found.html")]
struct NotFound {
    slug: String,
}

struct ShowcaseItem {
    title: String,
    url: String,
    description_paragraphs: Vec<String>,
    tags: Vec<String>,
}

struct ContactItem {
    value: String,
    /// Empty when the entry isn't a recognised email/URL.
    href: String,
}

#[derive(Template)]
#[template(path = "pages/profiles/index.html")]
struct Index {
    items: Vec<IndexItem>,
    open_count: usize,
    casual_count: usize,
    closed_count: usize,
}

struct IndexItem {
    slug: String,
    headline: String,
    username: String,
    status_id: &'static str,
    status_label: &'static str,
    work_type: String,
    location: String,
    skills: Vec<String>,
    /// The author's last login, the index's sort key.
    last_seen: String,
}

const PROFILE_LIST_LIMIT: i64 = 100;
/// The index row shows the same five tags the terminal row does.
const INDEX_ROW_TAGS: usize = 5;

#[tracing::instrument(skip(state))]
async fn handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    metrics::record_page_view("profiles", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for profile page")?;

    let Some(work) = WorkProfile::find_by_slug(&client, &slug)
        .await
        .context("failed to load work profile by slug")?
    else {
        let page = NotFound { slug: slug.clone() };
        return Ok((StatusCode::NOT_FOUND, Html(page.render()?)).into_response());
    };

    let user_profile = Profile::load(&client, work.user_id)
        .await
        .context("failed to load author profile")?;

    let bio_html = render_markdown(&user_profile.bio);
    let show_bio = !bio_html.is_empty();

    let showcases = Showcase::list_by_user_id(&client, work.user_id)
        .await
        .context("failed to load author showcases")?
        .into_iter()
        .map(|s| ShowcaseItem {
            title: s.title,
            url: s.url,
            description_paragraphs: split_paragraphs(&s.description),
            tags: s.tags,
        })
        .collect::<Vec<_>>();
    let show_showcases = !showcases.is_empty();

    let contacts = parse_contacts(&work.contact);
    let show_contact = !contacts.is_empty();

    let poster_json = match DeadchannelRunner::poster(&client, work.user_id)
        .await
        .context("failed to load runner poster")?
    {
        Some(value) => match serde_json::from_value::<Canvas>(value) {
            Ok(canvas) => Some(crate::pages::gallery::canvas_json(&canvas)?),
            Err(error) => {
                tracing::error!(error = %error, user_id = %work.user_id, "runner poster failed to decode; profile page shows none");
                None
            }
        },
        None => None,
    };
    let base = state.config.public_base_url.as_str();
    let page_url = format!("{base}/profiles/{}", work.slug);
    let (og_image, og_image_alt) = match poster_json {
        Some(_) => (
            format!("{page_url}/poster.png"),
            format!(
                "@{}'s poster, painted at PRINTS in the late.sh undercity",
                user_profile.username
            ),
        ),
        None => (
            format!("{base}/static/og-image.png"),
            "late.sh terminal clubhouse preview".to_string(),
        ),
    };
    let page = Page {
        poster_json,
        og_title: format!("@{} on late.sh", user_profile.username),
        og_description: format!("{} · {}", work.headline, status_label(work.status)),
        page_url,
        og_image,
        og_image_alt,
        headline: work.headline,
        username: user_profile.username.clone(),
        status_id: work.status.as_str(),
        status_label: status_label(work.status),
        work_type: work.work_type.label().to_string(),
        location: work.location,
        show_contact,
        contact: work.contact,
        contacts,
        skills: work.skills,
        links: work.links,
        summary_paragraphs: split_paragraphs(&work.summary),
        slug: work.slug.clone(),
        created: format_date(work.created),
        updated: format_date(work.updated),
        show_bio,
        bio_html,
        show_late_fetch: true,
        fetch_created: user_profile
            .created_at
            .map(format_date)
            .unwrap_or_else(|| "—".to_string()),
        fetch_theme: user_profile
            .theme_id
            .clone()
            .unwrap_or_else(|| "contrast".to_string()),
        fetch_ide: dash_or(user_profile.ide.as_deref()),
        fetch_terminal: dash_or(user_profile.terminal.as_deref()),
        fetch_os: dash_or(user_profile.os.as_deref()),
        fetch_langs: user_profile.langs,
        show_showcases,
        showcases,
    };

    Ok(Html(page.render()?).into_response())
}

/// The runner's poster as the picture a profile link's preview card
/// shows. No profile, no standing runner, no poster, or one that fails to
/// decode: 404, and the card falls back to the site's own picture.
#[tracing::instrument(skip(state))]
async fn poster_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    metrics::record_page_view("profile_poster", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for profile poster")?;

    let Some(work) = WorkProfile::find_by_slug(&client, &slug)
        .await
        .context("failed to load work profile by slug")?
    else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let Some(value) = DeadchannelRunner::poster(&client, work.user_id)
        .await
        .context("failed to load runner poster")?
    else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let canvas = match serde_json::from_value::<Canvas>(value) {
        Ok(canvas) => canvas,
        Err(error) => {
            tracing::error!(error = %error, user_id = %work.user_id, "runner poster failed to decode; no poster image");
            return Ok(StatusCode::NOT_FOUND.into_response());
        }
    };
    let png = board_png::render(&canvas).context("failed to render runner poster")?;
    Ok(board_png::response(png))
}

#[tracing::instrument(skip(state))]
async fn index_handler(State(state): State<AppState>) -> Result<Response, AppError> {
    metrics::record_page_view("profiles_index", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for profiles index")?;

    // Whoever logged in last, first: the model sorts by `users.last_seen`.
    let profiles = WorkProfile::list_index(&client, PROFILE_LIST_LIMIT)
        .await
        .context("failed to list work profiles")?;

    let user_ids: Vec<_> = profiles
        .iter()
        .map(|p| p.user_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let authors = Profile::list_by_user_ids(&client, &user_ids)
        .await
        .context("failed to load author profiles")?;

    let mut open_count = 0;
    let mut casual_count = 0;
    let mut closed_count = 0;
    let items: Vec<IndexItem> = profiles
        .into_iter()
        .map(|p| {
            match p.status {
                WorkStatus::Open => open_count += 1,
                WorkStatus::Casual => casual_count += 1,
                WorkStatus::NotLooking => closed_count += 1,
            }
            let author = authors.get(&p.user_id);
            IndexItem {
                username: match author {
                    Some(author) if !author.username.is_empty() => author.username.clone(),
                    _ => p.user_id.to_string()[..8].to_string(),
                },
                status_id: p.status.as_str(),
                status_label: status_label(p.status),
                work_type: p.work_type.label().to_string(),
                location: p.location,
                skills: p.skills.into_iter().take(INDEX_ROW_TAGS).collect(),
                last_seen: match author.and_then(|author| author.last_seen) {
                    Some(last_seen) => format_date(last_seen),
                    None => "—".to_string(),
                },
                headline: p.headline,
                slug: p.slug,
            }
        })
        .collect();

    let page = Index {
        items,
        open_count,
        casual_count,
        closed_count,
    };
    Ok(Html(page.render()?).into_response())
}

/// The long form of the status for the web, where there is room for it.
fn status_label(status: WorkStatus) -> &'static str {
    match status {
        WorkStatus::Open => "open to work",
        WorkStatus::Casual => "casually listening",
        WorkStatus::NotLooking => "not looking",
    }
}

/// Render a user-supplied markdown bio as HTML. Raw HTML in the source is
/// stripped so an author can't smuggle <script> or other arbitrary tags.
fn render_markdown(text: &str) -> String {
    use pulldown_cmark::{Event, Options, Parser, html};

    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let opts = Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES;
    let parser = Parser::new_ext(trimmed, opts).filter(|event| {
        !matches!(
            event,
            Event::Html(_) | Event::InlineHtml(_) | Event::FootnoteReference(_)
        )
    });

    let mut out = String::with_capacity(trimmed.len() + 64);
    html::push_html(&mut out, parser);
    out
}

fn format_date(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d").to_string()
}

/// Split a free-form contact string on commas. Each entry is trimmed; emails
/// and URLs become clickable, anything else (e.g. "DM on late.sh") is plain text.
fn parse_contacts(text: &str) -> Vec<ContactItem> {
    text.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| ContactItem {
            value: s.to_string(),
            href: contact_href(s),
        })
        .collect()
}

fn contact_href(value: &str) -> String {
    if value.starts_with("http://") || value.starts_with("https://") {
        return value.to_string();
    }
    if let Some((local, domain)) = value.split_once('@')
        && !local.is_empty()
        && domain.contains('.')
        && !value.contains(char::is_whitespace)
    {
        return format!("mailto:{value}");
    }
    String::new()
}

fn dash_or(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "—".to_string())
}

#[cfg(test)]
mod profiles_test;
