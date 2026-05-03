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
use late_core::models::{
    profile::Profile, showcase::Showcase, user::User, work_profile::WorkProfile,
};

use crate::{AppState, error::AppError, metrics};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/profiles", get(index_handler))
        .route("/profiles/{slug}", get(handler))
}

#[derive(Template)]
#[template(path = "pages/profiles/page.html")]
struct Page {
    headline: String,
    username: String,
    status_id: &'static str,
    status_label: &'static str,
    work_type: String,
    location: String,
    skills: Vec<String>,
    links: Vec<String>,
    summary_paragraphs: Vec<String>,
    slug: String,
    created: String,
    updated: String,
    show_bio: bool,
    bio_paragraphs: Vec<String>,
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
    summary_preview: String,
    updated: String,
}

const PROFILE_LIST_LIMIT: i64 = 100;
const SUMMARY_PREVIEW_CHARS: usize = 180;

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

    let bio_paragraphs = if work.include_bio {
        split_paragraphs(&user_profile.bio)
    } else {
        Vec::new()
    };
    let show_bio = work.include_bio && !bio_paragraphs.is_empty();

    let showcases = if work.include_showcases {
        let entries = Showcase::list_by_user_id(&client, work.user_id)
            .await
            .context("failed to load author showcases")?;
        entries
            .into_iter()
            .map(|s| ShowcaseItem {
                title: s.title,
                url: s.url,
                description_paragraphs: split_paragraphs(&s.description),
                tags: s.tags,
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let show_showcases = work.include_showcases && !showcases.is_empty();

    let page = Page {
        headline: work.headline,
        username: user_profile.username.clone(),
        status_id: status_id(&work.status),
        status_label: status_label(&work.status),
        work_type: work.work_type,
        location: work.location,
        skills: work.skills,
        links: work.links,
        summary_paragraphs: split_paragraphs(&work.summary),
        slug: work.slug.clone(),
        created: format_date(work.created),
        updated: format_date(work.updated),
        show_bio,
        bio_paragraphs,
        show_late_fetch: work.include_late_fetch,
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

#[tracing::instrument(skip(state))]
async fn index_handler(State(state): State<AppState>) -> Result<Response, AppError> {
    metrics::record_page_view("profiles_index", false);

    let client = state
        .db
        .get()
        .await
        .context("failed to get db client for profiles index")?;

    let mut profiles = WorkProfile::list_recent(&client, PROFILE_LIST_LIMIT)
        .await
        .context("failed to list work profiles")?;
    // Open first, then casual, then not-looking. Within each bucket the model
    // already orders by updated DESC, so we just need a stable sort_by_key.
    profiles.sort_by_key(|p| status_priority(&p.status));

    let user_ids: Vec<_> = profiles
        .iter()
        .map(|p| p.user_id)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let usernames = User::list_usernames_by_ids(&client, &user_ids)
        .await
        .context("failed to load author usernames")?;

    let mut open_count = 0;
    let mut casual_count = 0;
    let mut closed_count = 0;
    let items: Vec<IndexItem> = profiles
        .into_iter()
        .map(|p| {
            match p.status.as_str() {
                "open" => open_count += 1,
                "casual" => casual_count += 1,
                _ => closed_count += 1,
            }
            IndexItem {
                username: usernames
                    .get(&p.user_id)
                    .cloned()
                    .unwrap_or_else(|| p.user_id.to_string()[..8].to_string()),
                status_id: status_id(&p.status),
                status_label: status_label(&p.status),
                work_type: p.work_type,
                location: p.location,
                skills: p.skills,
                summary_preview: summary_preview(&p.summary, SUMMARY_PREVIEW_CHARS),
                updated: format_date(p.updated),
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

fn status_priority(status: &str) -> u8 {
    match status {
        "open" => 0,
        "casual" => 1,
        "not-looking" => 2,
        _ => 3,
    }
}

fn summary_preview(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut out: String = collapsed.chars().take(max_chars).collect();
    if let Some(idx) = out.rfind(' ') {
        out.truncate(idx);
    }
    out.push('…');
    out
}

fn status_id(status: &str) -> &'static str {
    match status {
        "open" => "open",
        "casual" => "casual",
        "not-looking" => "not-looking",
        _ => "unknown",
    }
}

fn status_label(status: &str) -> &'static str {
    match status {
        "open" => "open to work",
        "casual" => "casually listening",
        "not-looking" => "not looking",
        _ => "unknown",
    }
}

fn split_paragraphs(text: &str) -> Vec<String> {
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn format_date(ts: DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d").to_string()
}

fn dash_or(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| "—".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        dash_or, split_paragraphs, status_id, status_label, status_priority, summary_preview,
    };

    #[test]
    fn paragraphs_drop_empty_and_trim() {
        let para = split_paragraphs("hello\n\n  world  \n");
        assert_eq!(para, vec!["hello", "world"]);
    }

    #[test]
    fn status_helpers_map_known_values() {
        assert_eq!(status_id("open"), "open");
        assert_eq!(status_id("nope"), "unknown");
        assert_eq!(status_label("not-looking"), "not looking");
    }

    #[test]
    fn dash_or_handles_blank() {
        assert_eq!(dash_or(None), "—");
        assert_eq!(dash_or(Some("   ")), "—");
        assert_eq!(dash_or(Some(" rust ")), "rust");
    }

    #[test]
    fn status_priority_orders_open_first() {
        let mut statuses = vec!["not-looking", "open", "casual", "weird"];
        statuses.sort_by_key(|s| status_priority(s));
        assert_eq!(statuses, vec!["open", "casual", "not-looking", "weird"]);
    }

    #[test]
    fn summary_preview_collapses_whitespace_and_truncates_on_word() {
        assert_eq!(
            summary_preview("hello\n\n  there  friend", 80),
            "hello there friend"
        );
        let preview = summary_preview("alpha beta gamma delta epsilon zeta", 18);
        assert_eq!(preview, "alpha beta gamma…");
    }
}
