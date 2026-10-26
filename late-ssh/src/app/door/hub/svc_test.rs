use late_core::{
    models::{
        door_opening::DoorCard,
        leaderboard::{DayGame, DayTally, DoorGame, day_tallies},
    },
    test_utils::{create_test_user, test_db},
};

use super::svc::GamesHubService;
use crate::app::door::hub::state::HubGame;

/// An opening recorded through the service is a door opened in the day's
/// tallies, and every hub card maps to the stored card it is: the two
/// info-only server cards to none, the roguelikes to the leaderboard's.
#[tokio::test]
async fn an_opening_is_written_and_every_card_knows_its_key() {
    let test = test_db().await;
    let service = GamesHubService::new(test.db.clone());
    let player = create_test_user(&test.db, "hub-player").await;
    for card in [DoorCard::Brogue, DoorCard::Brogue, DoorCard::Zork] {
        service.record(player.id, card).await.expect("record");
    }

    let client = test.db.get().await.expect("db client");
    let doors: Vec<DayTally> = day_tallies(&client, chrono::Utc::now().date_naive())
        .await
        .expect("day")
        .into_iter()
        .filter(|tally| matches!(tally.game, DayGame::Door(_)))
        .collect();
    assert_eq!(
        doors,
        vec![
            DayTally {
                game: DayGame::Door(DoorCard::Brogue),
                plays: 2,
                won: 0,
            },
            DayTally {
                game: DayGame::Door(DoorCard::Zork),
                plays: 1,
                won: 0,
            },
        ]
    );

    let cards: Vec<Option<DoorCard>> = HubGame::ALL.iter().map(|game| game.card()).collect();
    assert_eq!(
        cards,
        vec![
            Some(DoorCard::NightCity),
            Some(DoorCard::Lateania),
            Some(DoorCard::Realm),
            Some(DoorCard::Dcss),
            Some(DoorCard::Nethack),
            Some(DoorCard::Brogue),
            Some(DoorCard::Darkroom),
            Some(DoorCard::Zork),
            None,
            None,
            Some(DoorCard::GreenDragon),
            Some(DoorCard::Usurper),
            Some(DoorCard::Dopewars),
            Some(DoorCard::Bashquest),
            Some(DoorCard::Rebels),
            Some(DoorCard::Codekeep),
        ],
        "the selector's order, the two server cards in the middle"
    );
    for game in DoorGame::ALL {
        let hub = match game {
            DoorGame::Dcss => HubGame::Dcss,
            DoorGame::Nethack => HubGame::Nethack,
            DoorGame::Brogue => HubGame::Brogue,
        };
        assert_eq!(hub.card(), Some(game.card()));
    }
}
