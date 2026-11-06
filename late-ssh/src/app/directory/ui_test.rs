//! The Profiles page drawn for real at a wide and a narrow size: one person
//! with a card and a project, one with projects only. The wide frame holds
//! the list beside the detail pane; the narrow one stacks them, and `l`
//! brings the detail over the list.

use chrono::{DateTime, Duration, Utc};
use late_core::models::{
    profile::Profile,
    showcase::Showcase,
    work_profile::{WorkProfile, WorkStatus, WorkType},
};
use ratatui::{Terminal, backend::TestBackend};
use std::collections::HashSet;
use uuid::Uuid;

use super::{DirectoryPageView, draw_directory_page};
use crate::app::chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem};
use crate::app::directory::state::{DirectoryState, Shelf};
use crate::app::usermap::state::UserMapState;

/// An author who last logged in at `last_seen`.
fn author(username: &str, last_seen: DateTime<Utc>) -> Profile {
    Profile {
        username: username.to_string(),
        last_seen: Some(last_seen),
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
        author_profile: Some(author(username, at)),
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
        author_profile: Some(author(username, at)),
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
/// People shelf here and never reads the jobs shelf.
fn jobs_state_for_tests() -> crate::app::jobs::state::JobsState {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy pool");
    crate::app::jobs::state::JobsState::new(crate::app::jobs::svc::JobsService::new(
        db,
        crate::app::ai::svc::AiService::new(false, None),
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
    let mut usermap = UserMapState::default();
    terminal
        .draw(|frame| {
            draw_directory_page(
                frame,
                frame.area(),
                DirectoryPageView {
                    directory: state,
                    jobs,
                    usermap: &mut usermap,
                    viewer_langs: &["rust".to_string()],
                    viewer_has_country: true,
                    projects: &fixture.projects,
                    people: &fixture.people,
                    work_marker: Some(Utc::now()),
                    showcase_marker: Some(Utc::now()),
                    current_user_id: fixture.viewer,
                    can_moderate: false,
                    can_retract_jobs: false,
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
    let state = people_state();
    let lines = render(&fixture, &state, 120, 30);
    let text = lines.join("\n");

    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs")
            && lines[0].ends_with("everyone · mine · search"),
        "the section row, counts on the left, filters on the right:\n{text}"
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
    let header = cells(&lines[2], 50, 120);
    assert!(
        header.contains("@meythewitch · ● open · contract · Antalya, Turkey"),
        "header names status, type, and place:\n{text}"
    );
    let card = row_of(&lines, "▸ CARD").expect("card section, focused first");
    let about = row_of(&lines, "  ABOUT").expect("about section");
    let projects = row_of(&lines, "  PROJECTS").expect("projects section");
    let fetch = row_of(&lines, "  LATE.FETCH").expect("late.fetch last");
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
    // late.fetch: identity on the left, setup on the right, paired by row.
    let country = row_of(&lines, "country  PL").expect("country row");
    assert!(
        country == fetch + 1 && lines[country].contains("ide      nvim"),
        "country pairs with ide on the first row:\n{text}"
    );
    let langs = row_of(&lines, "langs    rust · elixir").expect("langs row");
    assert!(
        langs == fetch + 2 && lines[langs].contains("os       nixos"),
        "langs pairs with os on the second row:\n{text}"
    );
    let footer = &lines[lines.len() - 1];
    assert!(
        footer.starts_with(" j/k people") && footer.ends_with("w your profile · i add project"),
        "the footer is the section's keys, then the page's actions:\n{text}"
    );
}

#[test]
fn a_narrow_frame_stacks_and_l_opens_the_detail_over_the_list() {
    let fixture = fixture();
    let mut state = people_state();
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
    let mut state = people_state();
    state.set_shelf(Shelf::Jobs);

    // Before the replica has read the shelf, and once it has read nothing.
    let mut jobs = jobs_state_for_tests();
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs ")
            && lines[0].ends_with("all · for me · search"),
        "no count until the shelf is read:\n{}",
        lines[0]
    );
    assert!(
        row_of(&lines, "Reading the shelf…").is_some(),
        "{}",
        lines.join("\n")
    );
    jobs.loaded = true;
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs 0 "),
        "{}",
        lines[0]
    );
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
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs 2 "),
        "{text}"
    );
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
    let footer = &lines[lines.len() - 1];
    assert!(
        footer.starts_with(" j/k jobs") && footer.ends_with("w your profile · i add project"),
        "{text}"
    );

    // `/`: the viewer's langs say rust, so only Acme stays.
    jobs.toggle_for_me();
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    let text = lines.join("\n");
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs 1 "),
        "{text}"
    );
    assert!(
        row_of(&lines, "Acme · Backend Engineer").is_some(),
        "{text}"
    );
    assert!(
        row_of(&lines, "Gopher · Backend Engineer").is_none(),
        "{text}"
    );
}

/// `s` on the Jobs section opens a search box over the shelf, lights its
/// filter, and the count and the rows follow the query.
#[test]
fn the_jobs_shelf_searches_by_any_word_on_the_card() {
    use crate::app::jobs::state_test::posting;

    let fixture = fixture();
    let mut state = people_state();
    state.set_shelf(Shelf::Jobs);
    let mut jobs = jobs_state_for_tests();
    jobs.loaded = true;
    jobs.items = vec![
        posting("Acme", &["rust", "postgres"]),
        posting("Gopher", &["go"]),
    ];

    jobs.enter_search();
    for ch in "goph".chars() {
        jobs.search_push(ch);
    }
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    let text = lines.join("\n");
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs 1 ")
            && lines[0].ends_with("all · for me · search"),
        "{text}"
    );
    assert!(row_of(&lines, "Search jobs").is_some(), "{text}");
    assert!(row_of(&lines, "goph▏").is_some(), "{text}");
    assert!(
        row_of(&lines, "Gopher · Backend Engineer").is_some(),
        "{text}"
    );
    assert!(
        row_of(&lines, "Acme · Backend Engineer").is_none(),
        "{text}"
    );
    assert!(lines[lines.len() - 1].contains(" s search"), "{text}");

    jobs.search_push('x');
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    assert!(
        row_of(&lines, "No posting matches.").is_some(),
        "{}",
        lines.join("\n")
    );

    jobs.exit_search();
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 30);
    let text = lines.join("\n");
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs 2 "),
        "{text}"
    );
    assert!(row_of(&lines, "Search jobs").is_none(), "{text}");
}

#[test]
fn people_wheel_scrolls_viewport_and_repaints_hits_without_moving_selection() {
    use crate::app::directory::mouse::{Pane, Target};
    let mut fixture = fixture();
    fixture.people = (0..20)
        .map(|index| card(Uuid::now_v7(), &format!("person{index}"), index))
        .collect();
    fixture.projects.clear();
    let mut state = people_state();
    render(&fixture, &state, 120, 20);
    let first = state
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| matches!(target, Target::Person(_)))
        .unwrap()
        .0;
    state.mouse.scroll(first.x, first.y, 9, (120, 20));
    assert_eq!(state.selected(), 0);
    assert_eq!(state.mouse.offset(Pane::PeopleList), 9);
    assert_eq!(state.mouse.offset(Pane::PeopleDetail), 0);
    assert!(
        state.mouse.target(first.x, first.y, (120, 20)).is_none(),
        "old hits dropped until paint"
    );
    let lines = render(&fixture, &state, 120, 20);
    for (rect, target) in state.mouse.hits() {
        if let Target::Person(id) = target {
            let person = fixture
                .people
                .iter()
                .find(|person| person.profile.user_id == id)
                .unwrap();
            assert!(
                lines
                    .iter()
                    .skip(usize::from(rect.y))
                    .take(usize::from(rect.height))
                    .any(|line| line.contains(&format!("@{}", person.author_username))),
                "hit belongs to the painted row"
            );
        }
    }
    state.move_selection(1, 20);
    render(&fixture, &state, 120, 20);
    assert!(
        state.mouse.offset(Pane::PeopleList) < 9,
        "keyboard reveals the selected row"
    );
    assert!(state.mouse.target(first.x, first.y, (80, 20)).is_none());
}

