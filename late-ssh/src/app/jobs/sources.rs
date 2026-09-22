//! The three feeds, parsed into [`FetchedPosting`]s. Pure: the bytes come
//! in, rows-to-be come out, and `svc.rs` does the fetching and the
//! writing. Each source is an API or feed published to be read; nothing
//! here reads an HTML page.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use late_core::models::job_posting::{FetchedPosting, JobSource, RemoteKind};

/// The source text kept for the read, in characters. Past this a posting
/// is boilerplate; the role, stack, and pay are near the top.
pub(crate) const RAW_LIMIT: usize = 4_000;

pub(crate) const HN_WHOISHIRING_URL: &str =
    "https://hacker-news.firebaseio.com/v0/user/whoishiring.json";
pub(crate) const WWR_RSS_URL: &str = "https://weworkremotely.com/remote-jobs.rss";
pub(crate) const JOBICY_API_URL: &str = "https://jobicy.com/api/v2/remote-jobs";

/// The title every monthly thread starts with.
const HN_HIRING_TITLE: &str = "Ask HN: Who is hiring?";
/// How many of `whoishiring`'s newest submissions are opened to find the
/// month's thread: the account posts three threads a month.
pub(crate) const HN_SUBMISSIONS_TO_CHECK: usize = 6;

pub(crate) fn hn_item_url(id: i64) -> String {
    format!("https://hacker-news.firebaseio.com/v0/item/{id}.json")
}

/// The comment on HN, the card's link when the post names no URL.
pub(crate) fn hn_comment_url(id: i64) -> String {
    format!("https://news.ycombinator.com/item?id={id}")
}

/// The Jobicy tag queries, one request each. The API's tag match is a
/// substring one (`rust` returns "trust"), so every row is rechecked on
/// word boundaries against the words beside its query.
pub(crate) const JOBICY_TAGS: &[(&str, &[&str])] = &[
    ("rust", &["rust"]),
    ("golang", &["go", "golang"]),
    ("elixir", &["elixir"]),
];

#[derive(serde::Deserialize)]
struct HnUser {
    #[serde(default)]
    submitted: Vec<i64>,
}

#[derive(serde::Deserialize)]
struct HnItem {
    id: i64,
    #[serde(default)]
    deleted: bool,
    #[serde(default)]
    dead: bool,
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    time: i64,
    #[serde(default)]
    kids: Vec<i64>,
}

/// The ids `whoishiring` submitted, newest first.
pub(crate) fn parse_hn_submitted(json: &str) -> Result<Vec<i64>> {
    let user: HnUser = serde_json::from_str(json).context("parsing the whoishiring user")?;
    Ok(user.submitted)
}

/// The thread's top-level comment ids when `json` is a "Who is hiring?"
/// thread, `None` for the account's other threads.
pub(crate) fn parse_hn_hiring_thread(json: &str) -> Result<Option<Vec<i64>>> {
    let item: HnItem = serde_json::from_str(json).context("parsing an hn submission")?;
    if item.title.starts_with(HN_HIRING_TITLE) {
        Ok(Some(item.kids))
    } else {
        Ok(None)
    }
}

/// One top-level comment as a posting; `None` for a deleted or dead one,
/// or one with no text. Remote kind is left for the read.
pub(crate) fn parse_hn_comment(json: &str) -> Result<Option<FetchedPosting>> {
    let item: HnItem = serde_json::from_str(json).context("parsing an hn comment")?;
    if item.deleted || item.dead || item.text.trim().is_empty() {
        return Ok(None);
    }
    let posted_at = DateTime::from_timestamp(item.time, 0)
        .with_context(|| format!("hn comment {} has an unreadable time", item.id))?;
    Ok(Some(FetchedPosting {
        source: JobSource::Hn,
        external_id: item.id.to_string(),
        url: hn_comment_url(item.id),
        raw: cut(&html_to_text(&item.text), RAW_LIMIT),
        posted_at,
        remote_kind: None,
        regions: Vec::new(),
        dropped: false,
    }))
}

