use super::i18n::*;

#[test]
fn tr_returns_english_for_default_locale() {
    set_current_by_id("en");
    assert_eq!(tr("settings.username"), "Username");
}

#[test]
fn tr_returns_chinese_when_set() {
    set_current_by_id("zh-hans");
    assert_eq!(tr("settings.username"), "用户名");
}

#[test]
fn tr_falls_back_to_english_when_translation_missing() {
    // Key exists only in en.toml, not zh-hans.toml.
    set_current_by_id("zh-hans");
    assert_eq!(tr("settings.only_in_english"), "English-only value");
}

#[test]
fn tr_falls_back_to_key_when_missing_everywhere() {
    set_current_by_id("en");
    assert_eq!(tr("nonexistent.key"), "nonexistent.key");
    set_current_by_id("zh-hans");
    assert_eq!(tr("nonexistent.key"), "nonexistent.key");
}

#[test]
fn trf_substitutes_placeholders_in_english() {
    set_current_by_id("en");
    assert_eq!(trf("lobby.chips", &[("n", "42")]), "42 chips");
}

#[test]
fn trf_substitutes_placeholders_in_chinese() {
    set_current_by_id("zh-hans");
    assert_eq!(trf("lobby.chips", &[("n", "42")]), "42 筹码");
}

#[test]
fn set_current_accepts_aliases_and_unknowns() {
    set_current_by_id("zh-CN");
    assert_eq!(current_id(), "zh-hans");
    set_current_by_id("chinese");
    assert_eq!(current_id(), "zh-hans");
    set_current_by_id("");
    assert_eq!(current_id(), "en");
    set_current_by_id("klingon");
    assert_eq!(current_id(), "en");
}

#[test]
fn thread_local_locale_is_independent_across_threads() {
    set_current_by_id("en");
    let other = std::thread::spawn(|| {
        set_current_by_id("zh-hans");
        current_id()
    })
    .join()
    .unwrap();
    // The child thread's locale must not leak into this one - this is the
    // multi-user SSH server invariant: concurrent sessions stay isolated.
    assert_eq!(other, "zh-hans");
    assert_eq!(current_id(), "en");
}

#[test]
fn cycle_id_wraps_through_options() {
    assert_eq!(cycle_id("en", true), "zh-hans");
    assert_eq!(cycle_id("zh-hans", true), "en");
    assert_eq!(cycle_id("en", false), "zh-hans");
}

#[test]
fn label_for_id_returns_display_label() {
    assert_eq!(label_for_id("en"), "English");
    assert_eq!(label_for_id("zh-hans"), "中文(简体)");
}
