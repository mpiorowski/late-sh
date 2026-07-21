use super::*;
use serde_json::json;

fn template(key: &str, difficulty: &str, domain: &str, kind: &str) -> QuestTemplate {
    QuestTemplate {
        id: Uuid::now_v7(),
        created: DateTime::<Utc>::UNIX_EPOCH,
        updated: DateTime::<Utc>::UNIX_EPOCH,
        key: key.to_string(),
        title: key.to_string(),
        description: key.to_string(),
        cadence: "daily".to_string(),
        bucket: "skill".to_string(),
        domain: domain.to_string(),
        difficulty: difficulty.to_string(),
        kind: kind.to_string(),
        params: json!({}),
        target: 1,
        reward_chips: 100,
        weight: 100,
        active: true,
        starts_at: None,
        ends_at: None,
    }
}

#[test]
fn slots_draw_arcade_by_difficulty_and_skip_non_arcade_quests() {
    let period_start = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
    let templates = vec![
        template("easy_arcade", "easy", "puzzle", "daily_puzzle_win"),
        template("medium_arcade", "medium", "arcade", "arcade_score"),
        template("hard_arcade", "hard", "arcade", "arcade_score"),
        template("medium_other", "medium", "bonsai", "bonsai_watered"),
    ];

    let slot_one = choose_template(&templates, "daily", period_start, 1, &[]).unwrap();
    let slot_two = choose_template(&templates, "daily", period_start, 2, &[]).unwrap();
    let weekly = choose_template(&templates, "weekly", period_start, 1, &[]).unwrap();

    assert_eq!(slot_one.key, "easy_arcade");
    assert_eq!(slot_two.key, "medium_arcade");
    assert_eq!(weekly.key, "hard_arcade");
}

#[test]
fn medium_slot_keeps_same_domain_puzzles_after_easy_pick() {
    // Regression: slot 1 (easy) always consumes a puzzle-domain quest. The
    // medium slot must still be able to draw a medium *puzzle*, not only the
    // arcade-score games. Domain avoidance used to lock every puzzle out of
    // slot 2, leaving Rubik's Cube to dominate the medium draw.
    let templates = vec![
        template("easy_sudoku", "easy", "puzzle", "daily_puzzle_win"),
        template("medium_sudoku", "medium", "puzzle", "daily_puzzle_win"),
        template("medium_2048", "medium", "arcade", "arcade_score"),
        template(
            "solve_rubiks_cube",
            "medium",
            "arcade",
            "arcade_puzzle_solved",
        ),
    ];
    let easy_pick = templates[0].id;

    let pool = filtered_pool(
        &templates,
        Some("medium"),
        Some(QuestSource::Arcade),
        &[easy_pick],
    );
    let keys: Vec<&str> = pool.iter().map(|template| template.key.as_str()).collect();

    assert_eq!(pool.len(), 3, "full medium bucket must remain: {keys:?}");
    assert!(
        keys.contains(&"medium_sudoku"),
        "medium puzzle must stay eligible after an easy puzzle pick: {keys:?}"
    );
    assert!(keys.contains(&"medium_2048"));
    assert!(keys.contains(&"solve_rubiks_cube"));
}

#[test]
fn daily_streak_bonus_starts_on_second_consecutive_full_daily_and_caps() {
    let day = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
    let next_day = day.checked_add_signed(Duration::days(1)).unwrap();
    let sixth_day = day.checked_add_signed(Duration::days(5)).unwrap();
    let skipped_day = day.checked_add_signed(Duration::days(7)).unwrap();

    assert_eq!(
        next_daily_streak_advance(None, day),
        Some(DailyStreakAdvance {
            consecutive_days: 1,
            bonus_level: 0,
            reward_chips: 0
        })
    );
    assert_eq!(
        next_daily_streak_advance(Some((day, 1)), next_day),
        Some(DailyStreakAdvance {
            consecutive_days: 2,
            bonus_level: 1,
            reward_chips: 100
        })
    );
    assert_eq!(
        next_daily_streak_advance(Some((sixth_day, 6)), sixth_day),
        None
    );
    assert_eq!(
        next_daily_streak_advance(Some((day, 5)), skipped_day),
        Some(DailyStreakAdvance {
            consecutive_days: 1,
            bonus_level: 0,
            reward_chips: 0
        })
    );
    assert_eq!(
        next_daily_streak_advance(Some((sixth_day, 6)), sixth_day.succ_opt().unwrap()),
        Some(DailyStreakAdvance {
            consecutive_days: 7,
            bonus_level: 5,
            reward_chips: 500
        })
    );
}
