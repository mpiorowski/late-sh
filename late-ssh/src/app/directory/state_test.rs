use chrono::{Duration, Utc};
use uuid::Uuid;

use super::state::{DirectoryState, PersonFocus, person_entries};
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

#[test]
fn one_row_per_person_regardless_of_what_they_brought() {
    let both = Uuid::now_v7();
    let card_only = Uuid::now_v7();
    let projects_only = Uuid::now_v7();
    let projects = vec![
        project(both, "proj-a", 30),
        project(projects_only, "proj-b", 20),
        project(projects_only, "proj-c", 40),
    ];
    let people = vec![person(both, "both card", 10), person(card_only, "card", 5)];

    let entries = person_entries(&projects, &people, false, both, "");
    assert_eq!(entries.len(), 3);

    let both_entry = entries
        .iter()
        .find(|entry| entry.user_id == both)
        .expect("person with card and project");
    assert!(both_entry.work.is_some());
    assert_eq!(both_entry.projects.len(), 1);
    assert_eq!(both_entry.focus_len(), 2);

    let projects_entry = entries
        .iter()
        .find(|entry| entry.user_id == projects_only)
        .expect("person with projects only");
    assert!(projects_entry.work.is_none());
    assert_eq!(projects_entry.projects.len(), 2);
    assert_eq!(projects_entry.focus_len(), 2);
}

#[test]
fn people_sort_by_latest_activity() {
    let stale_card = Uuid::now_v7();
    let fresh_project = Uuid::now_v7();
    let projects = vec![project(fresh_project, "fresh", 5)];
    let people = vec![person(stale_card, "stale", 60)];

    let entries = person_entries(&projects, &people, false, stale_card, "");
    assert_eq!(entries[0].user_id, fresh_project);
    assert_eq!(entries[1].user_id, stale_card);
}

#[test]
fn a_fresh_project_bumps_a_person_with_an_old_card() {
    let user = Uuid::now_v7();
    let other = Uuid::now_v7();
    let projects = vec![project(user, "brand-new", 1)];
    let people = vec![
        person(user, "old card", 600),
        person(other, "newer card", 30),
    ];

    let entries = person_entries(&projects, &people, false, user, "");
    assert_eq!(
        entries[0].user_id, user,
        "latest project counts as activity"
    );
}

#[test]
fn mine_only_keeps_the_viewer_row() {
    let viewer = Uuid::now_v7();
    let other = Uuid::now_v7();
    let projects = vec![project(viewer, "mine", 10), project(other, "theirs", 5)];
    let people = vec![person(other, "their card", 1)];

    let entries = person_entries(&projects, &people, true, viewer, "");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].user_id, viewer);
}

#[test]
fn query_matches_username_card_and_projects() {
    let by_project = Uuid::now_v7();
    let by_card = Uuid::now_v7();
    let by_nothing = Uuid::now_v7();
    let projects = vec![
        project(by_project, "late-tui", 10),
        project(by_nothing, "blog", 5),
    ];
    let people = vec![person(by_card, "late nights welcome", 1)];

    let entries = person_entries(&projects, &people, false, by_project, "late");
    let ids: Vec<Uuid> = entries.iter().map(|entry| entry.user_id).collect();
    assert!(ids.contains(&by_project));
    assert!(ids.contains(&by_card));
    assert!(!ids.contains(&by_nothing));
}

#[test]
fn focus_targets_card_first_then_projects() {
    let user = Uuid::now_v7();
    let projects = vec![project(user, "proj", 10)];
    let people = vec![person(user, "card", 5)];
    let entries = person_entries(&projects, &people, false, user, "");
    let entry = &entries[0];

    assert!(matches!(entry.focus_target(0), Some(PersonFocus::Card(_))));
    let Some(PersonFocus::Project(item)) = entry.focus_target(1) else {
        panic!("focus 1 should be the project");
    };
    assert_eq!(item.showcase.title, "proj");
    assert!(entry.focus_target(2).is_none());
}

#[test]
fn focus_targets_projects_directly_without_a_card() {
    let user = Uuid::now_v7();
    let projects = vec![project(user, "only-proj", 10)];
    let entries = person_entries(&projects, &[], false, user, "");
    let Some(PersonFocus::Project(item)) = entries[0].focus_target(0) else {
        panic!("focus 0 should be the project when there is no card");
    };
    assert_eq!(item.showcase.title, "only-proj");
}

#[test]
fn unread_when_any_row_moved_past_its_marker() {
    let user = Uuid::now_v7();
    let projects = vec![project(user, "proj", 10)];
    let people = vec![person(user, "card", 60)];
    let entries = person_entries(&projects, &people, false, user, "");
    let entry = &entries[0];

    let before_everything = Some(Utc::now() - Duration::minutes(120));
    let after_everything = Some(Utc::now());
    assert!(entry.is_unread(before_everything, before_everything));
    assert!(
        entry.is_unread(after_everything, before_everything),
        "a fresh project alone keeps the person unread"
    );
    assert!(!entry.is_unread(after_everything, after_everything));
    assert!(
        entry.is_unread(None, None),
        "no marker means everything is new"
    );
}

#[test]
fn selection_moves_clamp_and_reset_focus() {
    let mut state = DirectoryState::new();
    state.move_focus(1, 3);
    assert_eq!(state.focus(), 1);
    state.move_selection(1, 3);
    assert_eq!(state.selected(), 1);
    assert_eq!(state.focus(), 0, "moving selection resets focus");
    state.move_selection(10, 3);
    assert_eq!(state.selected(), 2);
    state.move_selection(-10, 3);
    assert_eq!(state.selected(), 0);
    state.move_selection(1, 0);
    assert_eq!(state.selected(), 0);
}

#[test]
fn focus_wraps_across_the_person_items() {
    let mut state = DirectoryState::new();
    state.move_focus(1, 2);
    assert_eq!(state.focus(), 1);
    state.move_focus(1, 2);
    assert_eq!(state.focus(), 0, "focus wraps forward");
    state.move_focus(-1, 2);
    assert_eq!(state.focus(), 1, "focus wraps backward");
    state.move_focus(1, 0);
    assert_eq!(state.focus(), 0);
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
