use super::*;
use chrono::Duration;

#[test]
fn stage_thresholds_match_growth_ranges() {
    let cases = [
        (true, 0, Stage::Seed),
        (true, 99, Stage::Seed),
        (true, 100, Stage::Sprout),
        (true, 199, Stage::Sprout),
        (true, 200, Stage::Sapling),
        (true, 299, Stage::Sapling),
        (true, 300, Stage::Young),
        (true, 399, Stage::Young),
        (true, 400, Stage::Mature),
        (true, 499, Stage::Mature),
        (true, 500, Stage::Ancient),
        (true, 599, Stage::Ancient),
        (true, 600, Stage::Blossom),
        (true, 700, Stage::Blossom),
        (false, 999, Stage::Dead),
    ];

    for (is_alive, growth_points, expected) in cases {
        assert_eq!(stage_for(is_alive, growth_points), expected);
    }
}

#[test]
fn can_water_and_days_since_watered_track_today() {
    let today = BonsaiService::today();

    assert_eq!(days_since_watered_on(None, today), None);
    assert!(can_water_on(true, None, today));

    assert_eq!(days_since_watered_on(Some(today), today), Some(0));
    assert!(!can_water_on(true, Some(today), today));

    assert_eq!(
        days_since_watered_on(Some(today - Duration::days(1)), today),
        Some(1)
    );
    assert!(can_water_on(true, Some(today - Duration::days(1)), today));
}

#[test]
fn is_wilting_depends_on_age_or_days_since_watered() {
    assert!(!is_wilting_state(true, 1, None));
    assert!(is_wilting_state(true, 2, None));
    assert!(!is_wilting_state(true, 10, Some(1)));
    assert!(is_wilting_state(true, 10, Some(2)));
    assert!(!is_wilting_state(false, 10, Some(5)));
}

#[test]
fn should_die_after_seven_dry_days() {
    let today = BonsaiService::today();
    assert!(!should_die(today - Duration::days(6), today));
    assert!(should_die(today - Duration::days(7), today));
    assert!(should_die(today - Duration::days(20), today));
}

#[test]
fn share_label_reflects_alive_and_dead_states() {
    assert_eq!(share_label(true, 12), "ADMIRE my tree (Day 12)");
    assert_eq!(share_label(false, 12), "ADMIRE my tree [RIP]");
}

#[test]
fn share_art_with_care_includes_uncut_branch_glyphs() {
    let date = NaiveDate::from_ymd_opt(2026, 5, 13).unwrap();
    let stage = Stage::Mature;
    let seed = 42;
    let care = BonsaiCareState::fallback(date, seed, stage);
    let base_lines = super::super::ui::tree_ascii(stage, seed, false);
    let targets = branch_targets_for(stage, seed, date, &base_lines, care.branch_goal);
    let target = targets.first().expect("branch target");

    let base_char = base_lines[target.y].chars().nth(target.x);
    assert_eq!(base_char, Some(' '));

    let shared = share_art_with_care(stage, seed, &care);
    let shared_char = shared
        .lines()
        .nth(target.y)
        .and_then(|line| line.chars().nth(target.x));

    assert_eq!(shared_char, Some(target.glyph));
}
