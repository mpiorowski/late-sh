//! The parsers over trimmed real pulls (`fixtures/`, cut from the probe
//! of 2026-09-22): what a fetch turns into rows, and what it drops.

use late_core::models::job_posting::{JobSource, RemoteKind};

use super::sources::{
    html_to_text, parse_hn_comment, parse_hn_hiring_thread, parse_hn_submitted, parse_jobicy,
    parse_wwr_rss, word_match,
};

const HN_ITEMS: &str = include_str!("fixtures/hn_items.json");
const WWR: &str = include_str!("fixtures/wwr.xml");
const JOBICY_RUST: &str = include_str!("fixtures/jobicy_rust.json");

fn hn_items() -> Vec<String> {
    let items: Vec<serde_json::Value> = serde_json::from_str(HN_ITEMS).expect("fixture");
    items.into_iter().map(|item| item.to_string()).collect()
}

#[test]
fn hn_comments_become_postings_and_deleted_or_dead_ones_do_not() {
    let items = hn_items();
    let postings: Vec<_> = items
        .iter()
        .map(|json| parse_hn_comment(json).expect("parse"))
        .collect();
    // Ten live posts, one deleted, one dead.
    assert_eq!(postings.len(), 12);
    assert_eq!(postings.iter().filter(|p| p.is_some()).count(), 10);
    assert!(postings[10].is_none(), "deleted");
    assert!(postings[11].is_none(), "dead");

    let quobyte = postings
        .iter()
        .flatten()
        .find(|p| p.raw.starts_with("QUOBYTE"))
        .expect("the quobyte post");
    assert_eq!(quobyte.source, JobSource::Hn);
    assert_eq!(quobyte.external_id, "49523712");
    assert_eq!(quobyte.url, "https://news.ycombinator.com/item?id=49523712");
    assert_eq!(quobyte.remote_kind, None, "the read decides for hn");
    assert!(!quobyte.dropped);
    // Entities decoded, tags gone, paragraphs kept as lines.
    assert!(quobyte.raw.contains("https://www.quobyte.com/"));
    assert!(!quobyte.raw.contains("&#x2F;"));
    assert!(!quobyte.raw.contains("<p>"));
    assert!(quobyte.raw.contains("\nAt Quobyte we are working"));
    assert_eq!(quobyte.posted_at.to_rfc3339(), "2026-09-01T15:53:18+00:00");
}

