use chrono::Utc;
use late_core::models::{
    profile::Profile,
    work_profile::{WorkProfile, WorkStatus, WorkType},
};
use ratatui_textarea::TextArea;
use uuid::Uuid;

use super::state::{
    CardValues, EditorState, EscapeOutcome, Field, Page, ProjectRow, ProjectValues, ProjectsView,
    Save, Scope, validate_card, validate_project,
};

fn card(user_id: Uuid) -> WorkProfile {
    let now = Utc::now();
    WorkProfile {
        id: Uuid::now_v7(),
        user_id,
        slug: "w_abcdef123456".to_string(),
        headline: "Rust backend engineer".to_string(),
        status: WorkStatus::Casual,
        work_type: WorkType::Contract,
        location: "EU remote".to_string(),
        contact: "me@example.com".to_string(),
        links: vec!["https://github.com/me".to_string()],
        skills: vec!["rust".to_string(), "cobol".to_string()],
        skills_tags: vec!["rust".to_string(), "cobol".to_string()],
        summary: "Terminal software.".to_string(),
        created: now,
        updated: now,
    }
}

fn profile() -> Profile {
    Profile {
        bio: "hello".to_string(),
        ide: Some("nvim".to_string()),
        langs: vec!["rust".to_string()],
        ..Profile::default()
    }
}

fn project() -> ProjectRow {
    ProjectRow {
        id: Uuid::now_v7(),
        title: "SalaTUI".to_string(),
        url: "https://example.com/salatui".to_string(),
        tags: vec!["rust".to_string(), "tui".to_string()],
        description: "A salad in the terminal.".to_string(),
        created: Utc::now(),
    }
}

fn type_into(ta: &mut TextArea<'static>, text: &str) {
    ta.select_all();
    ta.cut();
    ta.insert_str(text);
}

#[test]
fn opening_your_own_profile_seeds_every_page_and_is_clean() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    assert!(!editor.is_open());
    editor.open_own(viewer, Some(&card(viewer)), &profile(), Page::Card);
    assert!(editor.is_open());
    assert_eq!(editor.scope(), &Scope::Own);
    assert_eq!(
        editor.scope().pages(),
        &[Page::Card, Page::About, Page::Projects]
    );
    assert_eq!(editor.page(), Page::Card);
    assert_eq!(editor.status(), WorkStatus::Casual);
    assert_eq!(
        editor.card_values(),
        CardValues {
            headline: "Rust backend engineer".to_string(),
            status: WorkStatus::Casual,
            work_type: WorkType::Contract,
            location: "EU remote".to_string(),
            contact: "me@example.com".to_string(),
            links: "https://github.com/me".to_string(),
            skills: "rust, cobol".to_string(),
            summary: "Terminal software.".to_string(),
        }
    );
    assert_eq!(editor.about_values().bio, "hello");
    assert_eq!(editor.about_values().ide, "nvim");
    assert_eq!(editor.about_values().langs, "rust");
    assert!(!editor.dirty(), "nothing typed yet");
    assert_eq!(editor.escape(), EscapeOutcome::Closed);
    assert!(!editor.is_open());
}

#[test]
fn closed_fields_cycle_and_wrap_with_the_arrows() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, None, &profile(), Page::Card);
    editor.set_row(1);
    assert_eq!(editor.active_field(), Some(Field::Status));
    assert_eq!(editor.status(), WorkStatus::Open);
    editor.cycle_choice(true);
    assert_eq!(editor.status(), WorkStatus::Casual);
    editor.cycle_choice(false);
    editor.cycle_choice(false);
    assert_eq!(editor.status(), WorkStatus::NotLooking, "wraps backwards");
    // Enter on a choice row cycles instead of typing.
    editor.start_editing();
    assert!(!editor.editing());
    assert_eq!(editor.status(), WorkStatus::Open);
    editor.set_row(2);
    editor.cycle_choice(true);
    assert_eq!(
        editor.card_values().work_type,
        WorkType::FullTime,
        "any wraps to full-time"
    );
    assert!(editor.dirty());
}

#[test]
fn saving_without_a_headline_names_the_row_and_saves_nothing() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, None, &profile(), Page::Card);
    editor.set_row(3);
    editor.start_editing();
    type_into(editor.field_mut(Field::Location), "Warsaw");
    assert!(editor.save().is_err());
    assert!(editor.is_open(), "a failed save keeps the form");
    assert_eq!(editor.row(), 0, "the cursor lands on the failing row");
    assert_eq!(editor.error(), Some((Field::Headline, "headline required")));
    assert!(!editor.editing());
}

