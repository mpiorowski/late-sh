use chrono::{Duration, Utc};
use uuid::Uuid;

use super::state::{
    DirectoryEntry, DirectoryEntryId, DirectoryFilter, DirectoryState, merged_entries,
};
use crate::app::chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem};
use late_core::models::{showcase::Showcase, work_profile::WorkProfile};

fn project(user_id: Uuid, title: &str, age_minutes: i64) -> ShowcaseFeedItem {
    let now = Utc::now();
    ShowcaseFeedItem {
        showcase: Showcase {
            id: Uuid::now_v7(),
            user_id,
            title: title.to_string(),
            url: format!("https://example.com/{title}"),
            description: format!("{title} description"),
            tags: vec!["rust".to_string()],
            created: now - Duration::minutes(age_minutes),
            updated: now - Duration::minutes(age_minutes),
        },
        author_username: "builder".to_string(),
        author_profile: None,
    }
}

fn person(user_id: Uuid, headline: &str, age_minutes: i64) -> WorkFeedItem {
    let now = Utc::now();
    WorkFeedItem {
        profile: WorkProfile {
            id: Uuid::now_v7(),
            user_id,
            slug: "w_abcdef123456".to_string(),
            headline: headline.to_string(),
            status: "open".to_string(),
            work_type: "contract".to_string(),
            location: "remote".to_string(),
            contact: String::new(),
            links: Vec::new(),
            skills: vec!["go".to_string()],
            summary: format!("{headline} summary"),
            created: now - Duration::minutes(age_minutes),
            updated: now - Duration::minutes(age_minutes),
        },
        author_username: "worker".to_string(),
        author_profile: None,
    }
}

fn titles(entries: &[DirectoryEntry<'_>]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| match entry {
            DirectoryEntry::Project(item) => item.showcase.title.clone(),
            DirectoryEntry::Person(item) => item.profile.headline.clone(),
        })
        .collect()
}

#[test]
fn merged_entries_interleave_newest_first() {
    let viewer = Uuid::now_v7();
    let projects = vec![project(viewer, "old-project", 30)];
    let people = vec![person(viewer, "fresh-person", 5)];
    let entries = merged_entries(
        &projects,
        &people,
        DirectoryFilter::All,
        false,
        viewer,
        "",
    );
    assert_eq!(titles(&entries), vec!["fresh-person", "old-project"]);
}

#[test]
fn filter_narrows_to_one_kind() {
    let viewer = Uuid::now_v7();
    let projects = vec![project(viewer, "proj", 10)];
    let people = vec![person(viewer, "card", 5)];

    let only_projects = merged_entries(
        &projects,
        &people,
        DirectoryFilter::Projects,
        false,
        viewer,
        "",
    );
    assert_eq!(titles(&only_projects), vec!["proj"]);

    let only_people = merged_entries(
        &projects,
        &people,
        DirectoryFilter::People,
        false,
        viewer,
        "",
    );
    assert_eq!(titles(&only_people), vec!["card"]);
}

#[test]
fn mine_only_keeps_viewer_entries() {
    let viewer = Uuid::now_v7();
    let other = Uuid::now_v7();
    let projects = vec![project(viewer, "mine", 10), project(other, "theirs", 5)];
    let people = vec![person(other, "their card", 1)];
    let entries = merged_entries(&projects, &people, DirectoryFilter::All, true, viewer, "");
    assert_eq!(titles(&entries), vec!["mine"]);
}

#[test]
fn query_matches_across_both_kinds() {
    let viewer = Uuid::now_v7();
    let projects = vec![project(viewer, "late-tui", 10), project(viewer, "blog", 5)];
    let people = vec![person(viewer, "late nights welcome", 1)];
    let entries = merged_entries(
        &projects,
        &people,
        DirectoryFilter::All,
        false,
        viewer,
        "late",
    );
    assert_eq!(titles(&entries), vec!["late nights welcome", "late-tui"]);
}

#[test]
fn entry_ids_are_stable_identities() {
    let viewer = Uuid::now_v7();
    let projects = vec![project(viewer, "proj", 10)];
    let people = vec![person(viewer, "card", 5)];
    let entries = merged_entries(&projects, &people, DirectoryFilter::All, false, viewer, "");
    assert_eq!(
        entries[0].id(),
        DirectoryEntryId::Person(people[0].profile.id)
    );
    assert_eq!(
        entries[1].id(),
        DirectoryEntryId::Project(projects[0].showcase.id)
    );
}

#[test]
fn selection_moves_clamp_to_list() {
    let mut state = DirectoryState::new();
    state.move_selection(1, 3);
    assert_eq!(state.selected(), 1);
    state.move_selection(10, 3);
    assert_eq!(state.selected(), 2);
    state.move_selection(-10, 3);
    assert_eq!(state.selected(), 0);
    state.move_selection(1, 0);
    assert_eq!(state.selected(), 0);
}

#[test]
fn filter_cycle_resets_selection() {
    let mut state = DirectoryState::new();
    state.move_selection(2, 5);
    state.cycle_filter_next();
    assert_eq!(state.filter, DirectoryFilter::Projects);
    assert_eq!(state.selected(), 0);
    state.cycle_filter_prev();
    assert_eq!(state.filter, DirectoryFilter::All);
}

#[test]
fn search_query_is_active_only_in_search_mode() {
    let mut state = DirectoryState::new();
    state.enter_search();
    state.search_push('r');
    state.search_push('u');
    assert_eq!(state.active_query(), "ru");
    state.search_backspace();
    assert_eq!(state.active_query(), "r");
    state.exit_search();
    assert_eq!(state.active_query(), "");
    assert!(!state.search_mode());
}