#[test]
fn the_hiring_thread_is_found_by_title_among_the_accounts_submissions() {
    assert_eq!(
        parse_hn_submitted(r#"{"id":"whoishiring","submitted":[49522897,49522896,49522895]}"#)
            .expect("parse"),
        vec![49522897, 49522896, 49522895]
    );
    let hiring = r#"{"id":49522897,"title":"Ask HN: Who is hiring? (September 2026)","kids":[1,2,3],"time":1756742400,"type":"story"}"#;
    assert_eq!(
        parse_hn_hiring_thread(hiring).expect("parse"),
        Some(vec![1, 2, 3])
    );
    let wants = r#"{"id":49522896,"title":"Ask HN: Who wants to be hired? (September 2026)","kids":[9],"time":1756742400,"type":"story"}"#;
    assert_eq!(parse_hn_hiring_thread(wants).expect("parse"), None);
    let freelancer = r#"{"id":49522895,"title":"Ask HN: Freelancer? Seeking freelancer? (September 2026)","kids":[],"time":1756742400,"type":"story"}"#;
    assert_eq!(parse_hn_hiring_thread(freelancer).expect("parse"), None);
}

#[test]
fn wwr_items_carry_their_region_and_land_pending() {
    let postings = parse_wwr_rss(WWR);
    assert_eq!(postings.len(), 3);
    let first = &postings[0];
    assert_eq!(first.source, JobSource::Wwr);
    assert_eq!(
        first.external_id,
        "https://weworkremotely.com/remote-jobs/anthropic-anthropic-fellows-program-the-anthropic-institute-economics-policy"
    );
    assert_eq!(first.url, first.external_id);
    assert_eq!(first.remote_kind, Some(RemoteKind::Worldwide));
    assert!(first.regions.is_empty());
    assert!(!first.dropped);
    assert!(
        first
            .raw
            .starts_with("Anthropic: Anthropic Fellows Program")
    );
    assert!(first.raw.contains("Region: Anywhere in the World"));
    assert!(first.raw.contains("Type: Full-Time"));
    // The description was entity-encoded HTML; it reads as text now.
    assert!(first.raw.contains("Headquarters: London, UK"));
    assert!(!first.raw.contains("&lt;p&gt;"));
    assert!(!first.raw.contains("<strong>"));
    assert_eq!(first.posted_at.to_rfc3339(), "2026-09-22T07:30:50+00:00");
}

#[test]
fn jobicy_rows_are_rechecked_on_word_boundaries() {
    let postings = parse_jobicy(JOBICY_RUST, "rust", &["rust"]).expect("parse");
    assert_eq!(postings.len(), 3);
    let canonical = &postings[0];
    assert_eq!(canonical.source, JobSource::Jobicy);
    assert_eq!(canonical.external_id, "151197");
    assert_eq!(
        canonical.url,
        "https://jobicy.com/jobs/151197-c-rust-graphics-and-windowing-system-software-engineer-mir"
    );
    assert_eq!(canonical.remote_kind, Some(RemoteKind::Regions));
    assert_eq!(canonical.regions, vec!["APAC", "EMEA"]);
    assert!(!canonical.dropped);
    assert!(canonical.raw.starts_with(
        "C++/Rust Graphics and Windowing System Software Engineer - Mir\nCompany: Canonical\n"
    ));
    assert_eq!(postings[1].regions, vec!["USA"]);
    assert!(!postings[1].dropped);
    // "trust", not "rust": a tombstone, never read.
    assert!(postings[2].dropped, "{}", postings[2].raw);
    assert_eq!(postings[2].regions, vec!["Estonia", "Spain"]);

    // The JS query over the rust page: nothing here says JavaScript, or
    // React in a title, so every row is a tombstone.
    let js = parse_jobicy(
        JOBICY_RUST,
        "javascript",
        &["javascript", "js", "node", "react"],
    )
    .expect("parse");
    assert!(js.iter().all(|p| p.dropped), "no js on the rust page");

    // The same page rechecked for the golang query: only CertiK, whose
    // text says "Golang", survives; "go" as a verb in a description does
    // not count, only "Go" in a title would.
    let golang = parse_jobicy(JOBICY_RUST, "golang", &["go", "golang"]).expect("parse");
    let kept: Vec<&str> = golang
        .iter()
        .filter(|p| !p.dropped)
        .map(|p| p.external_id.as_str())
        .collect();
    assert_eq!(kept, vec!["152447"]);
}

#[test]
fn word_match_is_whole_word_and_case_blind() {
    assert!(word_match("Senior Rust Engineer", "rust"));
    assert!(word_match("rust/go backend", "go"));
    assert!(word_match("(Rust)", "rust"));
    assert!(!word_match("we build trust", "rust"));
    assert!(!word_match("Google Cloud", "go"));
    assert!(!word_match("golang", "go"));
    assert!(word_match("golang", "golang"));
}

#[test]
fn html_becomes_plain_lines() {
    assert_eq!(
        html_to_text(
            "Acme | Rust | REMOTE<p>We build &quot;things&quot; at <a href=\"https:&#x2F;&#x2F;acme.io\">https:&#x2F;&#x2F;acme.io</a><p><p>Pay: 100k &amp; equity"
        ),
        "Acme | Rust | REMOTE\nWe build \"things\" at https://acme.io\n\nPay: 100k & equity"
    );
    assert_eq!(
        html_to_text("&lt;p&gt;double&lt;/p&gt; encoded"),
        "double encoded"
    );
    assert_eq!(html_to_text("a &unknown; b & c"), "a &unknown; b & c");
}
