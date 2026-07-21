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
