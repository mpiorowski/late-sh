use crate::app::lobby::daily::games::*;

#[test]
fn kinds_round_trip() {
    for game in DailyGame::ALL {
        assert_eq!(DailyGame::from_kind(game.kind()), Some(game));
        assert_eq!(DailyGame::from_label(game.label()), Some(game));
    }
    assert_eq!(DailyGame::from_kind("duel_snake"), None);
    assert_eq!(
        DailyGame::from_label("BATTLESHIP"),
        Some(DailyGame::Battleship)
    );
}

#[test]
fn usage_lists_every_game() {
    assert_eq!(
        DailyGame::usage_labels(),
        "chess|battleship|connect4|reversi|checkers|backgammon"
    );
}
