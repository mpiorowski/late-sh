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
        "chess|chess960|battleship|connect4|reversi|checkers|backgammon|briscola|8ball|9ball|snooker"
    );
}

#[test]
fn every_pool_game_is_known_to_be_one() {
    // `is_pool` gates the chat floor, the move-count wording, the claim and
    // the whole shot channel — and, on the board, every key and every click.
    // Snooker shipped for one commit with a hand-written list that had not
    // heard of it, and the board was completely dead. The roster and the
    // ruleset have to agree about which games are pool.
    use crate::app::games::pool_core::rules::PoolRules;

    let claimed: Vec<DailyGame> = DailyGame::ALL
        .into_iter()
        .filter(|game| game.is_pool())
        .collect();
    assert_eq!(
        claimed.len(),
        PoolRules::ALL.len(),
        "one roster entry per ruleset: {claimed:?}"
    );
    for game in claimed {
        assert!(
            game.label().len() > 3,
            "{game:?} needs a name a player can type"
        );
    }
    // And nothing else claims to be pool.
    assert!(!DailyGame::Chess.is_pool());
    assert!(!DailyGame::Briscola.is_pool());
}
