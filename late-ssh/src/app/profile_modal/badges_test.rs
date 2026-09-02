//! The badge codes are authored in four places: `profile_award` names the
//! category and its code, the Leaderboards guide explains how to earn it, and
//! the help modal lists it. Nothing in the type system ties the last two to
//! the first, so a badge can ship working but undocumented — which is exactly
//! what happened when A Dark Room's second ending was added.

use late_core::models::profile_award::{
    CROWN_AWARD_CATEGORY, MILESTONE_AWARD_CATEGORIES, award_badge,
};

/// Every badge granted outside the ranked monthly boards. The milestones are
/// one list already; the crown is monthly but rankless, and it is documented
/// in the same two places, so it is checked alongside them.
fn undocumentable_badges() -> Vec<String> {
    MILESTONE_AWARD_CATEGORIES
        .iter()
        .chain(std::iter::once(&CROWN_AWARD_CATEGORY))
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

#[test]
fn the_leaderboards_guide_explains_every_milestone_badge() {
    let guide = guide_text();
    let missing: Vec<String> = undocumentable_badges()
        .into_iter()
        .filter(|code| !guide.contains(code.as_str()))
        .collect();

    assert!(
        missing.is_empty(),
        "these badges are granted but absent from the Leaderboards badge guide: {missing:?}"
    );
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

    let rendered: String = badges::badge_lines(&awards)
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
    assert!(!rendered.contains("more"), "badges were truncated: {rendered}");
}
