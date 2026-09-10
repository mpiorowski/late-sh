//! What a ledger row's `source_ref` means to a reader, resolved.
//!
//! A ledger row is a pointer: `reason` says what happened, `source_ref` names
//! the row it happened to. Most refs are ids only the database cares about.
//! The ones a person can read (the other side of a gift or gild, the game a
//! payout was for, who lost the crown, how many pot tickets, the quest, the
//! gallery place, the round's size, the song, a drink, a SKU, a link) are
//! followed here into a
//! [`LedgerDetail`], a closed enum the profile modal turns into copy. One
//! arm per [`ChipMove`] decides what its ref points at, so a new reason
//! cannot ship without saying so.
//!
//! This module is pure: the service loads the referenced rows and hands them
//! in as [`LedgerSources`].

use std::collections::HashMap;

use late_core::models::chat_message_gild::GildParties;
use late_core::models::chips::{ChipLedgerEntry, ChipMove};
use late_core::models::drink_round::DrinkRound;
use late_core::models::game_payout::GamePayoutSource;
use late_core::models::pot::Pot;
use late_core::models::profile_award::ProfileAward;
use uuid::Uuid;

/// The readable side of a ledger row. Usernames are resolved; game keys are
/// the reward template's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LedgerDetail {
    GiftTo {
        username: String,
    },
    GiftFrom {
        username: String,
    },
    GildTo {
        username: String,
    },
    /// One buyer per gild ref. Rows written before the ref became the gild
    /// id carry the message id and resolve to every buyer of that message.
    GildFrom {
        usernames: Vec<String>,
    },
    GamePayout {
        game: String,
        payout_kind: String,
    },
    /// Who lost the crown to this take. Absent on the first take ever.
    CrownFrom {
        username: String,
    },
    /// How many tickets one buy bought: the row's chips over the pot's price.
    PotTickets {
        count: i64,
    },
    /// How many tickets were in the pot that paid out.
    PotWon {
        tickets: i64,
    },
    Quest {
        title: String,
    },
    GalleryPlace {
        rank: i32,
        month: chrono::NaiveDate,
    },
    /// How many patrons a round reached: the row's chips over the price.
    RoundFor {
        patrons: i64,
    },
    Song {
        title: String,
    },
    Drink(String),
    Sku(String),
    Link(String),
    StreakDay(String),
}

/// A ledger row and its resolved detail, if the ref meant anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerRow {
    pub entry: ChipLedgerEntry,
    pub detail: Option<LedgerDetail>,
}

/// The rows the ledger's refs point at, loaded by the service.
#[derive(Clone, Debug, Default)]
pub struct LedgerSources {
    pub gilds: HashMap<Uuid, GildParties>,
    pub payouts: HashMap<Uuid, GamePayoutSource>,
    /// Reign id to the user it took the crown from.
    pub deposed: HashMap<Uuid, Uuid>,
    pub pots: HashMap<Uuid, Pot>,
    /// Assignment id to the quest's title.
    pub quests: HashMap<Uuid, String>,
    pub awards: HashMap<Uuid, ProfileAward>,
    pub rounds: HashMap<Uuid, DrinkRound>,
    /// YouTube video id to its title.
    pub songs: HashMap<String, String>,
    pub usernames: HashMap<Uuid, String>,
}

/// Which side of a transfer the ref names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    To,
    From,
}

/// What a reason's `source_ref` points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pointer {
    /// The other user of a gift.
    Counterparty(Side),
    /// A `chat_message_gilds` row (or, for old rows, the gilded message).
    Gild(Side),
    /// A `game_payout_claims` row.
    PayoutClaim,
    /// A `crown_reigns` row.
    Reign,
    /// A `pots` row, from the ticket side or the payout side.
    PotTicket,
    PotWon,
    /// A `quest_assignments` row.
    QuestAssignment,
    /// A `profile_awards` row.
    Award,
    /// A `drink_rounds` row.
    Round,
    /// A YouTube video id, not a row id.
    Video,
    Drink,
    Sku,
    Link,
    StreakDay,
    /// An id only the database cares about.
    Opaque,
}

