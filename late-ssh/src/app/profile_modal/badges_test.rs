//! The badge codes are authored in four places: `profile_award` names the
//! category and its code, the Leaderboards guide explains how to earn it, and
//! the help modal lists it. Nothing in the type system ties the last two to
//! the first, so a badge can ship working but undocumented — which is exactly
//! what happened when A Dark Room's second ending was added.

use late_core::models::profile_award::{
    MILESTONE_AWARD_CATEGORIES, SINGLE_HOLDER_AWARD_CATEGORIES, all_award_categories,
    award_badge, award_category_code,
};

/// Every badge granted outside the ranked monthly boards. The milestones are
/// one list already; the single-holder monthly awards (the crown, Late Time)
/// are documented in the same two places, so they are checked alongside them.
fn undocumentable_badges() -> Vec<String> {
    MILESTONE_AWARD_CATEGORIES
        .iter()
        .chain(SINGLE_HOLDER_AWARD_CATEGORIES.iter())
        .map(|category| award_badge(category, 1))
        .collect()
}

use crate::app::help_modal::data::{HelpTopic, lines_for};
use crate::app::profile_modal::badges;

/// Every line of the rendered guide, flattened back to plain text.
fn guide_text() -> String {
    badges::guide_lines()
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every badge a chat label can carry, ranked monthly boards included. The
/// settings modal's Chat badges picker is built from the same list
/// (`chat_badge_rows`), so it is never missing one; the guide is written by
/// hand and this is what keeps it level with the picker.
#[test]
fn the_leaderboards_guide_explains_every_badge() {
    let guide = guide_text();
    let missing: Vec<&str> = all_award_categories()
        .into_iter()
        .map(award_category_code)
        .filter(|code| !guide.contains(code))
        .collect();

    assert!(
        missing.is_empty(),
        "these badges are granted but absent from the Leaderboards badge guide: {missing:?}"
    );
}

/// The guide reads in the picker's order (the order a chat label stacks
/// badges), so the two lists line up row for row.
#[test]
fn the_leaderboards_guide_lists_badges_in_the_pickers_order() {
    let guide = guide_text();
    let entry_codes: Vec<&str> = guide
        .lines()
        .filter_map(|line| line.strip_prefix("  "))
        .filter_map(|entry| entry.split_whitespace().next())
        .collect();
    let picker_codes: Vec<&str> = all_award_categories()
        .into_iter()
        .map(award_category_code)
        .collect();
    assert_eq!(entry_codes, picker_codes);
}

/// Every help topic's text, since the badge legend is one page of many.
fn help_text() -> String {
    HelpTopic::ALL
        .iter()
        .flat_map(|topic| lines_for(*topic, false, "https://late.sh/listen"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_help_modal_lists_every_milestone_badge() {
    let help = help_text();
    let missing: Vec<String> = undocumentable_badges()
        .into_iter()
        .filter(|code| !help.contains(code.as_str()))
        .collect();

    assert!(
        missing.is_empty(),
        "these badges are granted but absent from the help modal: {missing:?}"
    );
}

/// The overview lists every award it is handed. It used to cut off at six with
/// a "+N more" tail, which hid exactly the badges a long-running account earned.
#[test]
fn every_award_is_listed() {
    use chrono::{NaiveDate, Utc};
    use late_core::models::profile_award::ProfileAward;
    use uuid::Uuid;

    let awards: Vec<ProfileAward> = MILESTONE_AWARD_CATEGORIES
        .iter()
        .map(|category| ProfileAward {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            category: (*category).to_string(),
            period_month: NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid month"),
            rank: 1,
            score_value: 0,
            awarded_at: Utc::now(),
        })
        .collect();

    let rendered: String = badges::badge_lines(&awards, 200)
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect();

    for award in &awards {
        assert!(
            rendered.contains(&award.badge()),
            "{} is missing from the rendered badge list: {rendered}",
            award.badge()
        );
    }
    assert!(
        !rendered.contains("more"),
        "badges were truncated: {rendered}"
    );
}

/// Every badge is listed however many there are: the rows wrap to the
/// width, and nothing is folded into a "+N more".
#[test]
fn badge_lines_wrap_to_the_width_and_keep_every_badge() {
    use chrono::{NaiveDate, Utc};
    use late_core::models::profile_award::ProfileAward;
    use uuid::Uuid;

    let awards: Vec<ProfileAward> = (0..12)
        .map(|index| ProfileAward {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            category: "artboard".to_string(),
            period_month: NaiveDate::from_ymd_opt(2025, 1 + (index % 12) as u32, 1)
                .expect("valid month"),
            rank: 1,
            score_value: 0,
            awarded_at: Utc::now(),
        })
        .collect();
    let width = 40;
    let lines = badges::badge_lines(&awards, width);
    assert!(
        lines.len() > 1,
        "twelve badges do not fit one 40-column row"
    );
    for line in &lines {
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(
            text.chars().count() <= width,
            "a badge row overflows the width: {text:?}"
        );
    }
    let rendered: String = lines
        .iter()
        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
        .collect();
    for award in &awards {
        let badge = format!("[{} {}]", award.badge(), award.month_label());
        assert!(rendered.contains(&badge), "{badge} was folded away");
    }
}
