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

/// The status bar customizer's detail pane is built from this list, so a dial
/// that appears here for a component that cannot use it is a control the user
/// can move onto and change with no effect.
#[test]
fn statusline_dials_omit_controls_the_component_cannot_use() {
    // Keyhints has a brief display switch; its text is otherwise pre-styled.
    assert_eq!(
        statusline_dials_for(StatusComponent::Shortcuts),
        vec![StatuslineDial::Brief]
    );
    // Always on, no dial of its own: label and drop tier, nothing else.
    assert_eq!(
        statusline_dials_for(StatusComponent::Users),
        vec![StatuslineDial::Label, StatuslineDial::LowPriority]
    );
    // Hides when idle and picks what it counts.
    assert_eq!(
        statusline_dials_for(StatusComponent::Quests),
        vec![
            StatuslineDial::Label,
            StatuslineDial::AutoHide,
            StatuslineDial::LowPriority,
            StatuslineDial::Variant,
        ]
    );
    // Always on, but does have a dial.
    assert_eq!(
        statusline_dials_for(StatusComponent::Time),
        vec![
            StatuslineDial::Label,
            StatuslineDial::LowPriority,
            StatuslineDial::Variant,
        ]
    );
}

#[test]
fn every_statusline_dial_offered_has_a_heading() {
    for component in StatusComponent::ALL {
        let setting = StatusComponentSetting::new(component);
        for dial in statusline_dials_for(component) {
            assert!(
                !dial.title(&setting).is_empty(),
                "{} {dial:?} heading",
                component.as_str()
            );
        }
    }
}

/// `Time` has no word to paint, so `Text` and `None` would render identically.
/// Cycling has to step over it rather than parking the user on a dead position.
#[test]
fn label_cycle_skips_text_for_a_component_with_no_word() {
    assert_eq!(
        next_label_mode(LabelMode::Icon, StatusComponent::Time, true),
        LabelMode::None
    );
    assert_eq!(
        next_label_mode(LabelMode::None, StatusComponent::Time, true),
        LabelMode::Icon
    );
    assert_eq!(
        next_label_mode(LabelMode::Icon, StatusComponent::Time, false),
        LabelMode::None
    );
    // Everything else keeps all three positions.
    assert_eq!(
        next_label_mode(LabelMode::None, StatusComponent::Chips, true),
        LabelMode::Text
    );
}

#[test]
fn variant_cycle_wraps_and_recovers_from_an_unset_variant() {
    let variants = StatusComponent::Station.variants();
    assert_eq!(
        next_variant(Some(StatusVariant::StationName), variants, true),
        Some(StatusVariant::StationTrack)
    );
    assert_eq!(
        next_variant(Some(StatusVariant::StationTrack), variants, true),
        Some(StatusVariant::StationName)
    );
    assert_eq!(
        next_variant(Some(StatusVariant::StationName), variants, false),
        Some(StatusVariant::StationTrack)
    );
    // A component whose stored variant went stale still moves off the first.
    assert_eq!(
        next_variant(None, variants, true),
        Some(StatusVariant::StationTrack)
    );
    assert_eq!(
        next_variant(None, StatusComponent::Chips.variants(), true),
        None
    );
}
