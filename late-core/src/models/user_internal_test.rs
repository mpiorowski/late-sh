use super::*;

#[test]
fn extract_theme_id_reads_trimmed_string() {
    let settings = json!({ "theme_id": " purple " });
    assert_eq!(extract_theme_id(&settings).as_deref(), Some("purple"));
}

#[test]
fn extract_favorite_theme_ids_keeps_starred_order() {
    let settings = json!({ "favorite_theme_ids": ["mocha", "gruvbox", "nord"] });
    assert_eq!(
        extract_favorite_theme_ids(&settings),
        vec!["mocha", "gruvbox", "nord"]
    );
}

#[test]
fn extract_favorite_theme_ids_drops_junk_without_losing_the_rest() {
    let settings = json!({
        "favorite_theme_ids": ["  mocha  ", "", "mocha", 7, null, "nord"]
    });
    // Trimmed, deduped on first occurrence, non-strings and blanks skipped.
    assert_eq!(extract_favorite_theme_ids(&settings), vec!["mocha", "nord"]);
}

#[test]
fn extract_favorite_theme_ids_missing_is_empty() {
    assert!(extract_favorite_theme_ids(&json!({})).is_empty());
    assert!(extract_favorite_theme_ids(&json!({ "favorite_theme_ids": "nope" })).is_empty());
}

#[test]
fn extract_theme_id_missing_returns_none() {
    let settings = json!({});
    assert_eq!(extract_theme_id(&settings), None);
}

#[test]
fn chat_profile_award_badges_prefer_frontier_king_over_archdemon() {
    assert_eq!(
        chat_profile_award_badges(Some("LMG LKN".to_string())).as_deref(),
        Some("LKN")
    );
    assert_eq!(
        chat_profile_award_badges(Some("AW1 LMG LKN CHIP2".to_string())).as_deref(),
        Some("AW1 LKN CHIP2")
    );
}

#[test]
fn chat_profile_award_badges_prefer_sundering_deep_over_the_lesser_crowns() {
    assert_eq!(
        chat_profile_award_badges(Some("LMG LKN LYS".to_string())).as_deref(),
        Some("LYS")
    );
    assert_eq!(
        chat_profile_award_badges(Some("AW1 LMG LYS CHIP2".to_string())).as_deref(),
        Some("AW1 LYS CHIP2")
    );
}

#[test]
fn chat_profile_award_badges_prefer_kaethyr_over_every_lesser_crown() {
    assert_eq!(
        chat_profile_award_badges(Some("LMG LKN LYS LKA".to_string())).as_deref(),
        Some("LKA")
    );
    assert_eq!(
        chat_profile_award_badges(Some("AW1 LMG LKA CHIP2".to_string())).as_deref(),
        Some("AW1 LKA CHIP2")
    );
}

#[test]
fn chat_profile_award_badges_keep_archdemon_when_it_is_the_best_lateania_badge() {
    assert_eq!(
        chat_profile_award_badges(Some("AW1 LMG CHIP2".to_string())).as_deref(),
        Some("AW1 LMG CHIP2")
    );
    assert_eq!(
        chat_profile_award_badges(Some("LMG".to_string())).as_deref(),
        Some("LMG")
    );
}

#[test]
fn chat_profile_award_badges_prefer_ascension_over_amulet() {
    // Ascension implies the Amulet, so the chat label collapses NHA into NHY.
    assert_eq!(
        chat_profile_award_badges(Some("NHA NHY".to_string())).as_deref(),
        Some("NHY")
    );
    // The Amulet alone stands on its own.
    assert_eq!(
        chat_profile_award_badges(Some("AW1 NHA".to_string())).as_deref(),
        Some("AW1 NHA")
    );
}

#[test]
fn chat_profile_award_badges_prefer_brogue_mastery_over_escape() {
    // A mastery implies the escape, so the chat label collapses BRE into BRM.
    assert_eq!(
        chat_profile_award_badges(Some("BRE BRM".to_string())).as_deref(),
        Some("BRM")
    );
    // The escape alone stands on its own, and other games' pairs collapse
    // independently.
    assert_eq!(
        chat_profile_award_badges(Some("AW1 BRE".to_string())).as_deref(),
        Some("AW1 BRE")
    );
    assert_eq!(
        chat_profile_award_badges(Some("NHA BRE BRM DCO".to_string())).as_deref(),
        Some("NHA BRM DCO")
    );
}

