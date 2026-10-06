use late_core::models::job_posting::RemoteKind;
use ratatui_textarea::TextArea;
use uuid::Uuid;

use super::post::{PostField, PostForm, PostValues, next_scope, validate_post};

fn type_into(ta: &mut TextArea<'static>, text: &str) {
    ta.select_all();
    ta.cut();
    ta.insert_str(text);
}

fn filled() -> PostValues {
    PostValues {
        company: "Acme".to_string(),
        title: "Rust Engineer".to_string(),
        link: "https://acme.example/jobs/1".to_string(),
        scope: Some(RemoteKind::Regions),
        regions: " EU , US,, ".to_string(),
        tags: ["Rust", "k8s", "cobol"].map(str::to_string).to_vec(),
        pay: "€90k".to_string(),
        excerpt: "  Acme builds\n\n tools.  ".to_string(),
    }
}

#[test]
fn a_filled_form_becomes_a_posting_with_folded_tags_and_split_regions() {
    let by = Uuid::now_v7();
    let posting = validate_post(&filled(), by).expect("valid");
    assert_eq!(posting.posted_by, by);
    assert_eq!(posting.company, "Acme");
    assert_eq!(posting.remote_kind, RemoteKind::Regions);
    assert_eq!(posting.regions, vec!["EU", "US"]);
    assert_eq!(
        posting.tags,
        vec!["rust", "kubernetes"],
        "cobol is not in the list"
    );
    assert_eq!(posting.excerpt, "Acme builds tools.");

    // Worldwide keeps no regions, whatever was typed.
    let mut worldwide = filled();
    worldwide.scope = Some(RemoteKind::Worldwide);
    assert!(
        validate_post(&worldwide, by)
            .expect("valid")
            .regions
            .is_empty()
    );
}

#[test]
fn the_rules_name_the_row_in_order() {
    let by = Uuid::now_v7();
    let mut values = filled();
    values.company = "  ".to_string();
    assert_eq!(
        validate_post(&values, by).unwrap_err().0,
        PostField::Company
    );
    values = filled();
    values.link = "acme.example".to_string();
    assert_eq!(validate_post(&values, by).unwrap_err().0, PostField::Link);
    values = filled();
    values.regions = String::new();
    assert_eq!(
        validate_post(&values, by).unwrap_err().0,
        PostField::Regions,
        "within regions needs at least one"
    );
    values.scope = Some(RemoteKind::Hybrid);
    assert_eq!(
        validate_post(&values, by).unwrap_err().0,
        PostField::Regions
    );
    values = filled();
    values.excerpt = "x".repeat(401);
    assert_eq!(
        validate_post(&values, by).unwrap_err().0,
        PostField::Excerpt
    );
}

#[test]
fn the_form_walks_its_rows_cycles_the_scope_and_lands_on_the_failing_row() {
    let mut form = PostForm::default();
    assert!(!form.is_open());
    form.open();
    assert!(form.is_open());
    assert!(form.editing(), "opens typing into the company row");
    assert_eq!(form.active_field(), PostField::Company);
    type_into(form.field_mut(PostField::Company), "Acme");
    form.commit_and_advance(true);
    assert_eq!(form.active_field(), PostField::Title);
    type_into(form.field_mut(PostField::Title), "Rust Engineer");
    form.commit_and_advance(true);
    type_into(
        form.field_mut(PostField::Link),
        "https://acme.example/jobs/1",
    );
    form.commit_and_advance(true);
    // The scope row is landed on, not cycled, until an explicit key.
    assert_eq!(form.active_field(), PostField::Scope);
    assert!(!form.editing());
    assert_eq!(form.scope(), RemoteKind::Worldwide);
    form.cycle_scope(true);
    assert_eq!(form.scope(), RemoteKind::Regions);
    assert_eq!(next_scope(RemoteKind::Hybrid, true), RemoteKind::Worldwide);
    assert_eq!(next_scope(RemoteKind::Worldwide, false), RemoteKind::Hybrid);
    form.set_tags(vec!["rust".to_string()]);
    assert_eq!(form.field_text(PostField::Tags), "rust");

    // Ctrl+S with no regions and no excerpt: the first bad row wins.
    let by = Uuid::now_v7();
    assert!(form.submit(by).is_none());
    assert_eq!(form.active_field(), PostField::Regions);
    assert_eq!(
        form.error().map(|e| e.field),
        Some(Some(PostField::Regions))
    );
    assert!(!form.pending());

    form.start_editing();
    type_into(form.field_mut(PostField::Regions), "EU");
    form.move_row(3);
    assert_eq!(form.active_field(), PostField::Excerpt);
    form.start_editing();
    type_into(form.field_mut(PostField::Excerpt), "Acme builds tools.");
    let posting = form.submit(by).expect("a posting");
    assert_eq!(posting.regions, vec!["EU"]);
    assert!(form.pending(), "the save is in flight");

    // The cap answer keeps the form with the reason; Esc then closes it.
    form.settle(Some("three live postings per person"));
    assert!(!form.pending());
    assert_eq!(form.error().map(|e| e.field), Some(None));
    assert!(form.escape(), "idle, so Esc closes");
    assert!(!form.is_open());
}
