use super::*;

#[test]
fn branch_goal_scales_with_growth_stage() {
    let date = NaiveDate::from_ymd_opt(2026, 4, 24).unwrap();

    for stage in [Stage::Seed, Stage::Sprout] {
        let goal = branch_goal_for(stage, 42, date);
        assert!((1..=2).contains(&goal));
        assert_eq!(goal, branch_goal_for(stage, 42, date));
    }

    let goal = branch_goal_for(Stage::Sapling, 42, date);
    assert!((2..=3).contains(&goal));

    for stage in [Stage::Young, Stage::Mature] {
        let goal = branch_goal_for(stage, 42, date);
        assert!((3..=4).contains(&goal));
    }

    for stage in [Stage::Ancient, Stage::Blossom] {
        let goal = branch_goal_for(stage, 42, date);
        assert!((4..=5).contains(&goal));
    }
}

#[test]
fn cut_selected_records_branch_once() {
    let date = NaiveDate::from_ymd_opt(2026, 4, 24).unwrap();
    let mut state = BonsaiCareState::fallback(date, 42, Stage::Seed);
    let targets = [BranchTarget {
        id: 7,
        x: 1,
        y: 1,
        glyph: '/',
    }];

    state.set_cursor(1, 1);
    assert_eq!(state.cut_at_cursor(&targets), Some(7));
    assert_eq!(state.cut_at_cursor(&targets), None);
    assert_eq!(state.branches_done(), 1);
}

#[test]
fn tree_char_detection_includes_all_foliage_textures() {
    for ch in ['@', '#', '*', '.', ',', '\'', 'o', 'O'] {
        assert!(is_tree_char(Some(ch)), "missing foliage glyph {ch}");
    }
}
