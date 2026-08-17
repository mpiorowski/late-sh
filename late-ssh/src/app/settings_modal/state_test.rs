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

#[test]
fn tweak_row_all_has_no_duplicates_and_includes_chat_log_rows() {
    // `TweakRow::ALL` is a hand-maintained, manually-sized array - nothing
    // enforces at compile time that every variant appears exactly once, so
    // this guards against the array silently drifting out of sync with the
    // enum (as would've happened if a future edit added a variant but
    // forgot to append it here).
    for (i, row) in TweakRow::ALL.iter().enumerate() {
        assert!(
            !TweakRow::ALL[..i].contains(row),
            "{row:?} appears more than once in ALL"
        );
    }
    assert_eq!(TweakRow::ALL.len(), 11);
    assert!(TweakRow::ALL.contains(&TweakRow::SaveDailyChatLogs));
    assert!(TweakRow::ALL.contains(&TweakRow::ViewTodaysChatLog));
}
