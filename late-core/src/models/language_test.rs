use crate::models::language::*;

#[test]
fn normalize_id_accepts_direct_ids() {
    assert_eq!(normalize_id("en"), "en");
    assert_eq!(normalize_id("zh-hans"), "zh-hans");
}

#[test]
fn normalize_id_is_case_insensitive() {
    assert_eq!(normalize_id("EN"), "en");
    assert_eq!(normalize_id("ZH-HANS"), "zh-hans");
}

#[test]
fn normalize_id_accepts_chinese_aliases() {
    assert_eq!(normalize_id("zh"), "zh-hans");
    assert_eq!(normalize_id("zh-CN"), "zh-hans");
    assert_eq!(normalize_id("zh-cn"), "zh-hans");
    assert_eq!(normalize_id("zh_cn"), "zh-hans");
    assert_eq!(normalize_id("chinese"), "zh-hans");
}

#[test]
fn normalize_id_empty_or_unknown_falls_back_to_default() {
    assert_eq!(normalize_id(""), "en");
    assert_eq!(normalize_id("   "), "en");
    assert_eq!(normalize_id("klingon"), "en");
    // Traditional Chinese is not (yet) supported - falls back to English.
    assert_eq!(normalize_id("zh-Hant"), "en");
}

#[test]
fn label_for_id_returns_display_label() {
    assert_eq!(label_for_id("en"), "English");
    assert_eq!(label_for_id("zh-hans"), "中文(简体)");
    // Aliases resolve to the same label.
    assert_eq!(label_for_id("zh"), "中文(简体)");
    assert_eq!(label_for_id("unknown"), "English");
}

#[test]
fn cycle_id_wraps_through_all_options() {
    assert_eq!(cycle_id("en", true), "zh-hans");
    assert_eq!(cycle_id("zh-hans", true), "en"); // wraps around
    assert_eq!(cycle_id("en", false), "zh-hans"); // wraps backward
    assert_eq!(cycle_id("zh-hans", false), "en");
}

#[test]
fn cycle_id_unknown_starts_from_default() {
    assert_eq!(cycle_id("klingon", true), "zh-hans");
}

#[test]
fn default_id_is_english() {
    assert_eq!(DEFAULT_ID, "en");
}

#[test]
fn from_id_roundtrips_through_normalize() {
    for lang in OPTIONS {
        assert_eq!(from_id(lang.id()), *lang);
    }
}
