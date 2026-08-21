use super::*;

#[test]
fn normalize_optional_text_trims_and_collapses_blank() {
    assert_eq!(
        normalize_optional_text("  VS   Code  ").as_deref(),
        Some("VS Code")
    );
    assert_eq!(normalize_optional_text("   "), None);
}

fn option_named(label: &str) -> &'static theme::ThemeOption {
    theme::OPTIONS
        .iter()
        .find(|option| option.label == label)
        .unwrap_or_else(|| panic!("no theme labelled {label}"))
}

#[test]
fn theme_search_matches_the_theme_name() {
    let option = option_named("Catppuccin Mocha");
    assert!(theme_matches(option, "mocha"));
    assert!(theme_matches(option, "moc"));
    // Case folded on both sides.
    assert!(theme_matches(option, "MOCHA".to_lowercase().as_str()));
    assert!(!theme_matches(option, "nonsense"));
}

/// Families are the other way people name a theme, and a group label that no
/// theme repeats in its own name would otherwise be unsearchable.
#[test]
fn theme_search_matches_the_group_name() {
    let option = option_named("Catppuccin Mocha");
    assert!(theme_matches(option, &option.group.label().to_lowercase()));
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