#[test]
fn a_full_card_saves_with_normalized_tags_and_closes() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, None, &profile(), Page::Card);
    type_into(editor.field_mut(Field::Headline), "Elixir engineer");
    type_into(editor.field_mut(Field::Location), "remote");
    type_into(
        editor.field_mut(Field::Links),
        "https://late.sh, not-a-link",
    );
    type_into(
        editor.field_mut(Field::Skills),
        "Elixir, PostgreSQL, ts, cobol, elixir",
    );
    type_into(editor.field_mut(Field::Summary), "Ship things.");
    let preview = editor.skills_preview();
    assert_eq!(preview.tags, vec!["elixir", "postgres", "typescript"]);
    assert_eq!(preview.free, vec!["cobol"]);

    let saves = editor.save().expect("valid card");
    assert!(!editor.is_open(), "a save closes the modal");
    let [Save::Card { params, editing }] = saves.as_slice() else {
        panic!("expected one card save, got {saves:?}");
    };
    assert_eq!(*editing, None);
    assert_eq!(params.user_id, viewer);
    assert!(
        params.slug.starts_with("w_") && params.slug.len() == 14,
        "{}",
        params.slug
    );
    assert_eq!(params.headline, "Elixir engineer");
    assert_eq!(params.links, vec!["https://late.sh"]);
    assert_eq!(params.skills, vec!["elixir", "postgresql", "ts", "cobol"]);
    assert_eq!(
        params.skills_tags,
        vec!["elixir", "postgres", "typescript", "cobol"]
    );
    assert_eq!(params.status, WorkStatus::Open);
    assert_eq!(params.work_type, WorkType::Any);
}

#[test]
fn editing_an_existing_card_keeps_its_id_and_slug_and_saves_the_about_page_too() {
    let viewer = Uuid::now_v7();
    let existing = card(viewer);
    let mut editor = EditorState::default();
    editor.open_own(viewer, Some(&existing), &profile(), Page::Card);
    type_into(editor.field_mut(Field::Headline), "Rust and Elixir");
    editor.switch_page(true);
    assert_eq!(editor.page(), Page::About);
    type_into(editor.field_mut(Field::Bio), "new bio");
    type_into(editor.field_mut(Field::Langs), "Rust, go, rust");

    let saves = editor.save().expect("valid");
    assert_eq!(saves.len(), 2, "{saves:?}");
    let Save::Card { params, editing } = &saves[0] else {
        panic!("card first: {saves:?}");
    };
    assert_eq!(*editing, Some(existing.id));
    assert_eq!(params.slug, existing.slug);
    assert_eq!(params.headline, "Rust and Elixir");
    let Save::About(about) = &saves[1] else {
        panic!("about second: {saves:?}");
    };
    assert_eq!(about.bio, "new bio");
    assert_eq!(about.langs, "Rust, go, rust");
}

#[test]
fn an_untouched_existing_card_saves_nothing_and_closes() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, Some(&card(viewer)), &profile(), Page::About);
    let saves = editor.save().expect("valid");
    assert!(saves.is_empty(), "{saves:?}");
    assert!(!editor.is_open());
}

#[test]
fn escape_asks_before_losing_typed_work_and_y_discards() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, None, &profile(), Page::Card);
    editor.start_editing();
    type_into(editor.field_mut(Field::Headline), "half a card");
    assert_eq!(
        editor.escape(),
        EscapeOutcome::Stayed,
        "first Esc stops typing"
    );
    assert!(!editor.editing());
    assert_eq!(editor.escape(), EscapeOutcome::AskedToDiscard);
    assert!(editor.confirm_discard());
    editor.confirm_discard_no();
    assert!(editor.is_open() && !editor.confirm_discard());
    assert_eq!(editor.escape(), EscapeOutcome::AskedToDiscard);
    assert_eq!(editor.confirm_discard_yes(), EscapeOutcome::Closed);
    assert!(!editor.is_open());
}

#[test]
fn enter_walks_the_rows_and_stops_at_the_last() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, None, &profile(), Page::Card);
    editor.start_editing();
    assert!(editor.editing());
    editor.commit_and_advance(true);
    assert_eq!(editor.active_field(), Some(Field::Status));
    assert!(!editor.editing(), "a choice row is not typed into");
    editor.commit_and_advance(true);
    editor.commit_and_advance(true);
    assert_eq!(editor.active_field(), Some(Field::Location));
    assert!(editor.editing());
    editor.set_row(7);
    editor.start_editing();
    editor.commit_and_advance(true);
    assert_eq!(
        editor.active_field(),
        Some(Field::Summary),
        "last row stays"
    );
    assert!(!editor.editing());
}

