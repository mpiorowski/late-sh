use super::*;

#[test]
fn extract_theme_id_reads_trimmed_string() {
    let settings = json!({ "theme_id": " purple " });
    assert_eq!(extract_theme_id(&settings).as_deref(), Some("purple"));
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
fn extract_bio_missing_returns_empty() {
    let settings = json!({});
    assert_eq!(extract_bio(&settings), "");
}

#[test]
fn extract_show_right_sidebar_defaults_to_true() {
    let settings = json!({});
    assert!(extract_show_right_sidebar(&settings));
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
fn extract_show_right_sidebar_reads_explicit_false() {
    let settings = json!({ "show_right_sidebar": false });
    assert!(!extract_show_right_sidebar(&settings));
}

#[test]
fn extract_show_right_sidebar_prefers_new_mode() {
    let settings = json!({
        "show_right_sidebar": true,
        "right_sidebar_mode": "off",
    });
    assert!(!extract_show_right_sidebar(&settings));
}

#[test]
fn extract_right_sidebar_mode_collapses_legacy_custom_to_on() {
    let settings = json!({ "right_sidebar_mode": "custom" });
    assert_eq!(extract_right_sidebar_mode(&settings), RightSidebarMode::On);
}

#[test]
fn extract_right_sidebar_mode_falls_back_to_legacy_bool() {
    let settings = json!({ "show_right_sidebar": false });
    assert_eq!(extract_right_sidebar_mode(&settings), RightSidebarMode::Off);
}

#[test]
fn extract_right_sidebar_components_defaults_to_all_at_default_state() {
    let settings = json!({});
    assert_eq!(
        extract_right_sidebar_components(&settings),
        default_right_sidebar_components()
    );
    // Every panel ships enabled.
    for setting in default_right_sidebar_components() {
        assert!(setting.enabled, "{:?}", setting.component);
    }
}

#[test]
fn extract_right_sidebar_components_preserves_order_and_backfills() {
    let settings = json!({
        "right_sidebar_components": [
            { "key": "bonsai", "enabled": false },
            { "key": "music", "enabled": true },
            { "key": "bogus", "enabled": true },
            { "key": "activity", "enabled": true },
        ]
    });
    let components = extract_right_sidebar_components(&settings);
    // Stored order kept for known entries, unknown dropped (including
    // the retired "pet" and "activity" keys), missing (daily,
    // visualizer) backfilled ENABLED at the end in ALL order: an existing
    // user's stored list predates newer panels, so they should appear
    // rather than silently stay hidden.
    assert_eq!(
        components,
        vec![
            RightSidebarComponentSetting {
                component: RightSidebarComponent::Bonsai,
                enabled: false,
            },
            RightSidebarComponentSetting {
                component: RightSidebarComponent::Music,
                enabled: true,
            },
            RightSidebarComponentSetting {
                component: RightSidebarComponent::Daily,
                enabled: true,
            },
            RightSidebarComponentSetting {
                component: RightSidebarComponent::Visualizer,
                enabled: true,
            },
        ]
    );
}

#[test]
fn extract_show_room_list_sidebar_defaults_to_true() {
    let settings = json!({});
    assert!(extract_show_room_list_sidebar(&settings));
}

#[test]
fn extract_show_room_list_sidebar_reads_explicit_false() {
    let settings = json!({ "show_room_list_sidebar": false });
    assert!(!extract_show_room_list_sidebar(&settings));
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
    assert_eq!(sanitize_username_input("...alice..."), "...alice...");
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
