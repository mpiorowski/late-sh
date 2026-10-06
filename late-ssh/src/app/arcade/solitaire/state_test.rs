use super::*;

fn test_state() -> State {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
    State::new(
        Uuid::nil(),
        SolitaireService::new(db, tokio::sync::broadcast::channel(4).0),
        Vec::new(),
    )
}

#[test]
fn reset_confirmation_is_per_action_kind() {
    let mut state = test_state();

    // Two presses of the same key confirm and fire.
    assert!(!state.request_reset(ResetKind::Reset));
    assert!(state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, None);

    // A press for a different kind re-arms for that kind instead of
    // firing the originally-armed action.
    assert!(!state.request_reset(ResetKind::NewBoard));
    assert!(!state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, Some(ResetKind::Reset));
    assert!(state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, None);
}

#[test]
fn seeded_deal_uses_full_deck() {
    let snapshot = snapshot_from_seed(42);
    let count = snapshot.stock.len()
        + snapshot.waste.len()
        + snapshot.foundations.iter().map(Vec::len).sum::<usize>()
        + snapshot.tableau.iter().map(Vec::len).sum::<usize>();
    assert_eq!(count, 52);
    assert_eq!(snapshot.stock.len(), 24);
}

#[test]
fn draw_one_draws_one_card() {
    let mut stock = vec![
        Card {
            suit: Suit::Hearts,
            rank: 1,
        },
        Card {
            suit: Suit::Spades,
            rank: 13,
        },
    ];
    let mut waste = Vec::new();
    assert!(draw_stock_once(&mut stock, &mut waste, 1));
    assert_eq!(stock.len(), 1);
    assert_eq!(waste.len(), 1);
}

#[test]
fn draw_three_draws_up_to_three_cards() {
    let mut stock = vec![
        Card {
            suit: Suit::Hearts,
            rank: 1,
        },
        Card {
            suit: Suit::Spades,
            rank: 13,
        },
        Card {
            suit: Suit::Clubs,
            rank: 7,
        },
        Card {
            suit: Suit::Diamonds,
            rank: 10,
        },
    ];
    let mut waste = Vec::new();
    assert!(draw_stock_once(&mut stock, &mut waste, 3));
    assert_eq!(stock.len(), 1);
    assert_eq!(waste.len(), 3);
    assert_eq!(waste.last().map(|card| card.rank), Some(13));
}

#[test]
fn moving_from_tableau_reveals_next_card() {
    let mut state = test_state();
    state.tableau[0] = vec![TableauCard {
        card: Card {
            suit: Suit::Clubs,
            rank: 8,
        },
        face_up: true,
    }];
    state.tableau[1] = vec![
        TableauCard {
            card: Card {
                suit: Suit::Hearts,
                rank: 8,
            },
            face_up: false,
        },
        TableauCard {
            card: Card {
                suit: Suit::Hearts,
                rank: 7,
            },
            face_up: true,
        },
    ];

    assert!(state.try_move(Selection::Tableau { col: 1, row: 1 }, Focus::Tableau(0, 0)));
    assert!(state.tableau[1][0].face_up);
}

#[test]
fn ace_can_move_to_matching_foundation() {
    let mut state = test_state();
    state.waste = vec![Card {
        suit: Suit::Spades,
        rank: 1,
    }];
    assert!(state.try_move(Selection::Waste, Focus::Foundation(3)));
}

/// One card short of won, sitting face-up in the tableau. Shared with
/// `ui_test.rs`, which needs a board that can be won on one key.
pub(crate) fn almost_won_state() -> State {
    let mut state = test_state();
    state.stock.clear();
    state.waste.clear();
    state.tableau = std::array::from_fn(|_| Vec::new());
    for (idx, suit) in [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades]
        .into_iter()
        .enumerate()
    {
        state.foundations[idx] = (1..=13).map(|rank| Card { suit, rank }).collect();
    }
    let last = state.foundations[3].pop().expect("a full spade pile");
    state.tableau[0] = vec![TableauCard {
        card: last,
        face_up: true,
    }];
    state.cursor = Focus::Tableau(0, 0);
    state
}

#[tokio::test]
async fn the_last_card_starts_the_cascade() {
    let mut state = almost_won_state();
    assert!(state.win_anim.is_none());
    assert!(state.auto_move());
    assert!(state.is_game_over);
    assert!(state.win_cascade_running());
}

/// The win card is held back until the cards land, so the cascade is not
/// drawn through a box.
#[tokio::test]
async fn a_key_press_runs_the_cascade_out() {
    let mut state = almost_won_state();
    state.auto_move();
    assert!(state.win_cascade_running());
    assert!(state.skip_win_animation());
    assert!(!state.win_cascade_running());
    // Nothing left to skip: the next key press is the player's again.
    assert!(!state.skip_win_animation());
}

/// An undo after the win, then the last card replayed, is not a second
/// win: the finish is counted once and the cascade does not run again.
#[tokio::test]
async fn a_win_undone_and_replayed_is_not_a_second_finish() {
    let mut state = almost_won_state();
    assert!(state.auto_move());
    assert!(state.is_game_over);
    assert!(state.win_cascade_running());

    assert!(state.undo());
    assert!(!state.is_game_over);
    assert!(!state.win_cascade_running());

    assert!(state.auto_move());
    assert!(state.is_game_over);
    assert!(
        !state.win_cascade_running(),
        "the first crossing was already counted"
    );
}

/// Dealing again clears the heap; a won board reloaded from its snapshot
/// shows the win card without replaying the cascade.
#[tokio::test]
async fn a_new_deal_clears_the_cascade() {
    let mut state = almost_won_state();
    state.auto_move();
    assert!(state.win_cascade_running());
    state.reroll();
    assert!(state.win_anim.is_none());
    assert!(!state.is_game_over);
}

/// The piles empty as they throw: a king already in the air must not also be
/// sitting on its foundation.
#[tokio::test]
async fn the_piles_empty_as_the_cascade_throws() {
    let mut state = almost_won_state();
    state.auto_move();
    assert_eq!(
        state.displayed_foundation_top(3).map(|card| card.rank),
        Some(13)
    );

    for _ in 0..30 {
        state.tick_win_animation();
    }
    let top = state.displayed_foundation_top(3).map(|card| card.rank);
    assert!(
        top.is_none_or(|rank| rank < 13),
        "the spade pile still shows its king: {top:?}"
    );

    // Once the cascade is gone the piles are whole again, so a won board
    // reopened from its snapshot reads as won.
    state.win_anim = None;
    assert_eq!(
        state.displayed_foundation_top(3).map(|card| card.rank),
        Some(13)
    );
}