const fn pointer(mv: ChipMove) -> Pointer {
    match mv {
        ChipMove::GiftSent => Pointer::Counterparty(Side::To),
        ChipMove::GiftReceived => Pointer::Counterparty(Side::From),
        ChipMove::GildSent => Pointer::Gild(Side::To),
        ChipMove::GildReceived => Pointer::Gild(Side::From),
        ChipMove::DrinkPurchase => Pointer::Drink,
        ChipMove::ShopPurchase => Pointer::Sku,
        ChipMove::NewsShared => Pointer::Link,
        ChipMove::DailyQuestStreakReward => Pointer::StreakDay,
        ChipMove::CrownTaken => Pointer::Reign,
        ChipMove::PotTicket => Pointer::PotTicket,
        ChipMove::PotWon => Pointer::PotWon,
        ChipMove::QuestReward => Pointer::QuestAssignment,
        ChipMove::ArtboardPrize => Pointer::Award,
        ChipMove::RoundPurchase => Pointer::Round,
        ChipMove::SongQueued => Pointer::Video,
        ChipMove::DailyPuzzleWin
        | ChipMove::AsterionEscape
        | ChipMove::DailyChessWin
        | ChipMove::DailyChess960Win
        | ChipMove::DailyBattleshipWin
        | ChipMove::DailyConnectFourWin
        | ChipMove::DailyReversiWin
        | ChipMove::DailyCheckersWin
        | ChipMove::DailyBackgammonWin
        | ChipMove::DailyBriscolaWin
        | ChipMove::DailyEightBallWin
        | ChipMove::DailyNineBallWin
        | ChipMove::DailySnookerWin
        | ChipMove::TronWin
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
        | ChipMove::LateaniaKaethyrAscendantDefeat => Pointer::PayoutClaim,
        ChipMove::LegacyTableCredit
        | ChipMove::LegacyTableDebit
        | ChipMove::BlackjackBet
        | ChipMove::BlackjackPayout
        | ChipMove::PokerBet
        | ChipMove::PokerPayout
        | ChipMove::BonsaiWatered
        | ChipMove::PetFed
        | ChipMove::AquariumFed
        | ChipMove::FloorRestore
        | ChipMove::InitialBalance
        | ChipMove::SsnakeArenaEarned
        | ChipMove::SsnakeArenaLost => Pointer::Opaque,
    }
}

/// The ids the ledger points at, bucketed by the table they live in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LedgerRefs {
    /// User ids: the other side of each gift.
    pub counterparties: Vec<Uuid>,
    /// Gild refs: gild row ids, or message ids on old rows.
    pub gilds: Vec<Uuid>,
    /// `game_payout_claims` ids.
    pub payouts: Vec<Uuid>,
    /// `crown_reigns` ids.
    pub reigns: Vec<Uuid>,
    /// `pots` ids, from tickets and payouts alike.
    pub pots: Vec<Uuid>,
    /// `quest_assignments` ids.
    pub quests: Vec<Uuid>,
    /// `profile_awards` ids.
    pub awards: Vec<Uuid>,
    /// `drink_rounds` ids.
    pub rounds: Vec<Uuid>,
    /// YouTube video ids.
    pub videos: Vec<String>,
}

/// What the service has to load before [`resolve`] can say anything.
pub fn refs(entries: &[ChipLedgerEntry]) -> LedgerRefs {
    let mut refs = LedgerRefs::default();
    for entry in entries {
        let Some(mv) = entry.chip_move() else {
            continue;
        };
        let Some(source_ref) = entry.source_ref.as_deref() else {
            continue;
        };
        let pointer = pointer(mv);
        if pointer == Pointer::Video {
            refs.videos.push(source_ref.to_string());
            continue;
        }
        let Ok(id) = source_ref.parse::<Uuid>() else {
            continue;
        };
        match pointer {
            Pointer::Counterparty(_) => refs.counterparties.push(id),
            Pointer::Gild(_) => refs.gilds.push(id),
            Pointer::PayoutClaim => refs.payouts.push(id),
            Pointer::Reign => refs.reigns.push(id),
            Pointer::PotTicket | Pointer::PotWon => refs.pots.push(id),
            Pointer::QuestAssignment => refs.quests.push(id),
            Pointer::Award => refs.awards.push(id),
            Pointer::Round => refs.rounds.push(id),
            Pointer::Video
            | Pointer::Drink
            | Pointer::Sku
            | Pointer::Link
            | Pointer::StreakDay
            | Pointer::Opaque => {}
        }
    }
    refs
}