#[test]
fn extract_bio_missing_returns_empty() {
    let settings = json!({});
    assert_eq!(extract_bio(&settings), "");
}

#[test]
fn extract_enable_background_color_defaults_to_true() {
    let settings = json!({});
    assert!(extract_enable_background_color(&settings));
}

#[test]
fn extract_text_brightness_adjustment_defaults_to_zero_and_clamps() {
    assert_eq!(extract_text_brightness_adjustment(&json!({})), 0);
    assert_eq!(
        extract_text_brightness_adjustment(&json!({ "text_brightness_adjustment": 2 })),
        2
    );
    assert_eq!(
        extract_text_brightness_adjustment(&json!({ "text_brightness_adjustment": 9 })),
        5
    );
    assert_eq!(
        extract_text_brightness_adjustment(&json!({ "text_brightness_adjustment": -9 })),
        -5
    );
}

#[test]
fn extract_enable_background_color_reads_explicit_false() {
    let settings = json!({ "enable_background_color": false });
    assert!(!extract_enable_background_color(&settings));
}

#[test]
fn extract_country_normalizes_uppercase() {
    let settings = json!({ "country": " pl " });
    assert_eq!(extract_country(&settings).as_deref(), Some("PL"));
}

#[test]
fn extract_timezone_reads_trimmed_value() {
    let settings = json!({ "timezone": " Europe/Warsaw " });
    assert_eq!(
        extract_timezone(&settings).as_deref(),
        Some("Europe/Warsaw")
    );
}

#[test]
fn sanitize_username_input_trims_and_falls_back() {
    assert_eq!(sanitize_username_input("  night-owl  "), "night-owl");
    assert_eq!(sanitize_username_input("   "), "user");
}

#[test]
fn sanitize_username_input_replaces_spaces_and_invalid_chars() {
    assert_eq!(sanitize_username_input("  night owl  "), "night_owl");
    assert_eq!(sanitize_username_input("alice!!!bob"), "alice_bob");
    assert_eq!(sanitize_username_input("@alice"), "alice");
    assert_eq!(sanitize_username_input("a@b"), "ab");
}

/// A handle never ends in a dot: "thanks @alice." has to read as alice and
/// a full stop, so the dot is dropped wherever it ends up last, truncation
/// included. Dots anywhere else stay.
#[test]
fn sanitize_username_input_refuses_a_trailing_dot() {
    assert_eq!(sanitize_username_input("alice."), "alice");
    assert_eq!(sanitize_username_input("...alice..."), "...alice");
    assert_eq!(sanitize_username_input("j.doe"), "j.doe");
    assert_eq!(sanitize_username_input("alice_."), "alice");
    assert_eq!(sanitize_username_input("..."), "user");
    let cut_on_a_dot = format!("{}.tail", "a".repeat(USERNAME_MAX_LEN - 1));
    assert_eq!(
        sanitize_username_input(&cut_on_a_dot),
        "a".repeat(USERNAME_MAX_LEN - 1)
    );
}

#[test]
fn sanitize_username_input_collapses_repeated_separators() {
    assert_eq!(sanitize_username_input("a   b\t\tc"), "a_b_c");
    assert_eq!(sanitize_username_input("a@@@b###c"), "ab_c");
}

#[test]
fn truncate_to_boundary_respects_char_boundaries() {
    assert_eq!(truncate_to_boundary("abcdef", 4), "abcd");
    assert_eq!(truncate_to_boundary("żółw", 3), "żół");
}

#[test]
fn interaction_mode_absent_signals_first_run() {
    // No key = never chosen = show the onboarding prompt.
    assert_eq!(extract_interaction_mode(&json!({})), None);
}

#[test]
fn interaction_mode_round_trips_and_gates_the_mouse() {
    for (stored, mode, mouse) in [
        ("keyboard", InteractionMode::Keyboard, false),
        ("mouse", InteractionMode::Mouse, true),
        ("hybrid", InteractionMode::Hybrid, true),
    ] {
        let settings = json!({ "interaction_mode": stored });
        assert_eq!(extract_interaction_mode(&settings), Some(mode));
        assert_eq!(mode.as_str(), stored);
        assert_eq!(mode.mouse_enabled(), mouse, "mouse gate for {stored}");
    }
    // Unknown / garbage falls back to the safe both-work default.
    assert_eq!(
        extract_interaction_mode(&json!({ "interaction_mode": "wat" })),
        Some(InteractionMode::Hybrid)
    );
    assert!(InteractionMode::default().mouse_enabled());
}
