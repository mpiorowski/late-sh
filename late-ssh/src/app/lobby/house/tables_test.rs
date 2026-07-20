use crate::app::lobby::house::ssnake::settings::SsnakeSpeed;
use crate::app::lobby::house::tron::settings::{TronMode, TronSpeed};
use crate::app::lobby::house::tables::*;

#[test]
fn table_ids_are_distinct() {
    for a in HouseTable::ALL {
        for b in HouseTable::ALL {
            if a != b {
                assert_ne!(a.table_id(), b.table_id());
            }
        }
    }
}

#[test]
fn chat_slugs_round_trip() {
    for table in HouseTable::ALL {
        assert_eq!(HouseTable::from_chat_slug(table.chat_slug()), Some(table));
    }
    assert_eq!(HouseTable::from_chat_slug("chess"), None);
}

#[test]
fn fixed_settings_match_the_locked_decisions() {
    let poker = HouseTable::poker_settings();
    assert_eq!(poker.starting_stack(), 1_000);
    assert_eq!(poker.small_blind(), 10);
    assert_eq!(poker.big_blind(), 20);

    let blackjack = HouseTable::blackjack_settings();
    assert_eq!(blackjack.min_bet(), 10);

    let tron = HouseTable::tron_settings();
    assert_eq!(tron.speed, TronSpeed::Quick);
    assert_eq!(tron.mode, TronMode::Glitch);

    let ssnake = HouseTable::ssnake_settings();
    assert_eq!(ssnake.speed, SsnakeSpeed::Relaxed);
    assert_eq!(ssnake.level, None);
    assert_eq!(ssnake.seats, 4);
}
