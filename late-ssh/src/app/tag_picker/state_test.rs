use late_core::vocab::{Group, TAG_LIMIT};

use super::state::{CAP_NOTICE, Row, TagPickerState, TagPickerTarget};

fn chosen(state: &TagPickerState) -> Vec<&str> {
    state.chosen().iter().map(String::as_str).collect()
}

fn tags(rows: &[Row]) -> Vec<&'static str> {
    rows.iter()
        .filter_map(|row| match row {
            Row::Tag(tag) => Some(*tag),
            Row::Heading(_) => None,
        })
        .collect()
}

#[test]
fn opening_folds_what_the_field_held_and_keeps_only_the_scope() {
    let mut picker = TagPickerState::default();
    picker.open(
        TagPickerTarget::EditorLangs,
        ["Golang", "rust", "postgres", "brainfuck", "go"]
            .map(str::to_string)
            .to_vec(),
    );
    assert!(picker.is_open());
    // postgres is not a language, brainfuck is not in the list, go repeats.
    assert_eq!(chosen(&picker), vec!["go", "rust"]);
    let rows = picker.rows();
    assert_eq!(rows[0], Row::Heading(Group::Language));
    assert_eq!(rows[1], Row::Tag("rust"));
    assert_eq!(picker.cursor(), 1, "the cursor starts on the first tag");
    assert!(
        !tags(&rows).contains(&"postgres"),
        "langs offer languages only"
    );

    let mut skills = TagPickerState::default();
    skills.open(TagPickerTarget::EditorSkills, Vec::new());
    let rows = skills.rows();
    assert!(tags(&rows).contains(&"postgres"));
    assert!(tags(&rows).contains(&"kubernetes"));
    assert_eq!(
        rows.iter()
            .filter(|row| matches!(row, Row::Heading(_)))
            .count(),
        Group::ALL.len()
    );
}

#[test]
fn typing_filters_by_name_or_alias_and_enter_picks_then_clears() {
    let mut picker = TagPickerState::default();
    picker.open(TagPickerTarget::EditorSkills, Vec::new());
    for ch in "K8S".chars() {
        picker.push(ch);
    }
    assert_eq!(picker.query(), "k8s");
    assert_eq!(picker.rows(), vec![Row::Tag("kubernetes")]);
    picker.enter();
    assert_eq!(chosen(&picker), vec!["kubernetes"]);
    assert_eq!(picker.query(), "", "enter clears the query");
    assert_eq!(picker.current(), Some("kubernetes"), "and stays on the tag");

    // A prefix match sorts ahead of a substring one.
    for ch in "script".chars() {
        picker.push(ch);
    }
    assert_eq!(
        tags(&picker.rows()),
        vec!["bash", "typescript", "javascript"],
        "`scripting` is an alias of bash, so bash starts with it"
    );
    picker.backspace();
    assert_eq!(picker.query(), "scrip");
    // A character no tag is spelled with is ignored.
    picker.push('!');
    assert_eq!(picker.query(), "scrip");

    for _ in 0..5 {
        picker.backspace();
    }
    assert_eq!(picker.query(), "");
    picker.backspace();
    assert!(
        picker.chosen().is_empty(),
        "backspace on an empty query drops the last chosen tag"
    );
}

#[test]
fn the_cursor_skips_headings_and_space_toggles() {
    let mut picker = TagPickerState::default();
    picker.open(TagPickerTarget::EditorSkills, Vec::new());
    picker.set_visible_height(10);
    let rows = picker.rows();
    let languages = tags(
        &rows[..rows
            .iter()
            .rposition(|r| matches!(r, Row::Heading(Group::Framework)))
            .unwrap()],
    )
    .len();
    // Down through every language lands on the first framework, past its
    // heading, and the list scrolled to keep it in view.
    picker.move_cursor(languages as isize);
    assert_eq!(picker.current(), Some("react"));
    assert!(picker.scroll() > 0);
    picker.toggle();
    assert_eq!(chosen(&picker), vec!["react"]);
    picker.toggle();
    assert!(picker.chosen().is_empty());
    // Up past the top stays on the first tag, and the scroll follows.
    picker.move_cursor(-1000);
    assert_eq!(picker.current(), Some("rust"));
    assert_eq!(picker.scroll(), 0);
}

#[test]
fn a_thirteenth_pick_is_refused_with_the_notice() {
    let mut picker = TagPickerState::default();
    picker.open(TagPickerTarget::SettingsLangs, Vec::new());
    for _ in 0..TAG_LIMIT {
        picker.toggle();
        picker.move_cursor(1);
    }
    assert_eq!(picker.chosen().len(), TAG_LIMIT);
    let next = picker.current().expect("a tag under the cursor");
    picker.toggle();
    assert_eq!(picker.chosen().len(), TAG_LIMIT);
    assert!(!picker.is_chosen(next));
    assert_eq!(picker.notice(), Some(CAP_NOTICE));
    picker.backspace();
    assert_eq!(picker.chosen().len(), TAG_LIMIT - 1);
    assert_eq!(picker.notice(), None);

    let (target, chosen) = picker.close().expect("was open");
    assert_eq!(target, TagPickerTarget::SettingsLangs);
    assert_eq!(chosen.len(), TAG_LIMIT - 1);
    assert!(!picker.is_open());
    assert_eq!(picker.close(), None);
}
