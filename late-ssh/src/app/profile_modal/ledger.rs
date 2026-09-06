//! The chips section of a profile: one plain-English line per ledger row.
//!
//! The ledger is public, so this is where anyone can audit a place on the
//! Top Chips board: every row says what it paid for, and rows the board
//! ignores are marked. Every `ChipMove` gets a label and a decision about
//! what its `source_ref` means to a reader, both exhaustive, so a new reason
//! cannot ship without copy.

use chrono::{DateTime, Utc};
use late_core::models::chat_message_gild::GildParties;
use late_core::models::chips::{ChipLedgerEntry, ChipMove};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use uuid::Uuid;

use crate::app::common::theme;

/// Column widths for a row: `Sep 06  +1,333  gild received  <detail>`.
const DATE_WIDTH: usize = 6;
const DELTA_WIDTH: usize = 8;
const LABEL_WIDTH: usize = 18;
/// Trailing marker for a row the Top Chips board does not count.
const OFF_BOARD: &str = "off";

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

/// What the row's `source_ref` means to a reader, if anything. Most refs
/// are ids that only the database cares about; the ones a person can read
/// (a drink, a SKU, a link, the other side of a gift or a gild) are shown.
/// `username` resolves a user id; `gild` resolves a gild row's ref to its
/// author and buyer.
pub(crate) fn detail(
    mv: ChipMove,
    source_ref: Option<&str>,
    username: impl Fn(Uuid) -> Option<String>,
    gild: impl Fn(Uuid) -> Option<GildParties>,
) -> Option<String> {
    let source_ref = source_ref?;
    let name = |id: Uuid| username(id).map(|name| format!("@{name}"));
    match mv {
        ChipMove::GiftSent => {
            let recipient: Uuid = source_ref.parse().ok()?;
            Some(format!("to {}", name(recipient)?))
        }
        ChipMove::GiftReceived => {
            let sender: Uuid = source_ref.parse().ok()?;
            Some(format!("from {}", name(sender)?))
        }
        ChipMove::GildSent => {
            let message_id: Uuid = source_ref.parse().ok()?;
            Some(format!("to {}", name(gild(message_id)?.author_user_id)?))
        }
        // One buyer per gild ref. Rows written before the ref became the
        // gild id carry the message id instead and resolve to every buyer of
        // that message, so the list is the honest answer for them.
        ChipMove::GildReceived => {
            let message_id: Uuid = source_ref.parse().ok()?;
            let buyers: Vec<String> = gild(message_id)?
                .buyer_user_ids
                .iter()
                .filter_map(|id| name(*id))
                .collect();
            if buyers.is_empty() {
                None
            } else {
                Some(format!("from {}", buyers.join(", ")))
            }
        }
        ChipMove::DrinkPurchase | ChipMove::ShopPurchase | ChipMove::NewsShared => {
            Some(source_ref.to_string())
        }
        ChipMove::DailyQuestStreakReward => Some(format!("streak day {source_ref}")),
        ChipMove::LegacyTableCredit
        | ChipMove::LegacyTableDebit
        | ChipMove::BlackjackBet
        | ChipMove::BlackjackPayout
        | ChipMove::PokerBet
        | ChipMove::PokerPayout
        | ChipMove::BonsaiWatered
        | ChipMove::FloorRestore
        | ChipMove::InitialBalance
        | ChipMove::CrownTaken
        | ChipMove::PotTicket
        | ChipMove::PotWon
        | ChipMove::ArtboardPrize
        | ChipMove::SongQueued
        | ChipMove::RoundPurchase
        | ChipMove::QuestReward
        | ChipMove::DailyPuzzleWin
        | ChipMove::AsterionEscape
        | ChipMove::DailyChessWin
        | ChipMove::DailyChess960Win
        | ChipMove::DailyBattleshipWin
        | ChipMove::DailyConnectFourWin
        | ChipMove::DailyReversiWin
        | ChipMove::DailyCheckersWin
        | ChipMove::DailyBackgammonWin
        | ChipMove::DailyBriscolaWin
        | ChipMove::TronWin
        | ChipMove::SsnakeArenaEarned
        | ChipMove::SsnakeArenaLost
        | ChipMove::GreendragonDragonSlain
        | ChipMove::DarkroomEscape
        | ChipMove::DarkroomBeaconEscape
        | ChipMove::NethackAmuletAcquired
        | ChipMove::NethackAscension
        | ChipMove::DcssOrbFound
        | ChipMove::DcssOrbEscape
        | ChipMove::BrogueEscape
        | ChipMove::BrogueMastery
        | ChipMove::LateaniaArchdemonDefeat
        | ChipMove::LateaniaFrontierKingDefeat
        | ChipMove::LateaniaSunderingDeepDefeat
        | ChipMove::LateaniaKaethyrAscendantDefeat => None,
    }
}

/// `1234567` as `1,234,567`, with the sign kept in front.
pub(crate) fn thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
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

/// What the marker on a row means, under the summary.
pub(crate) fn off_board_note() -> Line<'static> {
    Line::from(Span::styled(
        format!("rows marked {OFF_BOARD} do not count for Top Chips"),
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
pub(crate) fn row_line(
    entry: &ChipLedgerEntry,
    width: usize,
    username: impl Fn(Uuid) -> Option<String>,
    gild: impl Fn(Uuid) -> Option<GildParties>,
) -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let (label, detail, counts) = match entry.chip_move() {
        Some(mv) => (
            label(mv),
            detail(mv, entry.source_ref.as_deref(), username, gild),
            mv.counts_as_earnings(),
        ),
        None => ("other", None, true),
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
    let tail = if counts { 0 } else { OFF_BOARD.len() + 2 };
    if let Some(detail) = detail {
        let room = width.saturating_sub(used + 2 + tail);
        if room > 0 {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(clip(&detail, room), text_style));
        }
    }
    if !counts {
        spans.push(Span::styled(format!("  {OFF_BOARD}"), dim));
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
