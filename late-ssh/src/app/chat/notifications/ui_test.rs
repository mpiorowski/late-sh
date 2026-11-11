use super::{mention_preview, preview_rows};

#[test]
fn preview_rows_wraps_into_two_rows() {
    let rows = preview_rows(
        "@mat this is a long mention preview that should use both rows in the mentions panel",
        24,
        2,
    );

    assert_eq!(rows.len(), 2);
    assert!(rows[0].starts_with('"'));
    assert!(rows[1].ends_with('"'));
}

#[test]
fn preview_rows_drops_leading_reply_quote() {
    let rows = preview_rows("> quoted line\nactual reply line", 40, 2);

    assert_eq!(rows, vec!["\"actual reply line\"".to_string()]);
}

#[test]
fn preview_rows_keeps_body_when_all_quoted() {
    let rows = preview_rows("> only a quote", 40, 2);

    assert_eq!(rows, vec!["\"> only a quote\"".to_string()]);
}

#[test]
fn preview_rows_handles_empty_preview() {
    let rows = preview_rows("", 20, 2);

    assert_eq!(rows, vec!["\"\"".to_string()]);
}

/// The Mentions list reads a cut of the body, so the mask has to land
/// first: a cut through a spoiler would leave its opening words in clear.
#[test]
fn a_mention_preview_masks_spoilers_before_it_cuts() {
    assert_eq!(mention_preview("hi ||CRANE||"), "hi ▒▒▒▒▒");

    let long = format!("{} ||the butler did it||", "x".repeat(110));
    assert_eq!(
        mention_preview(&long),
        format!("{} {}", "x".repeat(110), "▒".repeat(9))
    );
}
