use super::*;

#[test]
fn tree_ascii_returns_lines_for_all_stages() {
    let stages = [
        Stage::Dead,
        Stage::Seed,
        Stage::Sprout,
        Stage::Sapling,
        Stage::Young,
        Stage::Mature,
        Stage::Ancient,
        Stage::Blossom,
    ];

    for stage in stages {
        for seed in 0..3 {
            let lines = tree_ascii(stage, seed, false);
            assert!(
                !lines.is_empty(),
                "stage {:?} seed {seed} has no art",
                stage
            );
        }
    }
}

#[test]
fn different_seeds_can_produce_different_variants() {
    let a = tree_ascii(Stage::Young, 0, false);
    let b = tree_ascii(Stage::Young, 1, false);
    let c = tree_ascii(Stage::Young, 2, false);

    assert!(a != b || b != c || a != c);
}

#[test]
fn high_stage_seeds_can_keep_style_with_different_forms() {
    let style = tree_variant_name(Stage::Mature, 0);
    assert_eq!(style, tree_variant_name(Stage::Mature, 8));
    assert_eq!(style, tree_variant_name(Stage::Mature, 16));

    let upright = tree_ascii(Stage::Mature, 0, false);
    let slim = tree_ascii(Stage::Mature, 8, false);
    let full = tree_ascii(Stage::Mature, 16, false);

    assert_ne!(upright, slim);
    assert_ne!(upright, full);
    assert_ne!(slim, full);
}

#[test]
fn status_specs_for_dead_tree_show_respawn_hint() {
    assert_eq!(
        status_line_specs(false, false),
        vec![StatusLineSpec::DeadHint]
    );
}

#[test]
fn status_specs_show_watering_status() {
    assert_eq!(status_line_specs(true, true), vec![]);
    assert_eq!(
        status_line_specs(true, false),
        vec![StatusLineSpec::WateredToday]
    );
}

#[test]
fn horizontal_window_allows_sway_when_art_fills_width() {
    assert_eq!(
        horizontal_window(22, 22, -1),
        HorizontalWindow {
            prefix_spaces: 0,
            skip_chars: 1,
            take_chars: 21,
        }
    );
    assert_eq!(
        horizontal_window(22, 22, 1),
        HorizontalWindow {
            prefix_spaces: 1,
            skip_chars: 0,
            take_chars: 21,
        }
    );
}

#[test]
fn horizontal_window_keeps_wide_art_centered() {
    assert_eq!(
        horizontal_window(72, 22, 0),
        HorizontalWindow {
            prefix_spaces: 25,
            skip_chars: 0,
            take_chars: 22,
        }
    );
}

#[test]
fn season_for_month_groups_calendar_months_correctly() {
    assert_eq!(season_for_month(3), Season::Spring);
    assert_eq!(season_for_month(5), Season::Spring);
    assert_eq!(season_for_month(6), Season::Summer);
    assert_eq!(season_for_month(8), Season::Summer);
    assert_eq!(season_for_month(9), Season::Autumn);
    assert_eq!(season_for_month(11), Season::Autumn);
    assert_eq!(season_for_month(12), Season::Winter);
    assert_eq!(season_for_month(1), Season::Winter);
    assert_eq!(season_for_month(2), Season::Winter);
}

#[test]
fn season_for_month_defaults_out_of_range_to_winter() {
    assert_eq!(season_for_month(0), Season::Winter);
    assert_eq!(season_for_month(13), Season::Winter);
}

#[test]
fn seasonal_leaf_color_leaves_non_foliated_stages_unchanged() {
    for stage in [
        Stage::Dead,
        Stage::Seed,
        Stage::Sprout,
        Stage::Ancient,
        Stage::Blossom,
    ] {
        for season in [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ] {
            assert_eq!(
                seasonal_leaf_color(stage, season),
                leaf_color_for_stage(stage),
                "non-foliated stage {stage:?} should ignore season {season:?}"
            );
        }
    }
}

#[test]
fn seasonal_leaf_color_tints_foliated_stages_by_season() {
    // Summer matches the year-round palette — it's the baseline.
    assert_eq!(
        seasonal_leaf_color(Stage::Young, Season::Summer),
        leaf_color_for_stage(Stage::Young),
    );
    // Spring / Autumn / Winter pull the foliated stages off the baseline.
    for season in [Season::Spring, Season::Autumn, Season::Winter] {
        assert_ne!(
            seasonal_leaf_color(Stage::Young, season),
            leaf_color_for_stage(Stage::Young),
            "season {season:?} should differ from the summer baseline"
        );
    }
}
