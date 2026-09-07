use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::chips::{ChipLedgerEntry, ChipMove};

use super::*;

fn text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn row(delta: i64, reason: &str, detail: Option<LedgerDetail>) -> LedgerRow {
    LedgerRow {
        entry: ChipLedgerEntry {
            delta,
            reason: reason.to_string(),
            source_ref: Some("ref".to_string()),
            created_at: Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap(),
        },
        detail,
    }
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

/// Every detail kind reads as a phrase a person would say.
#[test]
fn every_detail_has_copy() {
    let alice = || "alice".to_string();
    assert_eq!(
        detail(&LedgerDetail::GiftTo { username: alice() }),
        "to @alice"
    );
    assert_eq!(
        detail(&LedgerDetail::GiftFrom { username: alice() }),
        "from @alice"
    );
    assert_eq!(
        detail(&LedgerDetail::GildTo { username: alice() }),
        "to @alice"
    );
    assert_eq!(
        detail(&LedgerDetail::GildFrom {
            usernames: vec![alice(), "bob".to_string()]
        }),
        "from @alice, @bob"
    );
    assert_eq!(
        detail(&LedgerDetail::GamePayout {
            game: "minesweeper".to_string(),
            payout_kind: "daily_win_hard".to_string(),
        }),
        "minesweeper · daily win hard"
    );
    assert_eq!(
        detail(&LedgerDetail::CrownFrom { username: alice() }),
        "from @alice"
    );
    assert_eq!(detail(&LedgerDetail::PotTickets { count: 1 }), "1 ticket");
    assert_eq!(detail(&LedgerDetail::PotTickets { count: 3 }), "3 tickets");
    assert_eq!(
        detail(&LedgerDetail::PotWon { tickets: 42 }),
        "of 42 tickets"
    );
    assert_eq!(
        detail(&LedgerDetail::Quest {
            title: "Water your bonsai".to_string()
        }),
        "Water your bonsai"
    );
    assert_eq!(
        detail(&LedgerDetail::GalleryPlace {
            rank: 1,
            month: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        }),
        "#1 · Aug 2026"
    );
    assert_eq!(
        detail(&LedgerDetail::RoundFor { patrons: 4 }),
        "for 4 patrons"
    );
    assert_eq!(
        detail(&LedgerDetail::Song {
            title: "Never Gonna Give You Up".to_string()
        }),
        "Never Gonna Give You Up"
    );
    assert_eq!(
        detail(&LedgerDetail::Drink("Segfault Sour".to_string())),
        "Segfault Sour"
    );
    assert_eq!(
        detail(&LedgerDetail::Sku("bonsai-dynamic".to_string())),
        "bonsai-dynamic"
    );
    assert_eq!(
        detail(&LedgerDetail::Link("https://example.com".to_string())),
        "https://example.com"
    );
    assert_eq!(
        detail(&LedgerDetail::StreakDay("7".to_string())),
        "streak day 7"
    );
}

#[test]
fn a_gift_row_names_the_other_party() {
    let sent = row(
        -300,
        "chip_gift_sent",
        Some(LedgerDetail::GiftTo {
            username: "alice".to_string(),
        }),
    );
    assert_eq!(
        text(&row_line(&sent, 80)),
        "Sep 06      -300  gift sent           to @alice"
    );
}

#[test]
fn a_gild_row_names_who_gilded_whom() {
    let sent = row(
        -500,
        "chip_gild_sent",
        Some(LedgerDetail::GildTo {
            username: "author".to_string(),
        }),
    );
    assert_eq!(
        text(&row_line(&sent, 80)),
        "Sep 06      -500  gild sent           to @author"
    );
    let received = row(
        333,
        "chip_gild_received",
        Some(LedgerDetail::GildFrom {
            usernames: vec!["alice".to_string(), "bob".to_string()],
        }),
    );
    assert_eq!(
        text(&row_line(&received, 80)),
        "Sep 06      +333  gild received       from @alice, @bob"
    );
}

/// An arcade payout says which game and which milestone paid.
#[test]
fn a_payout_row_names_the_game() {
    let puzzle = row(
        100,
        "daily_puzzle_win",
        Some(LedgerDetail::GamePayout {
            game: "sudoku".to_string(),
            payout_kind: "daily_win_hard".to_string(),
        }),
    );
    assert_eq!(
        text(&row_line(&puzzle, 80)),
        "Sep 06      +100  daily puzzle        sudoku · daily win hard"
    );
}

/// A row the board ignores keeps its detail and only loses its colour.
#[test]
fn off_board_rows_are_dimmed_and_keep_their_detail() {
    let poker = row(2400, "poker_payout", None);
    assert_eq!(
        text(&row_line(&poker, 80)),
        "Sep 06    +2,400  poker payout      "
    );
    let shop = row(-8000, "shop_purchase", Some(LedgerDetail::Sku("username_glow_month".to_string())));
    assert_eq!(
        text(&row_line(&shop, 80)),
        "Sep 06    -8,000  shop                username_glow_month"
    );
    let quest = row(500, "quest_reward", None);
    assert_eq!(
        text(&row_line(&quest, 80)),
        "Sep 06      +500  quest reward      "
    );
}

/// A reason nothing in the roster wrote (seed rows, hand-written rows)
/// still renders rather than panicking, and counts like the board counts it.
#[test]
fn an_unknown_reason_renders_as_other() {
    let seed = row(5075, "leaderboard_seed", None);
    assert_eq!(
        text(&row_line(&seed, 80)),
        "Sep 06    +5,075  other             "
    );
}

/// A long detail is clipped to the width; a row without one ends at the label.
#[test]
fn details_show_and_clip() {
    let drink = row(
        -400,
        "drink_purchase",
        Some(LedgerDetail::Drink("Segfault Sour".to_string())),
    );
    assert_eq!(
        text(&row_line(&drink, 80)),
        "Sep 06      -400  drink               Segfault Sour"
    );
    let news = row(
        250,
        "news_shared",
        Some(LedgerDetail::Link(
            "https://example.com/a/very/long/path/that/keeps/going".to_string(),
        )),
    );
    let line = text(&row_line(&news, 60));
    assert_eq!(line.chars().count(), 60);
    assert!(line.ends_with('…'));
    let gild = row(1333, "chip_gild_received", None);
    assert_eq!(
        text(&row_line(&gild, 80)),
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
        "dim rows do not count for Top Chips"
    );
}
