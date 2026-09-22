//! The Profiles page drawn for real at a wide and a narrow size: one person
//! with a card and a project, one with projects only. The wide frame holds
//! the list beside the detail pane; the narrow one stacks them, and `l`
//! brings the detail over the list.

use chrono::{Duration, Utc};
use late_core::models::{
    profile::Profile,
    showcase::Showcase,
    work_profile::{WorkProfile, WorkStatus, WorkType},
};
use ratatui::{Terminal, backend::TestBackend};
use uuid::Uuid;

use super::{DirectoryPageView, draw_directory_page};
use crate::app::chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem};
use crate::app::directory::state::DirectoryState;

fn author(username: &str) -> Profile {
    Profile {
        username: username.to_string(),
        bio: "Builds terminals for a living.".to_string(),
        country: Some("PL".to_string()),
        ide: Some("nvim".to_string()),
        os: Some("nixos".to_string()),
        langs: vec!["rust".to_string(), "elixir".to_string()],
        ..Profile::default()
    }
}

fn card(user_id: Uuid, username: &str, age_minutes: i64) -> WorkFeedItem {
    let at = Utc::now() - Duration::minutes(age_minutes);
    WorkFeedItem {
        profile: WorkProfile {
            id: Uuid::now_v7(),
            user_id,
            slug: "w_abcdef123456".to_string(),
            headline: "Full Stack Developer".to_string(),
            status: WorkStatus::Open,
            work_type: WorkType::Contract,
            location: "Antalya, Turkey".to_string(),
            contact: "mey@example.com".to_string(),
            links: vec!["https://github.com/mey".to_string()],
            skills: ["rust", "kotlin", "typescript", "react", "go", "sql"]
                .map(str::to_string)
                .to_vec(),
            skills_tags: Vec::new(),
            summary: "Can collaborate on freelance projects.".to_string(),
            created: at,
            updated: at,
        },
        author_username: username.to_string(),
        author_profile: Some(author(username)),
    }
}

fn project(user_id: Uuid, username: &str, title: &str, age_minutes: i64) -> ShowcaseFeedItem {
    let at = Utc::now() - Duration::minutes(age_minutes);
    ShowcaseFeedItem {
        showcase: Showcase {
            id: Uuid::now_v7(),
            user_id,
            title: title.to_string(),
            url: format!("https://example.com/{title}"),
            description: format!("{title} is a thing you can run."),
            tags: vec!["rust".to_string(), "tui".to_string()],
            created: at,
            updated: at,
        },
        author_username: username.to_string(),
        author_profile: Some(author(username)),
    }
}

struct Fixture {
    people: Vec<WorkFeedItem>,
    projects: Vec<ShowcaseFeedItem>,
    viewer: Uuid,
}

fn fixture() -> Fixture {
    let viewer = Uuid::now_v7();
    let mey = Uuid::now_v7();
    let renu = Uuid::now_v7();
    Fixture {
        people: vec![card(mey, "meythewitch", 24 * 60)],
        projects: vec![
            project(renu, "Renu", "SalaTUI", 2 * 24 * 60),
            project(mey, "meythewitch", "Darklands", 5 * 24 * 60),
            project(renu, "Renu", "older-thing", 9 * 24 * 60),
        ],
        viewer,
    }
}

/// A shelf state over a pool that is never opened: the page draws the
/// People shelf here and only asks the jobs state whether it is on.
fn jobs_state_for_tests() -> crate::app::jobs::state::JobsState {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy pool");
    crate::app::jobs::state::JobsState::new(crate::app::jobs::svc::JobsService::new(
        db,
        crate::app::ai::svc::AiService::new(false, None),
        crate::test_helpers::test_app_flags_rx(),
    ))
}

fn render(fixture: &Fixture, state: &DirectoryState, width: u16, height: u16) -> Vec<String> {
    render_with_jobs(fixture, state, &jobs_state_for_tests(), width, height)
}