/// Every user the loaded gilds and reigns name, on top of the gift
/// counterparties, so the service can load their names in one query.
pub fn named_user_ids(
    refs: &LedgerRefs,
    gilds: &HashMap<Uuid, GildParties>,
    deposed: &HashMap<Uuid, Uuid>,
) -> Vec<Uuid> {
    let mut ids = refs.counterparties.clone();
    for parties in gilds.values() {
        ids.push(parties.author_user_id);
        ids.extend(parties.buyer_user_ids.iter().copied());
    }
    ids.extend(deposed.values().copied());
    ids
}

/// Pair every entry with what its ref resolves to.
pub fn resolve(entries: Vec<ChipLedgerEntry>, sources: &LedgerSources) -> Vec<LedgerRow> {
    entries
        .into_iter()
        .map(|entry| {
            let detail = detail(&entry, sources);
            LedgerRow { entry, detail }
        })
        .collect()
}

fn detail(entry: &ChipLedgerEntry, sources: &LedgerSources) -> Option<LedgerDetail> {
    let mv = entry.chip_move()?;
    let source_ref = entry.source_ref.as_deref()?;
    let username = |id: Uuid| sources.usernames.get(&id).cloned();
    match pointer(mv) {
        Pointer::Counterparty(side) => {
            let other: Uuid = source_ref.parse().ok()?;
            let username = username(other)?;
            match side {
                Side::To => Some(LedgerDetail::GiftTo { username }),
                Side::From => Some(LedgerDetail::GiftFrom { username }),
            }
        }
        Pointer::Gild(side) => {
            let gild_ref: Uuid = source_ref.parse().ok()?;
            let parties = sources.gilds.get(&gild_ref)?;
            match side {
                Side::To => Some(LedgerDetail::GildTo {
                    username: username(parties.author_user_id)?,
                }),
                Side::From => {
                    let usernames: Vec<String> = parties
                        .buyer_user_ids
                        .iter()
                        .filter_map(|id| username(*id))
                        .collect();
                    if usernames.is_empty() {
                        None
                    } else {
                        Some(LedgerDetail::GildFrom { usernames })
                    }
                }
            }
        }
        Pointer::PayoutClaim => {
            let claim_id: Uuid = source_ref.parse().ok()?;
            let source = sources.payouts.get(&claim_id)?;
            Some(LedgerDetail::GamePayout {
                game: source.game.clone(),
                payout_kind: source.payout_kind.clone(),
            })
        }
        Pointer::Reign => {
            let reign_id: Uuid = source_ref.parse().ok()?;
            let deposed = *sources.deposed.get(&reign_id)?;
            Some(LedgerDetail::CrownFrom {
                username: username(deposed)?,
            })
        }
        Pointer::PotTicket => {
            let pot_id: Uuid = source_ref.parse().ok()?;
            let pot = sources.pots.get(&pot_id)?;
            Some(LedgerDetail::PotTickets {
                count: entry.delta.abs() / pot.ticket_price,
            })
        }
        Pointer::PotWon => {
            let pot_id: Uuid = source_ref.parse().ok()?;
            let tickets = sources.pots.get(&pot_id)?.ticket_count?;
            Some(LedgerDetail::PotWon { tickets })
        }
        Pointer::QuestAssignment => {
            let assignment_id: Uuid = source_ref.parse().ok()?;
            Some(LedgerDetail::Quest {
                title: sources.quests.get(&assignment_id)?.clone(),
            })
        }
        Pointer::Award => {
            let award_id: Uuid = source_ref.parse().ok()?;
            let award = sources.awards.get(&award_id)?;
            Some(LedgerDetail::GalleryPlace {
                rank: award.rank,
                month: award.period_month,
            })
        }
        Pointer::Round => {
            let round_id: Uuid = source_ref.parse().ok()?;
            let round = sources.rounds.get(&round_id)?;
            Some(LedgerDetail::RoundFor {
                patrons: entry.delta.abs() / round.price_per_patron,
            })
        }
        Pointer::Video => Some(LedgerDetail::Song {
            title: sources.songs.get(source_ref)?.clone(),
        }),
        Pointer::Drink => Some(LedgerDetail::Drink(source_ref.to_string())),
        Pointer::Sku => Some(LedgerDetail::Sku(source_ref.to_string())),
        Pointer::Link => Some(LedgerDetail::Link(source_ref.to_string())),
        Pointer::StreakDay => Some(LedgerDetail::StreakDay(source_ref.to_string())),
        Pointer::Opaque => None,
    }
}

#[cfg(test)]
#[path = "ledger_test.rs"]
mod ledger_test;