#[test]
fn narrow_detail_copy_targets_keep_full_urls_after_wrapping_and_scroll() {
    use crate::app::directory::mouse::{Pane, Target};
    let mut fixture = fixture();
    let url = "https://example.com/a/very/long/project/link/with/a/query?name=full-value";
    fixture.people[0].profile.links = vec![url.to_string()];
    let mut state = people_state();
    state.open_detail();
    render(&fixture, &state, 45, 24);
    let mut found = false;
    for _ in 0..15 {
        for (rect, target) in state.mouse.hits() {
            if target == Target::Copy(url.to_string()) {
                assert!(rect.right() <= 45 && rect.bottom() <= 24);
                found = true;
            }
        }
        let pane = state
            .mouse
            .hits()
            .into_iter()
            .find(|(_, target)| matches!(target, Target::Copy(_)))
            .map(|(rect, _)| rect);
        if let Some(rect) = pane {
            state.mouse.scroll(rect.x, rect.y, 3, (45, 24));
        } else {
            state.mouse.scroll(10, 10, 3, (45, 24));
        }
        render(&fixture, &state, 45, 24);
    }
    assert!(found);
    assert!(state.mouse.offset(Pane::PeopleDetail) > 0);
    assert!(
        state
            .mouse
            .hits()
            .iter()
            .any(|(_, target)| *target == Target::Back)
    );
}

