use chrono::NaiveDate;
use late_core::models::quest::DailyQuestStreakSnapshot;
use ratatui::text::Line;

use super::{
    super::svc::{QuestItem, QuestSnapshot},
    heading_line, item_line, section_line, strip_height_for, strip_lines,
};

fn text(line: &Line) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn quest_item(title: &str, difficulty: &str, target: i32, progress: i32) -> QuestItem {
    QuestItem {
        title: title.to_string(),
        description: String::new(),
        cadence: "daily".to_string(),
        domain: "arcade".to_string(),
        difficulty: difficulty.to_string(),
        progress,
        target,
        reward_chips: 375,
        completed_at: None,
        period_end: NaiveDate::from_ymd_opt(2026, 8, 5).unwrap(),
    }
}

#[test]
fn heading_shows_streak_meter_and_next_bonus() {
    let streak = DailyQuestStreakSnapshot {
        consecutive_days: 3,
        bonus_level: 2,
        last_completed_date: NaiveDate::from_ymd_opt(2026, 8, 1),
        current_bonus_chips: 200,
        next_bonus_chips: 300,
    };
    let rendered = text(&heading_line(&streak, true));
    assert!(
        rendered.contains("▰▰▰▱▱▱"),
        "3-day streak fills 3 of 6 slots: {rendered}"
    );
    assert!(rendered.contains("3 days"), "{rendered}");
    assert!(rendered.contains("next daily +300 bonus"), "{rendered}");

    let empty = text(&heading_line(&DailyQuestStreakSnapshot::default(), true));
    assert!(empty.contains("▱▱▱▱▱▱"), "{empty}");
    assert!(empty.contains("win any daily quest"), "{empty}");

    let capped = DailyQuestStreakSnapshot {
        consecutive_days: 9,
        bonus_level: 5,
        last_completed_date: NaiveDate::from_ymd_opt(2026, 8, 1),
        current_bonus_chips: 500,
        next_bonus_chips: 500,
    };
    let rendered = text(&heading_line(&capped, true));
    assert!(
        rendered.contains("▰▰▰▰▰▰"),
        "capped streak fills the meter: {rendered}"
    );
    assert!(rendered.contains("max bonus +500/day"), "{rendered}");
}

#[test]
fn section_line_counts_done_and_sums_open_chips() {
    let mut done = quest_item("Win easy Sudoku", "easy", 1, 1);
    done.reward_chips = 150;
    let open = quest_item("Score 10,000 in Snake", "medium", 10_000, 0);
    let rendered = text(&section_line(
        "Daily",
        &[done.clone(), open],
        "resets 00:00 UTC",
    ));
    assert!(rendered.contains("Daily 1/2"), "{rendered}");
    assert!(rendered.contains("+375 chips to earn"), "{rendered}");
    assert!(rendered.contains("resets 00:00 UTC"), "{rendered}");

    let rendered = text(&section_line("Daily", &[done], "resets 00:00 UTC"));
    assert!(rendered.contains("all clear ✓"), "{rendered}");
}

#[test]
fn item_line_shows_progress_bar_only_for_multi_step_quests() {
    let scored = text(&item_line(
        &quest_item("Score 10,000 in Snake", "medium", 10_000, 4_000),
        90,
    ));
    assert!(scored.contains("○"), "{scored}");
    assert!(
        scored.contains("▰▰▰▱▱▱▱▱"),
        "4,000 of 10,000 fills 3 of 8 slots: {scored}"
    );
    assert!(scored.contains("4,000/10,000"), "{scored}");
    assert!(scored.contains("+375"), "{scored}");

    let single = text(&item_line(&quest_item("Win easy Sudoku", "easy", 1, 0), 90));
    assert!(
        !single.contains("▱"),
        "single-step quests get no bar: {single}"
    );
    assert!(!single.contains("0/1"), "{single}");

    let narrow = text(&item_line(
        &quest_item("Score 10,000 in Snake", "medium", 10_000, 4_000),
        60,
    ));
    assert!(
        !narrow.contains("▰"),
        "narrow terminals drop the bar: {narrow}"
    );
    assert!(narrow.contains("4,000/10,000"), "{narrow}");

    let done = text(&item_line(
        &quest_item("Score 10,000 in Snake", "medium", 10_000, 12_000),
        90,
    ));
    assert!(done.contains("✓"), "{done}");
    assert!(
        !done.contains("10,000/10,000"),
        "finished quests drop the progress tail: {done}"
    );
}

#[test]
fn strip_height_matches_rendered_lines_plus_separator() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
    let snapshot = QuestSnapshot {
        user_id: None,
        daily: vec![
            quest_item("Win easy Sudoku", "easy", 1, 0),
            quest_item("Score 10,000 in Snake", "medium", 10_000, 0),
        ],
        weekly: vec![quest_item("Win draw-3 Solitaire", "hard", 1, 0)],
        daily_streak: DailyQuestStreakSnapshot::default(),
    };
    let lines = strip_lines(&snapshot, true, today, 90);
    assert_eq!(strip_height_for(&snapshot, true), lines.len() as u16 + 1);

    let loading = QuestSnapshot::default();
    let lines = strip_lines(&loading, false, today, 90);
    assert_eq!(lines.len(), 2, "top padding plus the heading");
    assert_eq!(strip_height_for(&loading, false), 3);
}

#[test]
fn weekly_reset_note_counts_days_to_exclusive_period_end() {
    let today = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
    let item = quest_item("Win draw-3 Solitaire", "hard", 1, 0);
    let rendered = text(
        &strip_lines(
            &QuestSnapshot {
                user_id: None,
                daily: Vec::new(),
                weekly: vec![item],
                daily_streak: DailyQuestStreakSnapshot::default(),
            },
            true,
            today,
            90,
        )[3],
    );
    assert!(rendered.contains("resets in 4d"), "{rendered}");
}