fn render_with_jobs(
    fixture: &Fixture,
    state: &DirectoryState,
    jobs: &crate::app::jobs::state::JobsState,
    width: u16,
    height: u16,
) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            draw_directory_page(
                frame,
                frame.area(),
                DirectoryPageView {
                    directory: state,
                    jobs,
                    viewer_langs: &["rust".to_string()],
                    projects: &fixture.projects,
                    people: &fixture.people,
                    work_marker: Some(Utc::now()),
                    showcase_marker: Some(Utc::now()),
                    current_user_id: fixture.viewer,
                    profile_base_url: "https://late.sh",
                },
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// The cells `from..to` of a rendered line (every glyph here is one cell).
fn cells(line: &str, from: usize, to: usize) -> String {
    line.chars().skip(from).take(to - from).collect()
}

fn row_of(lines: &[String], needle: &str) -> Option<usize> {
    lines.iter().position(|line| line.contains(needle))
}

#[test]
fn a_wide_frame_draws_the_list_beside_the_sectioned_detail() {
    let fixture = fixture();
    let state = DirectoryState::new();
    let lines = render(&fixture, &state, 120, 30);
    let text = lines.join("\n");

    assert_eq!(
        lines[0], " people 2  ·  jobs",
        "the shelf strip with its count:\n{text}"
    );

    // Row one: the person with a card, three lines with fixed roles.
    assert!(
        lines[1].starts_with("▎@meythewitch  ● open")
            && cells(&lines[1], 0, 50).trim_end().ends_with("1d"),
        "line 1 is name, status, age:\n{text}"
    );
    assert!(
        lines[2].starts_with(" Full Stack Developer"),
        "line 2 is the headline:\n{text}"
    );
    assert!(
        lines[3].starts_with(" rust · kotlin · typescript")
            && cells(&lines[3], 0, 50).trim_end().ends_with("1 project"),
        "line 3 is the tags, giving way to the project count:\n{text}"
    );
    assert!(!lines[3].contains("sql"), "a sixth tag is dropped:\n{text}");

    // Row two: projects only, so the count sits on line 1 and the newest
    // title on line 2.
    assert!(lines[5].starts_with(" @Renu  2 projects"), "{text}");
    assert!(lines[6].starts_with(" ↳ SalaTUI"), "{text}");
    assert!(lines[7].starts_with(" rust · tui"), "{text}");

    // The detail pane, right of the rule: header, then the sections in order.
    let header = cells(&lines[1], 50, 120);
    assert!(
        header.contains("@meythewitch · ● open · contract · Antalya, Turkey"),
        "header names status, type, and place:\n{text}"
    );
    let card = row_of(&lines, "▸ CARD").expect("card section, focused first");
    let about = row_of(&lines, "  ABOUT").expect("about section");
    let projects = row_of(&lines, "  PROJECTS").expect("projects section");
    let fetch = row_of(&lines, "late.fetch").expect("late.fetch last");
    assert!(
        card < about && about < projects && projects < fetch,
        "section order:\n{text}"
    );
    assert!(lines[card + 1].contains("Full Stack Developer"), "{text}");
    assert!(row_of(&lines, "skills   rust · kotlin").is_some(), "{text}");
    assert!(
        row_of(&lines, "links    github.com/mey").is_some(),
        "{text}"
    );
    assert!(
        row_of(&lines, "page     late.sh/profiles/w_abcdef123456").is_some(),
        "{text}"
    );
    assert!(
        lines[fetch + 1].contains("PL  ·  rust · elixir  ·  ide nvim  ·  os nixos"),
        "{text}"
    );
    assert!(
        lines[lines.len() - 1].contains("Space jobs"),
        "the footer teaches the shelf key:\n{text}"
    );
}

#[test]
fn a_narrow_frame_stacks_and_l_opens_the_detail_over_the_list() {
    let fixture = fixture();
    let mut state = DirectoryState::new();
    let lines = render(&fixture, &state, 80, 20);
    let text = lines.join("\n");
    assert!(state.narrow(), "the draw records the stacked layout");
    assert!(lines[1].starts_with("▎@meythewitch  ● open"), "{text}");
    assert!(
        row_of(&lines, "▸ CARD").is_none(),
        "no detail beside the list:\n{text}"
    );
    assert!(
        lines[2].starts_with(" Full Stack Developer") && !lines[3].starts_with(" rust"),
        "under 24 rows the tag line goes:\n{text}"
    );
    assert!(lines[lines.len() - 1].contains("l open"), "{text}");

    state.open_detail();
    let lines = render(&fixture, &state, 80, 20);
    let text = lines.join("\n");
    assert!(
        row_of(&lines, "@meythewitch · ● open · contract").is_some(),
        "{text}"
    );
    assert!(
        row_of(&lines, "▸ CARD").is_some(),
        "the detail fills the frame:\n{text}"
    );
    assert!(lines[lines.len() - 1].contains("h back"), "{text}");
}

#[test]
fn the_jobs_shelf_lists_postings_and_the_for_me_filter_keeps_the_viewers_tags() {
    use crate::app::jobs::state_test::posting;

    let fixture = fixture();
    let mut state = DirectoryState::new();
    state.toggle_shelf();

    // Before the replica has read the shelf, and once it has read nothing.
    let mut jobs = jobs_state_for_tests();
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    assert_eq!(lines[0], " people 2  ·  jobs");
    assert!(
        row_of(&lines, "Reading the shelf…").is_some(),
        "{}",
        lines.join("\n")
    );
    jobs.loaded = true;
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    assert_eq!(lines[0], " people 2  ·  jobs 0");
    assert!(
        row_of(&lines, "No postings on the shelf yet.").is_some(),
        "{}",
        lines.join("\n")
    );

    // Two postings: the list on the left, the first one's card on the right.
    jobs.items = vec![
        posting("Acme", &["rust", "postgres"]),
        posting("Gopher", &["go"]),
    ];
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    let text = lines.join("\n");
    assert_eq!(lines[0], " people 2  ·  jobs 2");
    let acme = row_of(&lines, "Acme · Backend Engineer").expect("acme row");
    let gopher = row_of(&lines, "Gopher · Backend Engineer").expect("gopher row");
    assert!(acme < gopher, "{text}");
    assert!(lines[acme].starts_with("▎"), "selected gutter:\n{text}");
    assert!(row_of(&lines, "remote · EU · €80k").is_some(), "{text}");
    assert!(row_of(&lines, "rust · postgres").is_some(), "{text}");
    assert!(row_of(&lines, "via weworkremotely.com").is_some(), "{text}");
    // The detail pane: the stack, the link, the excerpt.
    assert!(
        row_of(&lines, "stack    rust · postgres").is_some(),
        "{text}"
    );
    assert!(
        row_of(&lines, "link     acme.example/jobs/1").is_some(),
        "{text}"
    );
    assert!(row_of(&lines, "Builds the backend.").is_some(), "{text}");
    assert!(lines[lines.len() - 1].contains("Space people"), "{text}");

    // `/`: the viewer's langs say rust, so only Acme stays.
    jobs.toggle_for_me();
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    let text = lines.join("\n");
    assert_eq!(lines[0], " people 2  ·  jobs 1");
    assert!(
        row_of(&lines, "Acme · Backend Engineer").is_some(),
        "{text}"
    );
    assert!(
        row_of(&lines, "Gopher · Backend Engineer").is_none(),
        "{text}"
    );
    assert!(lines[lines.len() - 1].contains("for me"), "{text}");
}
