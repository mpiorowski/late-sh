use chrono::{TimeZone, Utc};
use late_core::models::chips::{ChipLedgerEntry, ChipMove};
use uuid::Uuid;

use super::*;

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn entry(delta: i64, reason: &str, source_ref: Option<&str>) -> ChipLedgerEntry {
    ChipLedgerEntry {
        delta,
        reason: reason.to_string(),
        source_ref: source_ref.map(str::to_string),
        created_at: Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap(),
    }
}

fn no_names(_: Uuid) -> Option<String> {
    None
}

/// Every reason has copy, and no two reasons collide on a label except the
/// Super Snake pair, which is one game seen from both sides.
#[test]
fn every_move_has_a_label() {
    let mut seen = std::collections::HashSet::new();
    for mv in ChipMove::ALL {
        let label = label(*mv);
        assert!(!label.is_empty(), "{mv:?} has no label");
        let unique = seen.insert(label);
        assert!(
            unique || matches!(mv, ChipMove::SsnakeArenaLost),
            "{mv:?} shares its label {label:?} with another reason"
        );
    }
}

#[test]
fn a_gift_row_names_the_other_party() {
    let alice = Uuid::now_v7();
    let names = |id: Uuid| (id == alice).then(|| "alice".to_string());
    let sent = entry(-300, "chip_gift_sent", Some(&alice.to_string()));
    let line = text(&row_line(&sent, 80, names));
    assert_eq!(line, "Sep 06      -300  gift sent           to @alice  off");

    let received = entry(300, "chip_gift_received", Some(&alice.to_string()));
    let line = text(&row_line(&received, 80, names));
    assert_eq!(
        line,
        "Sep 06      +300  gift received       from @alice  off"
    );
}

/// A row the board ignores is marked; a row it counts is not.
#[test]
fn off_board_rows_are_marked() {
    let poker = entry(2400, "poker_payout", Some("hand"));
    assert_eq!(
        text(&row_line(&poker, 80, no_names)),
        "Sep 06    +2,400  poker payout        off"
    );
    let quest = entry(500, "quest_reward", Some("assignment-id"));
    assert_eq!(
        text(&row_line(&quest, 80, no_names)),
        "Sep 06      +500  quest reward      "
    );
}

/// A reason nothing in the roster wrote (seed rows, hand-written rows)
/// still renders rather than panicking, and counts like the board counts it.
#[test]
fn an_unknown_reason_renders_as_other() {
    let seed = entry(5075, "leaderboard_seed", Some("leaderboard-v2"));
    assert_eq!(
        text(&row_line(&seed, 80, no_names)),
        "Sep 06    +5,075  other             "
    );
}

/// A readable ref is shown and clipped to the width; an id is not shown.
#[test]
fn readable_refs_show_and_clip() {
    let drink = entry(-400, "drink_purchase", Some("Segfault Sour"));
    assert_eq!(
        text(&row_line(&drink, 80, no_names)),
        "Sep 06      -400  drink               Segfault Sour"
    );
    let news = entry(
        250,
        "news_shared",
        Some("https://example.com/a/very/long/path/that/keeps/going"),
    );
    let line = text(&row_line(&news, 60, no_names));
    assert_eq!(line.chars().count(), 60);
    assert!(line.ends_with('…'));
    let gild = entry(
        1333,
        "chip_gild_received",
        Some(&Uuid::now_v7().to_string()),
    );
    assert_eq!(
        text(&row_line(&gild, 80, no_names)),
        "Sep 06    +1,333  gild received     "
    );
}

#[test]
fn thousands_groups_and_keeps_the_sign() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1_000), "1,000");
    assert_eq!(thousands(168_092), "168,092");
    assert_eq!(thousands(-1_234_567), "-1,234,567");
}

#[test]
fn the_summary_reads_balance_then_month() {
    assert_eq!(
        text(&summary_line(Some(168_092), 860)),
        "balance 168,092  ·  this month +860"
    );
    assert_eq!(text(&summary_line(None, -50)), "this month -50");
    assert_eq!(
        text(&off_board_note()),
        "rows marked off do not count for Top Chips"
    );
}
