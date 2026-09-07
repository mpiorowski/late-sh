use std::collections::HashMap;

use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::chat_message_gild::GildParties;
use late_core::models::chips::ChipLedgerEntry;
use late_core::models::drink_round::DrinkRound;
use late_core::models::game_payout::GamePayoutSource;
use late_core::models::pot::{Pot, PotStatus};
use late_core::models::profile_award::ProfileAward;
use uuid::Uuid;

use super::*;

fn entry(delta: i64, reason: &str, source_ref: Option<&str>) -> ChipLedgerEntry {
    ChipLedgerEntry {
        delta,
        reason: reason.to_string(),
        source_ref: source_ref.map(str::to_string),
        created_at: Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap(),
    }
}

/// One ledger of every pointer kind, driven through `refs` and `resolve`:
/// the whole result is asserted, so a reason that starts resolving
/// differently shows up here.
#[test]
fn every_pointer_kind_resolves_from_its_source() {
    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();
    let stranger = Uuid::now_v7();
    let gild_id = Uuid::now_v7();
    let claim_id = Uuid::now_v7();
    let missing_claim_id = Uuid::now_v7();
    let reign_id = Uuid::now_v7();
    let first_reign_id = Uuid::now_v7();
    let open_pot_id = Uuid::now_v7();
    let drawn_pot_id = Uuid::now_v7();
    let assignment_id = Uuid::now_v7();
    let award_id = Uuid::now_v7();
    let round_id = Uuid::now_v7();
    let entries = vec![
        entry(-300, "chip_gift_sent", Some(&alice.to_string())),
        entry(300, "chip_gift_received", Some(&stranger.to_string())),
        entry(333, "chip_gild_received", Some(&gild_id.to_string())),
        entry(-500, "chip_gild_sent", Some(&gild_id.to_string())),
        entry(100, "daily_puzzle_win", Some(&claim_id.to_string())),
        entry(400, "daily_chess_win", Some(&missing_claim_id.to_string())),
        entry(-1000, "chip_crown_taken", Some(&reign_id.to_string())),
        entry(-1000, "chip_crown_taken", Some(&first_reign_id.to_string())),
        entry(-150, "pot_ticket", Some(&open_pot_id.to_string())),
        entry(1600, "pot_won", Some(&drawn_pot_id.to_string())),
        entry(-50, "pot_ticket", Some(&drawn_pot_id.to_string())),
        entry(500, "quest_reward", Some(&assignment_id.to_string())),
        entry(20_000, "artboard_prize", Some(&award_id.to_string())),
        entry(-1200, "round_purchase", Some(&round_id.to_string())),
        entry(25, "song_queued", Some("dQw4w9WgXcQ")),
        entry(25, "song_queued", Some("untitled-video")),
        entry(-400, "drink_purchase", Some("Segfault Sour")),
        entry(-900, "shop_purchase", Some("bonsai-dynamic")),
        entry(250, "news_shared", Some("https://example.com/a")),
        entry(150, "daily_quest_streak_reward", Some("7")),
        entry(2400, "poker_payout", Some(&Uuid::now_v7().to_string())),
        entry(5075, "leaderboard_seed", Some("leaderboard-v2")),
        entry(500, "quest_reward", None),
    ];

    let refs = refs(&entries);
    assert_eq!(
        refs,
        LedgerRefs {
            counterparties: vec![alice, stranger],
            gilds: vec![gild_id, gild_id],
            payouts: vec![claim_id, missing_claim_id],
            reigns: vec![reign_id, first_reign_id],
            pots: vec![open_pot_id, drawn_pot_id, drawn_pot_id],
            quests: vec![assignment_id],
            awards: vec![award_id],
            rounds: vec![round_id],
            videos: vec!["dQw4w9WgXcQ".to_string(), "untitled-video".to_string()],
        }
    );

    let pot = |id: Uuid, status: PotStatus, ticket_count: Option<i64>| Pot {
        id,
        opens_at: Utc::now(),
        draws_at: Utc::now(),
        status,
        ticket_price: 50,
        winner_user_id: None,
        ticket_count,
        payout_chips: ticket_count.map(|count| count * 50 * 4 / 5),
        drawn_at: None,
    };
    let sources = LedgerSources {
        gilds: HashMap::from([(
            gild_id,
            GildParties {
                author_user_id: alice,
                buyer_user_ids: vec![bob, stranger],
            },
        )]),
        payouts: HashMap::from([(
            claim_id,
            GamePayoutSource {
                game: "minesweeper".to_string(),
                payout_kind: "daily_win_hard".to_string(),
            },
        )]),
        deposed: HashMap::from([(reign_id, bob)]),
        pots: HashMap::from([
            (open_pot_id, pot(open_pot_id, PotStatus::Open, None)),
            (drawn_pot_id, pot(drawn_pot_id, PotStatus::Drawn, Some(40))),
        ]),
        quests: HashMap::from([(assignment_id, "Water your bonsai".to_string())]),
        awards: HashMap::from([(
            award_id,
            ProfileAward {
                id: award_id,
                user_id: alice,
                category: "artboard".to_string(),
                period_month: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                rank: 1,
                score_value: 4,
                awarded_at: Utc::now(),
            },
        )]),
        rounds: HashMap::from([(
            round_id,
            DrinkRound {
                id: round_id,
                buyer_user_id: Some(alice),
                price_per_patron: 400,
                created: Utc::now(),
            },
        )]),
        songs: HashMap::from([(
            "dQw4w9WgXcQ".to_string(),
            "Never Gonna Give You Up".to_string(),
        )]),
        usernames: HashMap::from([(alice, "alice".to_string()), (bob, "bob".to_string())]),
    };
    assert_eq!(
        named_user_ids(&refs, &sources.gilds, &sources.deposed),
        vec![alice, stranger, alice, bob, stranger, bob]
    );

    let details: Vec<Option<LedgerDetail>> = resolve(entries.clone(), &sources)
        .into_iter()
        .map(|row| row.detail)
        .collect();
    assert_eq!(
        details,
        vec![
            Some(LedgerDetail::GiftTo {
                username: "alice".to_string()
            }),
            // A counterparty whose name did not load says nothing.
            None,
            // The buyer without a name is left out, not shown as a blank.
            Some(LedgerDetail::GildFrom {
                usernames: vec!["bob".to_string()]
            }),
            Some(LedgerDetail::GildTo {
                username: "alice".to_string()
            }),
            Some(LedgerDetail::GamePayout {
                game: "minesweeper".to_string(),
                payout_kind: "daily_win_hard".to_string(),
            }),
            // A claim that did not load says nothing.
            None,
            Some(LedgerDetail::CrownFrom {
                username: "bob".to_string()
            }),
            // The first take ever deposed nobody.
            None,
            Some(LedgerDetail::PotTickets { count: 3 }),
            Some(LedgerDetail::PotWon { tickets: 40 }),
            Some(LedgerDetail::PotTickets { count: 1 }),
            Some(LedgerDetail::Quest {
                title: "Water your bonsai".to_string()
            }),
            Some(LedgerDetail::GalleryPlace {
                rank: 1,
                month: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
            }),
            Some(LedgerDetail::RoundFor { patrons: 3 }),
            Some(LedgerDetail::Song {
                title: "Never Gonna Give You Up".to_string()
            }),
            // A video nobody ever queued with a title says nothing.
            None,
            Some(LedgerDetail::Drink("Segfault Sour".to_string())),
            Some(LedgerDetail::Sku("bonsai-dynamic".to_string())),
            Some(LedgerDetail::Link("https://example.com/a".to_string())),
            Some(LedgerDetail::StreakDay("7".to_string())),
            None,
            None,
            None,
        ]
    );
}