/// Every `<item>` of the WWR feed. The region tag is the remote scope,
/// so the read never has to guess it.
pub(crate) fn parse_wwr_rss(xml: &str) -> Vec<FetchedPosting> {
    split_elements(xml, "item")
        .into_iter()
        .filter_map(parse_wwr_item)
        .collect()
}

fn parse_wwr_item(item: &str) -> Option<FetchedPosting> {
    let link = first_tag(item, "link")?;
    let external_id = first_tag(item, "guid").unwrap_or_else(|| link.clone());
    let title = first_tag(item, "title").unwrap_or_default();
    let region = first_tag(item, "region").unwrap_or_default();
    let kind = first_tag(item, "type").unwrap_or_default();
    let category = first_tag(item, "category").unwrap_or_default();
    let description = first_tag(item, "description").unwrap_or_default();
    let posted_at = first_tag(item, "pubDate").and_then(|value| parse_feed_date(&value))?;
    let (remote_kind, regions) = wwr_scope(&region);
    let raw = format!(
        "{title}\nRegion: {region}\nType: {kind}\nCategory: {category}\n\n{}",
        cut(&description, RAW_LIMIT)
    );
    Some(FetchedPosting {
        source: JobSource::Wwr,
        external_id,
        url: link,
        raw,
        posted_at,
        remote_kind: Some(remote_kind),
        regions,
        dropped: false,
    })
}

/// WWR's region tag: "Anywhere in the World" is worldwide, anything else
/// ("USA Only", "Europe Only", "Americas Only") is a region list.
fn wwr_scope(region: &str) -> (RemoteKind, Vec<String>) {
    let region = region.trim();
    if region.eq_ignore_ascii_case("anywhere in the world") || region.is_empty() {
        (RemoteKind::Worldwide, Vec::new())
    } else {
        let name = region
            .trim_end_matches(" Only")
            .trim_end_matches(" only")
            .to_string();
        (RemoteKind::Regions, vec![name])
    }
}

#[derive(serde::Deserialize)]
struct JobicyPage {
    #[serde(default)]
    jobs: Vec<JobicyJob>,
}

#[derive(serde::Deserialize)]
struct JobicyJob {
    id: serde_json::Value,
    #[serde(default)]
    url: String,
    #[serde(rename = "jobTitle", default)]
    title: String,
    #[serde(rename = "companyName", default)]
    company: String,
    #[serde(rename = "jobGeo", default)]
    geo: String,
    #[serde(rename = "jobLevel", default)]
    level: String,
    #[serde(rename = "jobType", default)]
    kind: serde_json::Value,
    #[serde(rename = "jobExcerpt", default)]
    excerpt: String,
    #[serde(rename = "jobDescription", default)]
    description: String,
    #[serde(rename = "pubDate", default)]
    pub_date: String,
}

/// One Jobicy tag query's page. A row whose text does not carry one of
/// `needles` as a whole word is the substring match's false positive and
/// comes back `dropped`, so it is tombstoned rather than read.
pub(crate) fn parse_jobicy(json: &str, needles: &[&str]) -> Result<Vec<FetchedPosting>> {
    let page: JobicyPage = serde_json::from_str(json).context("parsing a jobicy page")?;
    let mut out = Vec::new();
    for job in page.jobs {
        let external_id = match &job.id {
            serde_json::Value::String(id) => id.clone(),
            serde_json::Value::Number(id) => id.to_string(),
            _ => continue,
        };
        let Some(posted_at) = parse_feed_date(&job.pub_date) else {
            continue;
        };
        if job.url.is_empty() {
            continue;
        }
        let description = html_to_text(&job.description);
        let haystack = format!("{} {} {}", job.title, job.excerpt, description);
        let dropped = !needles.iter().any(|needle| word_match(&haystack, needle));
        let (remote_kind, regions) = jobicy_scope(&job.geo);
        let kind = match &job.kind {
            serde_json::Value::Array(kinds) => kinds
                .iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            serde_json::Value::String(kind) => kind.clone(),
            _ => String::new(),
        };
        let raw = format!(
            "{}\nCompany: {}\nRegion: {}\nLevel: {}\nType: {kind}\n\n{}",
            job.title,
            job.company,
            job.geo,
            job.level,
            cut(&description, RAW_LIMIT)
        );
        out.push(FetchedPosting {
            source: JobSource::Jobicy,
            external_id,
            url: job.url,
            raw,
            posted_at,
            remote_kind: Some(remote_kind),
            regions,
            dropped,
        });
    }
    Ok(out)
}