#[test]
fn jobs_wheel_keeps_selection_and_targets_the_visible_posting() {
    use crate::app::directory::{
        mouse::{Pane, Target},
        state::Shelf,
    };
    let fixture = fixture();
    let mut state = people_state();
    state.set_shelf(Shelf::Jobs);
    let mut jobs = jobs_state_for_tests();
    jobs.loaded = true;
    jobs.items = (0..20)
        .map(|index| crate::app::jobs::state_test::posting(&format!("Company{index}"), &["rust"]))
        .collect();
    render_with_jobs(&fixture, &state, &jobs, 120, 20);
    let first = jobs
        .mouse
        .hits()
        .into_iter()
        .find(|(_, target)| matches!(target, Target::Job(_)))
        .unwrap()
        .0;
    jobs.mouse.scroll(first.x, first.y, 6, (120, 20));
    let lines = render_with_jobs(&fixture, &state, &jobs, 120, 20);
    assert_eq!(jobs.selected(), 0);
    assert_eq!(jobs.mouse.offset(Pane::JobsList), 6);
    for (rect, target) in jobs.mouse.hits() {
        if let Target::Job(id) = target {
            let job = jobs.items.iter().find(|job| job.id == id).unwrap();
            assert!(
                lines
                    .iter()
                    .skip(usize::from(rect.y))
                    .take(usize::from(rect.height))
                    .any(|line| line.contains(&job.company))
            );
        }
    }
}

#[test]
fn opening_a_person_past_the_first_page_reveals_their_row() {
    use crate::app::directory::mouse::Pane;
    let mut fixture = fixture();
    fixture.people = (0..20)
        .map(|index| card(Uuid::now_v7(), &format!("person{index}"), index))
        .collect();
    fixture.projects.clear();
    let mut state = people_state();
    render(&fixture, &state, 120, 20);
    state.enter_search();
    render(&fixture, &state, 120, 20);
    state.exit_search();
    state.select_and_open(15);
    let lines = render(&fixture, &state, 120, 20);
    assert!(
        state.mouse.offset(Pane::PeopleList) > 0,
        "the list scrolls to the person"
    );
    assert!(
        lines.iter().any(|line| line.starts_with("▎@person")),
        "the selected row is on screen:\n{}",
        lines.join("\n")
    );
}