#[test]
fn the_projects_page_lists_then_forms_then_lists_again() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    editor.open_own(viewer, None, &profile(), Page::Projects);
    assert!(matches!(
        editor.projects_view(),
        ProjectsView::List { selected: 0 }
    ));
    assert!(editor.fields().is_empty());

    editor.start_new_project();
    assert!(matches!(editor.projects_view(), ProjectsView::Form(_)));
    assert!(editor.editing(), "a new project starts on its title");
    type_into(editor.field_mut(Field::Title), "late-tui");
    editor.stop_editing();
    assert!(editor.save().is_err(), "url required");
    assert_eq!(editor.error(), Some((Field::Url, "url required")));
    type_into(editor.field_mut(Field::Url), "https://example.com/late-tui");
    type_into(editor.field_mut(Field::Tags), "Rust, TUI, rust");
    type_into(editor.field_mut(Field::Description), "A late tui.");

    let saves = editor.save().expect("valid project");
    let [
        Save::Project {
            params,
            editing: None,
        },
    ] = saves.as_slice()
    else {
        panic!("expected one new project, got {saves:?}");
    };
    assert_eq!(params.user_id, viewer);
    assert_eq!(params.tags, vec!["rust", "tui"]);
    assert!(editor.is_open(), "saving a project returns to the list");
    assert!(matches!(editor.projects_view(), ProjectsView::List { .. }));
}

#[test]
fn escape_on_a_touched_project_form_asks_and_drops_only_the_draft() {
    let viewer = Uuid::now_v7();
    let mut editor = EditorState::default();
    let existing = project();
    editor.open_own_project(viewer, None, &profile(), &existing);
    assert!(matches!(editor.projects_view(), ProjectsView::Form(_)));
    assert!(!editor.dirty());
    type_into(editor.field_mut(Field::Title), "renamed");
    assert_eq!(editor.escape(), EscapeOutcome::AskedToDiscard);
    assert_eq!(editor.confirm_discard_yes(), EscapeOutcome::LeftProjectForm);
    assert!(editor.is_open());
    assert!(matches!(editor.projects_view(), ProjectsView::List { .. }));
    // An untouched form leaves without asking.
    editor.start_editing_project(&existing);
    assert_eq!(editor.escape(), EscapeOutcome::LeftProjectForm);
}

#[test]
fn a_moderator_on_someone_elses_card_gets_that_page_alone() {
    let viewer = Uuid::now_v7();
    let owner = Uuid::now_v7();
    let theirs = card(owner);
    let mut editor = EditorState::default();
    editor.open_card_of(viewer, owner, "them".to_string(), &theirs);
    assert_eq!(editor.scope().pages(), &[Page::Card]);
    editor.switch_page(true);
    assert_eq!(editor.page(), Page::Card, "nowhere to switch to");
    type_into(editor.field_mut(Field::Headline), "fixed typo");
    let saves = editor.save().expect("valid");
    let [Save::Card { params, editing }] = saves.as_slice() else {
        panic!("{saves:?}");
    };
    assert_eq!(params.user_id, owner, "the card stays theirs");
    assert_eq!(*editing, Some(theirs.id));
    assert!(!editor.is_open());

    let mut editor = EditorState::default();
    editor.open_project_of(viewer, owner, "them".to_string(), &project());
    assert_eq!(editor.scope().pages(), &[Page::Projects]);
    let saves = editor.save().expect("valid");
    assert!(matches!(
        saves.as_slice(),
        [Save::Project {
            editing: Some(_),
            ..
        }]
    ));
    assert!(!editor.is_open(), "a moderator's project save closes");
}

#[test]
fn validation_rules_name_the_field() {
    let mut values = CardValues {
        headline: "x".repeat(121),
        ..CardValues::default()
    };
    assert_eq!(
        validate_card(&values).unwrap_err(),
        (Field::Headline, "headline too long (max 120)")
    );
    values.headline = "ok".to_string();
    assert_eq!(validate_card(&values).unwrap_err().0, Field::Location);
    values.location = "remote".to_string();
    assert_eq!(validate_card(&values).unwrap_err().0, Field::Links);
    values.links = "https://a.example".to_string();
    assert_eq!(validate_card(&values).unwrap_err().0, Field::Summary);
    values.summary = "yes".to_string();
    assert!(validate_card(&values).is_ok());

    let mut project = ProjectValues {
        title: "t".to_string(),
        url: "ftp://nope".to_string(),
        ..ProjectValues::default()
    };
    assert_eq!(
        validate_project(&project).unwrap_err(),
        (Field::Url, "url must start with http:// or https://")
    );
    project.url = "https://ok".to_string();
    assert_eq!(
        validate_project(&project).unwrap_err().0,
        Field::Description
    );
}