/// Jobicy's `jobGeo`: "Anywhere" is worldwide, otherwise a comma list of
/// regions or countries.
fn jobicy_scope(geo: &str) -> (RemoteKind, Vec<String>) {
    let geo = geo.trim();
    if geo.eq_ignore_ascii_case("anywhere") || geo.is_empty() {
        return (RemoteKind::Worldwide, Vec::new());
    }
    let regions = geo
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    (RemoteKind::Regions, regions)
}

/// Whether `word` appears in `text` on its own, case-insensitively: `rust`
/// in "Rust engineer" but not in "trust".
pub(crate) fn word_match(text: &str, word: &str) -> bool {
    let text = text.to_ascii_lowercase();
    let word = word.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = text[from..].find(&word) {
        let start = from + at;
        let end = start + word.len();
        let before_ok = start == 0
            || !text[..start]
                .chars()
                .next_back()
                .is_some_and(is_word_char);
        let after_ok = end == text.len() || !text[end..].chars().next().is_some_and(is_word_char);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// HTML as the feeds carry it, to plain text: block tags become line
/// breaks, the rest is stripped, entities are decoded (including the
/// numeric ones HN uses for slashes), runs of blank lines are folded.
pub(crate) fn html_to_text(input: &str) -> String {
    let with_breaks = input
        .replace("<p>", "\n")
        .replace("<P>", "\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</li>", "\n")
        .replace("</div>", "\n")
        .replace("</h1>", "\n")
        .replace("</h2>", "\n")
        .replace("</h3>", "\n");
    let stripped = decode_entities(&strip_tags(&with_breaks));
    // Feeds double-encode: the description is `&lt;p&gt;` text, which the
    // first pass turned into real tags.
    let stripped = decode_entities(&strip_tags(&stripped));
    let mut out = String::with_capacity(stripped.len());
    let mut blank_run = 0;
    for line in stripped.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank_run += 1;
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        out.push_str(&line);
        out.push('\n');
    }
    out.trim().to_string()
}

fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';').filter(|end| *end <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn cut(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    input.chars().take(max_chars).collect()
}

fn parse_feed_date(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    DateTime::parse_from_rfc2822(value)
        .or_else(|_| DateTime::parse_from_rfc3339(value))
        .map(|at| at.with_timezone(&Utc))
        .ok()
}

/// The inner text of every `<tag>` element, in document order.
fn split_elements<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = xml;
    let close = format!("</{tag}>");
    while let Some(start) = find_open_tag(rest, tag) {
        let after_start = &rest[start..];
        let Some(open_end) = after_start.find('>') else {
            break;
        };
        let content_start = start + open_end + 1;
        let Some(close_at) = rest[content_start..].find(&close) else {
            break;
        };
        let close_start = content_start + close_at;
        out.push(&rest[content_start..close_start]);
        rest = &rest[close_start + close.len()..];
    }
    out
}

/// The first `<tag>`'s text with CDATA unwrapped, tags stripped, and
/// entities decoded.
fn first_tag(xml: &str, tag: &str) -> Option<String> {
    let start = find_open_tag(xml, tag)?;
    let after_start = &xml[start..];
    let open_end = after_start.find('>')?;
    let content_start = start + open_end + 1;
    let close = format!("</{tag}>");
    let close_start = xml[content_start..].find(&close)? + content_start;
    let inner = xml[content_start..close_start].trim();
    let inner = inner
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
        .unwrap_or(inner);
    Some(html_to_text(inner))
}

fn find_open_tag(xml: &str, tag: &str) -> Option<usize> {
    let plain = xml.find(&format!("<{tag}>"));
    let with_attrs = xml.find(&format!("<{tag} "));
    match (plain, with_attrs) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}