#[test]
fn the_connected_lead_the_people_list_under_their_own_rule() {
    let fixture = fixture();
    let renu = fixture.projects[0].showcase.user_id;
    let mut state = people_state();

    let quiet = render(&fixture, &state, 140, 40);
    assert!(
        row_of(&quiet, "\u{2500}\u{2500} online").is_none(),
        "no rule while nobody is connected"
    );
    let mey = row_of(&quiet, "@meythewitch").expect("mey listed");
    let renu_row = row_of(&quiet, "@Renu").expect("renu listed");
    assert!(mey < renu_row, "newest first while nobody is connected");

    state.set_online(HashSet::from([renu]));
    let lines = render(&fixture, &state, 140, 40);
    let online = row_of(&lines, "\u{2500}\u{2500} online 1").expect("online rule, with its count");
    let renu_row = row_of(&lines, "@Renu").expect("renu listed");
    let offline = row_of(&lines, "\u{2500}\u{2500} offline").expect("offline rule");
    let mey = row_of(&lines, "@meythewitch").expect("mey listed");
    assert!(
        online < renu_row && renu_row < offline && offline < mey,
        "{lines:#?}"
    );
}

/// Most of the page's tests look at the Profiles section, which is one `]`
/// in from where the page opens.
fn people_state() -> DirectoryState {
    let mut state = DirectoryState::new();
    state.set_shelf(Shelf::Profiles);
    state
}

/// The page opens on the map: the section row leads with it and its count
/// and carries the map's slices on its right, the world is drawn under the
/// map's title, and the footer is the map's keys then the page's actions.
#[test]
fn the_page_opens_on_the_map_shelf() {
    let fixture = fixture();
    let state = DirectoryState::new();
    let lines = render(&fixture, &state, 140, 40);
    assert!(
        lines[0].starts_with(" Map 0   Profiles 2   Jobs")
            && lines[0].ends_with("online · month · all"),
        "{:?}",
        lines[0]
    );
    assert!(
        lines[1].contains("where everyone is"),
        "the map's title right under the section row:\n{}",
        lines.join("\n")
    );
    assert!(
        lines.iter().any(|line| line.contains('\u{2580}')),
        "the world is drawn"
    );
    let footer = lines.last().expect("a footer");
    assert!(
        footer.starts_with(" m online/month/all")
            && footer.ends_with("w your profile · i add project"),
        "{lines:#?}"
    );
}

/// The mouse reaches everything the row and the footer print: every
/// section name, every filter that is not already on, and the page's
/// actions. The lit filter is no target: clicking it would toggle it off.
#[test]
fn the_section_row_and_the_footer_are_click_targets() {
    use crate::app::directory::mouse::Target;
    use crate::app::usermap::state::MapMode;

    let fixture = fixture();
    let state = DirectoryState::new();
    let lines = render(&fixture, &state, 140, 40);
    let targets: Vec<(String, Target)> = state
        .mouse
        .hits()
        .into_iter()
        .map(|(rect, target)| {
            let label = cells(
                &lines[usize::from(rect.y)],
                usize::from(rect.x),
                usize::from(rect.right()),
            );
            (label.trim().to_string(), target)
        })
        .collect();
    for expected in [
        ("Map 0".to_string(), Target::Shelf(Shelf::Map)),
        ("Profiles 2".to_string(), Target::Shelf(Shelf::Profiles)),
        ("Jobs".to_string(), Target::Shelf(Shelf::Jobs)),
        ("month".to_string(), Target::MapMode(MapMode::Recent)),
        ("all".to_string(), Target::MapMode(MapMode::Everyone)),
        ("w your profile".to_string(), Target::Key(b'w')),
        ("i add project".to_string(), Target::Key(b'i')),
    ] {
        assert!(targets.contains(&expected), "{expected:?} in {targets:#?}");
    }
    assert!(
        !targets
            .iter()
            .any(|(_, target)| *target == Target::MapMode(MapMode::Online)),
        "the slice already on is not a target"
    );
}
