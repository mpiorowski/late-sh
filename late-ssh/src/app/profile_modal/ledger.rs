//! The chips section of a profile: one plain-English line per ledger row.
//!
//! The ledger is public, so this is where anyone can audit a place on the
//! Top Chips board: every row says what it paid for, and rows the board
//! ignores are dimmed. Every `ChipMove` gets a label and every resolved
//! `LedgerDetail` gets copy, both exhaustive, so a new reason or detail
//! cannot ship without words. What a ref points at is decided in
//! `profile::ledger`; this module only says it.

use chrono::{DateTime, Utc};
use late_core::models::chips::ChipMove;
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;
use crate::app::profile::ledger::{LedgerDetail, LedgerRow};

/// Column widths for a row: `Sep 06  +1,333  gild received  <detail>`.
const DATE_WIDTH: usize = 6;
const DELTA_WIDTH: usize = 8;
const LABEL_WIDTH: usize = 18;
/// Trailing marker for a row the Top Chips board does not count.

/// What the row was, in a couple of words.
pub(crate) fn label(mv: ChipMove) -> &'static str {
    match mv {
        ChipMove::LegacyTableCredit => "table payout",
        ChipMove::LegacyTableDebit => "table bet",
        ChipMove::BlackjackBet => "blackjack bet",
        ChipMove::BlackjackPayout => "blackjack payout",
        ChipMove::PokerBet => "poker bet",
        ChipMove::PokerPayout => "poker payout",
        ChipMove::BonsaiWatered => "bonsai watered",
        ChipMove::FloorRestore => "floor restored",
        ChipMove::GiftSent => "gift sent",
        ChipMove::GiftReceived => "gift received",
        ChipMove::InitialBalance => "starting chips",
        ChipMove::GildSent => "gild sent",
        ChipMove::GildReceived => "gild received",
        ChipMove::CrownTaken => "crown taken",
        ChipMove::PotTicket => "pot ticket",
        ChipMove::PotWon => "pot won",
        ChipMove::NewsShared => "news shared",
        ChipMove::ArtboardPrize => "gallery prize",
        ChipMove::SongQueued => "song queued",
        ChipMove::RoundPurchase => "bought a round",
        ChipMove::DrinkPurchase => "drink",
        ChipMove::ShopPurchase => "shop",
        ChipMove::QuestReward => "quest reward",
        ChipMove::DailyQuestStreakReward => "quest streak",
        ChipMove::DailyPuzzleWin => "daily puzzle",
        ChipMove::AsterionEscape => "asterion escape",
        ChipMove::DailyChessWin => "chess win",
        ChipMove::DailyChess960Win => "chess960 win",
        ChipMove::DailyBattleshipWin => "battleship win",
        ChipMove::DailyConnectFourWin => "connect four win",
        ChipMove::DailyReversiWin => "reversi win",
        ChipMove::DailyCheckersWin => "checkers win",
        ChipMove::DailyBackgammonWin => "backgammon win",
        ChipMove::DailyBriscolaWin => "briscola win",
        ChipMove::TronWin => "tron win",
        ChipMove::SsnakeArenaEarned => "super snake",
        ChipMove::SsnakeArenaLost => "super snake",
        ChipMove::GreendragonDragonSlain => "dragon slain",
        ChipMove::DarkroomEscape => "dark room escape",
        ChipMove::DarkroomBeaconEscape => "dark room beacon",
        ChipMove::NethackAmuletAcquired => "nethack amulet",
        ChipMove::NethackAscension => "nethack ascension",
        ChipMove::DcssOrbFound => "dcss orb",
        ChipMove::DcssOrbEscape => "dcss escape",
        ChipMove::BrogueEscape => "brogue escape",
        ChipMove::BrogueMastery => "brogue mastery",
        ChipMove::LateaniaArchdemonDefeat => "lateania: archdemon",
        ChipMove::LateaniaFrontierKingDefeat => "lateania: frontier king",
        ChipMove::LateaniaSunderingDeepDefeat => "lateania: sundering deep",
        ChipMove::LateaniaKaethyrAscendantDefeat => "lateania: kaethyr",
    }
}

