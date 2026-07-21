use super::*;

#[test]
fn normalize_optional_text_trims_and_collapses_blank() {
    assert_eq!(
        normalize_optional_text("  VS   Code  ").as_deref(),
        Some("VS Code")
    );
    assert_eq!(normalize_optional_text("   "), None);
}

#[test]
fn readonly_bio_textarea_resets_cursor_to_top() {
    let input = bio_textarea_for_readonly_text("first line\nsecond line\nthird line");
    assert_eq!(input.cursor(), (0usize, 0usize));
}

#[test]
fn move_bio_cursor_to_end_goes_to_last_line_end() {
    let mut input = bio_textarea_for_readonly_text("first line\nsecond line\nthird line");

    move_bio_cursor_to_end(&mut input);

    assert_eq!(input.cursor(), (2usize, "third line".chars().count()));
}
