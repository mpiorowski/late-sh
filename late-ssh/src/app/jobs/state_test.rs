use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::job_posting::{JobPosting, JobSource, JobStatus, RemoteKind};
use late_core::models::work_profile::{WorkProfile, WorkStatus, WorkType};
use uuid::Uuid;

use super::state::{
    JobsCommand, match_line, matches, parse_jobs_command, scope_label, viewer_tags, wants_matches,
};

pub(crate) fn posting(company: &str, tags: &[&str]) -> JobPosting {
    let at = Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap();
    JobPosting {
        id: Uuid::now_v7(),
        source: JobSource::Wwr,
        external_id: company.to_ascii_lowercase(),
        status: JobStatus::Active,
        read_attempts: 0,
        url: format!("https://{}.example/jobs/1", company.to_ascii_lowercase()),
        company: company.to_string(),
        title: "Backend Engineer".to_string(),
        remote_kind: Some(RemoteKind::Regions),
        regions: vec!["EU".to_string()],
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        pay: "€80k".to_string(),
        excerpt: "Builds the backend.".to_string(),
        raw: String::new(),
        posted_at: at,
        released_on: Some(NaiveDate::from_ymd_opt(2026, 9, 21).unwrap()),
        first_seen: at,
        last_seen: at,
        posted_by: None,
    }
}

fn card(status: WorkStatus, skills_tags: &[&str]) -> WorkProfile {
    let at = Utc::now();
    WorkProfile {
        id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        slug: "w_abc".to_string(),
        headline: "Engineer".to_string(),
        status,
        work_type: WorkType::Any,
        location: String::new(),
        contact: String::new(),
        links: Vec::new(),
        skills: skills_tags.iter().map(|tag| tag.to_string()).collect(),
        skills_tags: skills_tags.iter().map(|tag| tag.to_string()).collect(),
        summary: String::new(),
        created: at,
        updated: at,
    }
}

#[test]
fn the_viewer_is_matched_on_card_tags_and_langs_through_the_vocabulary() {
    let card = card(WorkStatus::Open, &["rust", "postgres"]);
    // Langs come as the profile spells them; `Golang` folds to `go`, an
    // unknown word is left out, a repeat is one tag.
    let langs = [
        "Golang".to_string(),
        "rust".to_string(),
        "brainfuck".to_string(),
    ];
    assert_eq!(
        viewer_tags(Some(&card), &langs),
        vec!["rust", "postgres", "go"]
    );
    assert_eq!(viewer_tags(None, &langs), vec!["go", "rust"]);
    assert!(viewer_tags(None, &[]).is_empty());
}

#[test]
fn matches_rank_by_overlap_then_keep_shelf_order_and_skip_strangers() {
    let items = vec![
        posting("Newest", &["go"]),
        posting("Both", &["rust", "postgres", "docker"]),
        posting("Stranger", &["php", "laravel"]),
        posting("Older", &["rust"]),
    ];
    let tags = ["rust".to_string(), "postgres".to_string(), "go".to_string()];
    let names: Vec<&str> = matches(&items, &tags, 10)
        .into_iter()
        .map(|p| p.company.as_str())
        .collect();
    assert_eq!(names, vec!["Both", "Newest", "Older"]);
    let top: Vec<&str> = matches(&items, &tags, 1)
        .into_iter()
        .map(|p| p.company.as_str())
        .collect();
    assert_eq!(top, vec!["Both"]);
    assert!(matches(&items, &[], 10).is_empty());
}

#[test]
fn a_match_reads_as_one_line() {
    let mut item = posting("Acme", &["rust", "postgres", "docker", "aws", "grpc"]);
    assert_eq!(
        match_line(&item, 3),
        "Acme · Backend Engineer · remote · EU · rust, postgres, docker · €80k"
    );
    item.remote_kind = Some(RemoteKind::Worldwide);
    item.regions.clear();
    item.pay.clear();
    item.tags.clear();
    assert_eq!(scope_label(&item), "remote worldwide");
    assert_eq!(
        match_line(&item, 3),
        "Acme · Backend Engineer · remote worldwide"
    );
    item.remote_kind = Some(RemoteKind::Hybrid);
    item.regions = vec!["Berlin".to_string()];
    assert_eq!(scope_label(&item), "hybrid · Berlin");
}

#[test]
fn only_open_and_casual_cards_want_matches() {
    assert!(wants_matches(WorkStatus::Open));
    assert!(wants_matches(WorkStatus::Casual));
    assert!(!wants_matches(WorkStatus::NotLooking));
}

#[test]
fn the_jobs_command_parses_its_words_and_gates_the_press() {
    assert_eq!(parse_jobs_command("/jobs"), Some(Some(JobsCommand::Open)));
    assert_eq!(
        parse_jobs_command("  /jobs pull "),
        Some(Some(JobsCommand::Pull))
    );
    assert_eq!(
        parse_jobs_command("/jobs release"),
        Some(Some(JobsCommand::Release))
    );
    assert_eq!(parse_jobs_command("/jobs on"), Some(Some(JobsCommand::On)));
    assert_eq!(
        parse_jobs_command("/jobs off"),
        Some(Some(JobsCommand::Off))
    );
    assert_eq!(
        parse_jobs_command("/jobs post"),
        Some(Some(JobsCommand::Post))
    );
    assert!(!JobsCommand::Post.admin_only());
    assert_eq!(parse_jobs_command("/jobs now"), Some(None));
    assert_eq!(parse_jobs_command("/jobsboard"), None);
    assert_eq!(parse_jobs_command("hello /jobs"), None);
    assert!(!JobsCommand::Open.admin_only());
    for command in [
        JobsCommand::Pull,
        JobsCommand::Release,
        JobsCommand::On,
        JobsCommand::Off,
    ] {
        assert!(command.admin_only(), "{command:?}");
    }
}
