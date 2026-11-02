use uuid::Uuid;

use super::{Inputs, Item, Match, items};

fn quiet() -> Inputs {
    Inputs {
        my_moves: Vec::new(),
        results: Vec::new(),
        dms: Vec::new(),
        mentions: Vec::new(),
        tank_due: false,
        tree_due: false,
        pet_due: false,
    }
}

#[test]
fn nothing_waiting_is_an_empty_list_so_the_guest_stays_down() {
    assert!(items(quiet()).is_empty());
}

#[test]
fn the_list_puts_your_moves_first_then_results_people_and_the_companions() {
    let dm = Item::Dm {
        room_id: Uuid::nil(),
        peer: "ana".to_string(),
        unread: 3,
    };
    let mention = Item::Mention {
        room_id: Uuid::nil(),
        message_id: Uuid::nil(),
        actor: "bo".to_string(),
        room: Some("lounge".to_string()),
    };
    let listed = items(Inputs {
        my_moves: vec![Match {
            game: "chess",
            opponent: "mat".to_string(),
        }],
        results: vec![Match {
            game: "battleship",
            opponent: "lee".to_string(),
        }],
        dms: vec![dm.clone()],
        mentions: vec![mention.clone()],
        tank_due: true,
        tree_due: false,
        pet_due: true,
    });
    assert_eq!(
        listed,
        vec![
            Item::YourMove {
                game: "chess",
                opponent: "mat".to_string(),
            },
            Item::Result {
                game: "battleship",
                opponent: "lee".to_string(),
            },
            dm,
            mention,
            Item::FeedTank,
            Item::PetPet,
        ]
    );
    let words: Vec<String> = listed.iter().map(Item::label).collect();
    assert_eq!(
        words,
        vec![
            "your move: chess vs mat",
            "result in: battleship vs lee",
            "@ana: 3 unread",
            "@bo mentioned you in #lounge",
            "the tank is unfed today",
            "the pet is unpetted today",
        ]
    );
    let acts: Vec<&str> = listed.iter().map(Item::act).collect();
    assert_eq!(acts, vec!["play", "look", "read", "read", "feed", "pet"]);
}

#[test]
fn a_single_unread_dm_and_a_nameless_room_read_in_words() {
    let dm = Item::Dm {
        room_id: Uuid::nil(),
        peer: "ana".to_string(),
        unread: 1,
    };
    assert_eq!(dm.label(), "@ana: one unread");
    let mention = Item::Mention {
        room_id: Uuid::nil(),
        message_id: Uuid::nil(),
        actor: "bo".to_string(),
        room: None,
    };
    assert_eq!(mention.label(), "@bo mentioned you");
    assert_eq!(Item::WaterTree.label(), "the tree is unwatered today");
    assert_eq!(Item::WaterTree.act(), "water");
}