/// The resolved detail, in a couple of words.
pub(crate) fn detail(detail: &LedgerDetail) -> String {
    match detail {
        LedgerDetail::GiftTo { username } => format!("to @{username}"),
        LedgerDetail::GiftFrom { username } => format!("from @{username}"),
        LedgerDetail::GildTo { username } => format!("to @{username}"),
        LedgerDetail::GildFrom { usernames } => {
            let names: Vec<String> = usernames.iter().map(|name| format!("@{name}")).collect();
            format!("from {}", names.join(", "))
        }
        LedgerDetail::GamePayout { game, payout_kind } => {
            format!("{} · {}", humanize(game), humanize(payout_kind))
        }
        LedgerDetail::CrownFrom { username } => format!("from @{username}"),
        LedgerDetail::PotTickets { count } => plural(*count, "ticket"),
        LedgerDetail::PotWon { tickets } => format!("of {}", plural(*tickets, "ticket")),
        LedgerDetail::Quest { title } => title.clone(),
        LedgerDetail::GalleryPlace { rank, month } => {
            format!("#{rank} · {}", month.format("%b %Y"))
        }
        LedgerDetail::RoundFor { patrons } => format!("for {}", plural(*patrons, "patron")),
        LedgerDetail::Song { title } => title.clone(),
        LedgerDetail::Drink(drink) => drink.clone(),
        LedgerDetail::Sku(sku) => sku.clone(),
        LedgerDetail::Link(url) => url.clone(),
        LedgerDetail::StreakDay(day) => format!("streak day {day}"),
    }
}

/// A reward template key as words: `daily_win_hard` reads `daily win hard`.
fn humanize(key: &str) -> String {
    key.replace('_', " ")
}

/// `1 ticket`, `3 tickets`.
fn plural(count: i64, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// `1234567` as `1,234,567`, with the sign kept in front.
pub(crate) fn thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    if value < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}

fn signed(value: i64) -> String {
    if value > 0 {
        format!("+{}", thousands(value))
    } else {
        thousands(value)
    }
}

/// The header above the rows: the balance and this month's board figure.
pub(crate) fn summary_line(balance: Option<i64>, earned_month: i64) -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let value = Style::default().fg(theme::TEXT_BRIGHT());
    let mut spans = Vec::new();
    if let Some(balance) = balance {
        spans.push(Span::styled("balance ", dim));
        spans.push(Span::styled(thousands(balance), value));
        spans.push(Span::styled(
            "  ·  ",
            Style::default().fg(theme::BORDER_DIM()),
        ));
    }
    spans.push(Span::styled("this month ", dim));
    spans.push(Span::styled(
        signed(earned_month),
        delta_style(earned_month),
    ));
    Line::from(spans)
}

/// What a dim row means, under the summary.
pub(crate) fn off_board_note() -> Line<'static> {
    Line::from(Span::styled(
        "dim rows do not count for Top Chips".to_string(),
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

fn delta_style(delta: i64) -> Style {
    match delta.signum() {
        1 => Style::default().fg(theme::SUCCESS()),
        -1 => Style::default().fg(theme::ERROR()),
        _ => Style::default().fg(theme::TEXT_DIM()),
    }
}

/// One ledger row, clipped to `width`.
pub(crate) fn row_line(row: &LedgerRow, width: usize) -> Line<'static> {
    let entry = &row.entry;
    let dim = Style::default().fg(theme::TEXT_DIM());
    let (label, counts) = match entry.chip_move() {
        Some(mv) => (label(mv), mv.counts_as_earnings()),
        None => ("other", true),
    };
    let text_style = if counts {
        Style::default().fg(theme::TEXT())
    } else {
        dim
    };
    let delta_style = if counts {
        delta_style(entry.delta).add_modifier(Modifier::BOLD)
    } else {
        dim
    };

    let mut spans = vec![
        Span::styled(
            format!("{:<DATE_WIDTH$}", short_date(entry.created_at)),
            dim,
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:>DELTA_WIDTH$}", signed(entry.delta)),
            delta_style,
        ),
        Span::raw("  "),
        Span::styled(format!("{label:<LABEL_WIDTH$}"), text_style),
    ];
    let used = DATE_WIDTH + 2 + DELTA_WIDTH + 2 + LABEL_WIDTH;
    if let Some(detail) = row.detail.as_ref().map(detail) {
        let room = width.saturating_sub(used + 2);
        if room > 0 {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(clip(&detail, room), text_style));
        }
    }
    Line::from(spans)
}

fn short_date(at: DateTime<Utc>) -> String {
    at.format("%b %d").to_string()
}

fn clip(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    let keep = width.saturating_sub(1);
    let mut out: String = text.chars().take(keep).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "ledger_test.rs"]
mod ledger_test;
